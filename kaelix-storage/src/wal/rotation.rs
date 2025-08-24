//! WAL segment rotation and lifecycle management
//!
//! This module provides comprehensive segment rotation, sealing, and cleanup capabilities
//! while maintaining <10μs write latency through efficient segment handling.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use tokio::sync::RwLock;
use tokio::time::interval;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use super::{LogEntry, WalError, WalSegment, SegmentMetadata};

/// Segment rotation configuration
#[derive(Debug, Clone)]
pub struct RotationConfig {
    /// WAL directory path
    pub wal_dir: PathBuf,
    /// Maximum segment size in bytes (default: 64MB)
    pub max_segment_size: u64,
    /// Maximum segment age (default: 1 hour)
    pub max_segment_age: Duration,
    /// Maximum entries per segment (default: 1M entries)
    pub max_entries_per_segment: u64,
    /// Retention policy for cleanup
    pub retention_policy: RetentionPolicy,
    /// Cleanup check interval (default: 5 minutes)
    pub cleanup_interval: Duration,
    /// Whether to use memory mapping
    pub use_memory_mapping: bool,
}

impl Default for RotationConfig {
    fn default() -> Self {
        Self {
            wal_dir: PathBuf::from("wal"),
            max_segment_size: 64 * 1024 * 1024, // 64MB
            max_segment_age: Duration::from_secs(3600), // 1 hour
            max_entries_per_segment: 1_000_000,
            retention_policy: RetentionPolicy::ByCount(10),
            cleanup_interval: Duration::from_secs(300), // 5 minutes
            use_memory_mapping: true,
        }
    }
}

/// Retention policy for segment cleanup
#[derive(Debug, Clone)]
pub enum RetentionPolicy {
    /// Keep segments for specified duration
    ByAge(Duration),
    /// Keep last N segments
    ByCount(usize),
    /// Keep segments totaling up to specified size
    BySize(u64),
    /// Never delete segments (for testing)
    Never,
}

/// Statistics for rotation operations
#[derive(Debug, Default, Clone)]
pub struct RotationMetrics {
    pub rotations_total: u64,
    pub rotation_latency_nanos: u64,
    pub segments_created: u64,
    pub segments_sealed: u64,
    pub segments_cleaned: u64,
    pub cleanup_bytes_freed: u64,
    pub average_segment_size: u64,
    pub rotation_triggers: RotationTriggerCounts,
}

impl RotationMetrics {
    /// Record a rotation operation
    pub fn record_rotation(&mut self, latency: Duration) {
        self.rotations_total += 1;
        self.rotation_latency_nanos = latency.as_nanos() as u64;
        self.segments_created += 1;
    }

    /// Record segment sealing
    pub fn record_sealed(&mut self, segment_size: u64) {
        self.segments_sealed += 1;
        // Update running average of segment size
        let total_segments = self.segments_sealed;
        if total_segments > 0 {
            self.average_segment_size = 
                (self.average_segment_size * (total_segments - 1) + segment_size) / total_segments;
        }
    }

    /// Record cleanup operation
    pub fn record_cleanup(&mut self, segments_removed: usize, bytes_freed: u64) {
        self.segments_cleaned += segments_removed as u64;
        self.cleanup_bytes_freed += bytes_freed;
    }

    /// Get average rotation latency in nanoseconds
    pub fn avg_rotation_latency_nanos(&self) -> u64 {
        if self.rotations_total > 0 {
            self.rotation_latency_nanos / self.rotations_total
        } else {
            0
        }
    }
}

/// Counts of different rotation triggers
#[derive(Debug, Default, Clone)]
pub struct RotationTriggerCounts {
    pub size_triggered: u64,
    pub age_triggered: u64,
    pub count_triggered: u64,
    pub manual_triggered: u64,
}

/// Result of cleanup operation
#[derive(Debug)]
pub struct CleanupResult {
    pub segments_removed: usize,
    pub bytes_freed: u64,
    pub segments_remaining: usize,
}

/// Enhanced segment metadata for sealed segments
#[derive(Debug, Clone)]
pub struct SealedSegmentMetadata {
    /// Basic segment metadata
    pub metadata: SegmentMetadata,
    /// When the segment was sealed
    pub sealed_at: SystemTime,
    /// Sparse index for fast lookups
    pub index: SegmentIndex,
}

/// Sparse index for segment lookups
#[derive(Debug, Clone)]
pub struct SegmentIndex {
    /// Sequence number to file offset mapping (sparse)
    offset_index: BTreeMap<u64, u64>,
    /// Timestamp to file offset mapping (sparse)
    time_index: BTreeMap<u64, u64>,
    /// Index bounds for pruning
    bounds: IndexBounds,
}

impl SegmentIndex {
    /// Create new empty index
    pub fn new() -> Self {
        Self {
            offset_index: BTreeMap::new(),
            time_index: BTreeMap::new(),
            bounds: IndexBounds::default(),
        }
    }

    /// Add offset entry to index
    pub fn add_offset_entry(&mut self, sequence: u64, offset: u64) {
        self.offset_index.insert(sequence, offset);
        self.bounds.update_sequence_bounds(sequence);
    }

    /// Add time-based entry to index
    pub fn add_time_entry(&mut self, timestamp: u64, offset: u64) {
        self.time_index.insert(timestamp, offset);
        self.bounds.update_time_bounds(timestamp);
    }

    /// Find closest offset for sequence number
    pub fn find_offset(&self, sequence: u64) -> Option<u64> {
        // Find the entry with sequence <= target
        self.offset_index
            .range(..=sequence)
            .next_back()
            .map(|(_, offset)| *offset)
    }

    /// Find offset by timestamp (approximate)
    pub fn find_offset_by_time(&self, timestamp: u64) -> Option<u64> {
        self.time_index
            .range(..=timestamp)
            .next_back()
            .map(|(_, offset)| *offset)
    }
}

impl Default for SegmentIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// Index bounds for optimization
#[derive(Debug, Clone, Default)]
pub struct IndexBounds {
    pub min_sequence: Option<u64>,
    pub max_sequence: Option<u64>,
    pub min_timestamp: Option<u64>,
    pub max_timestamp: Option<u64>,
}

impl IndexBounds {
    /// Update sequence bounds
    fn update_sequence_bounds(&mut self, sequence: u64) {
        self.min_sequence = Some(self.min_sequence.map_or(sequence, |min| min.min(sequence)));
        self.max_sequence = Some(self.max_sequence.map_or(sequence, |max| max.max(sequence)));
    }

    /// Update time bounds
    fn update_time_bounds(&mut self, timestamp: u64) {
        self.min_timestamp = Some(self.min_timestamp.map_or(timestamp, |min| min.min(timestamp)));
        self.max_timestamp = Some(self.max_timestamp.map_or(timestamp, |max| max.max(timestamp)));
    }
}

/// Sealed segment with read-only access and indexing
#[derive(Debug)]
pub struct SealedSegment {
    /// Segment metadata
    pub metadata: SealedSegmentMetadata,
    /// File path
    pub file_path: PathBuf,
}

impl Clone for SealedSegment {
    fn clone(&self) -> Self {
        Self {
            metadata: self.metadata.clone(),
            file_path: self.file_path.clone(),
        }
    }
}

impl SealedSegment {
    /// Create sealed segment from active segment
    pub async fn from_active_segment(
        segment: WalSegment,
        config: &RotationConfig,
    ) -> Result<Self, WalError> {
        let metadata = segment.metadata();
        let metadata_clone = {
            let metadata_guard = metadata.read().unwrap();
            metadata_guard.clone()
        };
        
        // Build sparse index
        let index = Self::build_segment_index(&segment, &config.wal_dir).await?;
        
        let sealed_metadata = SealedSegmentMetadata {
            metadata: metadata_clone.clone(),
            sealed_at: SystemTime::now(),
            index,
        };

        Ok(Self {
            metadata: sealed_metadata,
            file_path: metadata_clone.path.clone(),
        })
    }

    /// Build sparse index for the segment
    async fn build_segment_index(
        segment: &WalSegment,
        _wal_dir: &Path,
    ) -> Result<SegmentIndex, WalError> {
        let mut index = SegmentIndex::new();
        let mut current_offset = 0;
        let sample_interval = 4096; // Sample every 4KB

        // Get segment metadata for bounds
        let segment_size = {
            let metadata = segment.metadata();
            let metadata_guard = metadata.read().unwrap();
            metadata_guard.size
        };

        while current_offset < segment_size {
            // Read entry header to get sequence and timestamp
            if let Ok(entry) = segment.read_entry(current_offset).await {
                // Add to offset index (sparse sampling)
                if current_offset % sample_interval == 0 {
                    index.add_offset_entry(entry.sequence_number, current_offset);
                }

                // Add to time index every 1000 entries for time-based queries
                if entry.sequence_number % 1000 == 0 {
                    index.add_time_entry(entry.timestamp, current_offset);
                }

                // Move to next entry
                current_offset += entry.serialized_size() as u64;
            } else {
                // If we can't read entry, advance by minimum entry size
                current_offset += 24; // Minimum header size
            }

            // Prevent infinite loop
            if current_offset >= segment_size {
                break;
            }
        }

        Ok(index)
    }

    /// Read entry from sealed segment
    pub async fn read_entry(&self, sequence: u64) -> Result<LogEntry, WalError> {
        // Use index to find approximate position
        let start_offset = self.metadata.index.find_offset(sequence).unwrap_or(0);
        
        // Read from file
        self.read_from_file(sequence, start_offset).await
    }

    /// Read entry from file
    async fn read_from_file(
        &self,
        sequence: u64,
        start_offset: u64,
    ) -> Result<LogEntry, WalError> {
        use std::io::{Read, Seek, SeekFrom};
        
        let mut file = std::fs::File::open(&self.file_path)
            .map_err(|e| WalError::Io(format!("Failed to open segment file: {}", e)))?;
        
        file.seek(SeekFrom::Start(start_offset))
            .map_err(|e| WalError::Io(format!("Failed to seek to offset: {}", e)))?;

        // Scan forward from the indexed position
        loop {
            // Read entry header
            let mut header_buf = [0u8; 24];
            match file.read_exact(&mut header_buf) {
                Ok(_) => {
                    if let Ok(entry) = LogEntry::deserialize_header(&header_buf) {
                        if entry.sequence_number == sequence {
                            // Found the entry, read the payload
                            let mut payload = vec![0u8; entry.payload_length as usize];
                            file.read_exact(&mut payload)
                                .map_err(|e| WalError::Io(format!("Failed to read payload: {}", e)))?;
                            
                            // Combine and deserialize
                            let mut full_data = header_buf.to_vec();
                            full_data.extend_from_slice(&payload);
                            
                            return LogEntry::deserialize(&full_data);
                        }
                        
                        // Skip payload if not the target sequence
                        file.seek(SeekFrom::Current(entry.payload_length as i64))
                            .map_err(|e| WalError::Io(format!("Failed to skip payload: {}", e)))?;
                        
                        // If we've passed the target sequence, it's not found
                        if entry.sequence_number > sequence {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        
        Err(WalError::Io(format!("Entry with sequence {} not found", sequence)))
    }

    /// Read range of entries from sealed segment
    pub async fn read_range(&self, start: u64, end: u64) -> Result<Vec<LogEntry>, WalError> {
        let mut entries = Vec::new();
        
        for sequence in start..=end {
            if let Ok(entry) = self.read_entry(sequence).await {
                entries.push(entry);
            }
        }
        
        Ok(entries)
    }
}

/// Segment rotator managing lifecycle
#[derive(Debug)]
pub struct SegmentRotator {
    /// Configuration
    config: RotationConfig,
    
    /// Active segment for writes (using WalSegment directly to avoid Clone issues)
    active_segment: Arc<RwLock<Option<SegmentMetadata>>>,
    
    /// Sealed segments (read-only)
    sealed_segments: Arc<RwLock<Vec<SealedSegment>>>,
    
    /// Next segment ID generator
    next_segment_id: Arc<AtomicU64>,
    
    /// Metrics
    metrics: Arc<RwLock<RotationMetrics>>,
    
    /// Shutdown flag
    shutdown: Arc<AtomicBool>,
    
    /// Background task handles
    rotation_task: Option<JoinHandle<()>>,
    cleanup_task: Option<JoinHandle<()>>,
}

impl SegmentRotator {
    /// Create new segment rotator
    pub async fn new(config: RotationConfig) -> Result<Self, WalError> {
        // Create WAL directory
        tokio::fs::create_dir_all(&config.wal_dir).await
            .map_err(|e| WalError::Io(format!("Failed to create WAL directory: {}", e)))?;

        let mut rotator = Self {
            config,
            active_segment: Arc::new(RwLock::new(None)),
            sealed_segments: Arc::new(RwLock::new(Vec::new())),
            next_segment_id: Arc::new(AtomicU64::new(1)),
            metrics: Arc::new(RwLock::new(RotationMetrics::default())),
            shutdown: Arc::new(AtomicBool::new(false)),
            rotation_task: None,
            cleanup_task: None,
        };

        // Create initial segment
        rotator.create_new_segment().await?;

        // Start background tasks
        rotator.start_background_tasks().await;

        Ok(rotator)
    }

    /// Start background monitoring and cleanup tasks
    async fn start_background_tasks(&mut self) {
        // Rotation monitoring task - simplified for now without segment references
        let config = self.config.clone();
        let shutdown = Arc::clone(&self.shutdown);

        self.rotation_task = Some(tokio::spawn(async move {
            Self::monitor_rotation_simple(config, shutdown).await;
        }));

        // Cleanup task
        let sealed_segments_cleanup = Arc::clone(&self.sealed_segments);
        let config_cleanup = self.config.clone();
        let shutdown_cleanup = Arc::clone(&self.shutdown);
        let metrics_cleanup = Arc::clone(&self.metrics);

        self.cleanup_task = Some(tokio::spawn(async move {
            Self::cleanup_task(
                sealed_segments_cleanup,
                config_cleanup,
                metrics_cleanup,
                shutdown_cleanup,
            ).await;
        }));
    }

    /// Simplified rotation monitoring (without holding segment references)
    async fn monitor_rotation_simple(
        config: RotationConfig,
        shutdown: Arc<AtomicBool>,
    ) {
        let mut interval = interval(Duration::from_millis(100));
        
        while !shutdown.load(Ordering::Acquire) {
            interval.tick().await;
            
            // For now, just check shutdown - in practice this would
            // check segment conditions and trigger rotation
        }
    }

    /// Check if current segment needs rotation
    pub async fn should_rotate(&self) -> Result<(bool, String), WalError> {
        let active_guard = self.active_segment.read().await;
        if let Some(ref metadata) = *active_guard {
            // Size check
            if metadata.size >= self.config.max_segment_size {
                return Ok((true, "size".to_string()));
            }
            
            // Age check
            if let Ok(elapsed) = metadata.created_at.elapsed() {
                if elapsed >= self.config.max_segment_age {
                    return Ok((true, "age".to_string()));
                }
            }
            
            // Entry count check
            if metadata.entry_count >= self.config.max_entries_per_segment {
                return Ok((true, "count".to_string()));
            }
        }
        
        Ok((false, "none".to_string()))
    }

    /// Rotate segment atomically
    pub async fn rotate_segment(&self) -> Result<u64, WalError> {
        let start_time = Instant::now();
        
        // Create new segment
        let new_segment_id = self.next_segment_id.fetch_add(1, Ordering::Relaxed);
        let new_segment = self.create_segment(new_segment_id).await?;
        let new_metadata = {
            let metadata = new_segment.metadata();
            let metadata_guard = metadata.read().unwrap();
            metadata_guard.clone()
        };
        
        // Atomic swap of active segment metadata
        let old_metadata = {
            let mut active = self.active_segment.write().await;
            std::mem::replace(&mut *active, Some(new_metadata))
        };
        
        if let Some(old_metadata) = old_metadata {
            // For demonstration, create a dummy sealed segment
            // In practice, this would properly seal the old segment
            let sealed_metadata = SealedSegmentMetadata {
                metadata: old_metadata.clone(),
                sealed_at: SystemTime::now(),
                index: SegmentIndex::new(),
            };
            
            let sealed_segment = SealedSegment {
                metadata: sealed_metadata,
                file_path: old_metadata.path.clone(),
            };
            
            // Add to sealed segments
            self.sealed_segments.write().await.push(sealed_segment);
            
            // Update metrics
            let rotation_latency = start_time.elapsed();
            let mut metrics = self.metrics.write().await;
            metrics.record_rotation(rotation_latency);
            metrics.record_sealed(old_metadata.size);
            
            info!(
                segment_id = new_segment_id,
                rotation_latency_nanos = rotation_latency.as_nanos(),
                "Segment rotation completed"
            );
        }
        
        Ok(new_segment_id)
    }

    /// Create new segment
    async fn create_new_segment(&mut self) -> Result<(), WalError> {
        let segment_id = self.next_segment_id.fetch_add(1, Ordering::Relaxed);
        let segment = self.create_segment(segment_id).await?;
        
        let metadata = {
            let metadata = segment.metadata();
            let metadata_guard = metadata.read().unwrap();
            metadata_guard.clone()
        };
        
        let mut active = self.active_segment.write().await;
        *active = Some(metadata);
        
        debug!(segment_id = segment_id, "Created new active segment");
        Ok(())
    }

    /// Create segment with given ID
    async fn create_segment(&self, segment_id: u64) -> Result<WalSegment, WalError> {
        let segment_path = self.config.wal_dir.join(format!("segment-{:08x}.wal", segment_id));
        
        WalSegment::create(
            segment_id,
            segment_path,
            self.config.max_segment_size,
            self.config.use_memory_mapping,
        )
    }

    /// Background cleanup task
    async fn cleanup_task(
        sealed_segments: Arc<RwLock<Vec<SealedSegment>>>,
        config: RotationConfig,
        metrics: Arc<RwLock<RotationMetrics>>,
        shutdown: Arc<AtomicBool>,
    ) {
        let mut interval = interval(config.cleanup_interval);
        
        while !shutdown.load(Ordering::Acquire) {
            interval.tick().await;
            
            if let Ok(cleanup_result) = Self::cleanup_segments(&sealed_segments, &config).await {
                if cleanup_result.segments_removed > 0 {
                    let mut metrics_guard = metrics.write().await;
                    metrics_guard.record_cleanup(cleanup_result.segments_removed, cleanup_result.bytes_freed);
                    
                    info!(
                        segments_removed = cleanup_result.segments_removed,
                        bytes_freed = cleanup_result.bytes_freed,
                        segments_remaining = cleanup_result.segments_remaining,
                        "Segment cleanup completed"
                    );
                }
            }
        }
    }

    /// Cleanup old segments based on retention policy
    async fn cleanup_segments(
        sealed_segments: &Arc<RwLock<Vec<SealedSegment>>>,
        config: &RotationConfig,
    ) -> Result<CleanupResult, WalError> {
        let mut segments = sealed_segments.write().await;
        let _original_count = segments.len();
        let mut removed_count = 0;
        let mut removed_bytes = 0;

        match &config.retention_policy {
            RetentionPolicy::ByAge(max_age) => {
                let cutoff = SystemTime::now() - *max_age;
                
                let mut i = 0;
                while i < segments.len() {
                    if segments[i].metadata.sealed_at < cutoff {
                        let segment = segments.remove(i);
                        if let Err(e) = std::fs::remove_file(&segment.file_path) {
                            warn!("Failed to delete segment file {:?}: {}", segment.file_path, e);
                        } else {
                            removed_count += 1;
                            removed_bytes += segment.metadata.metadata.size;
                        }
                    } else {
                        i += 1;
                    }
                }
            }

            RetentionPolicy::ByCount(max_count) => {
                while segments.len() > *max_count {
                    if let Some(segment) = segments.remove(0) {  // Remove oldest
                        if let Err(e) = std::fs::remove_file(&segment.file_path) {
                            warn!("Failed to delete segment file {:?}: {}", segment.file_path, e);
                        } else {
                            removed_count += 1;
                            removed_bytes += segment.metadata.metadata.size;
                        }
                    }
                }
            }

            RetentionPolicy::BySize(max_total_size) => {
                let mut total_size: u64 = segments.iter()
                    .map(|s| s.metadata.metadata.size)
                    .sum();

                while total_size > *max_total_size && !segments.is_empty() {
                    if let Some(segment) = segments.remove(0) {  // Remove oldest
                        total_size -= segment.metadata.metadata.size;
                        
                        if let Err(e) = std::fs::remove_file(&segment.file_path) {
                            warn!("Failed to delete segment file {:?}: {}", segment.file_path, e);
                        } else {
                            removed_count += 1;
                            removed_bytes += segment.metadata.metadata.size;
                        }
                    }
                }
            }

            RetentionPolicy::Never => {
                // No cleanup
            }
        }

        Ok(CleanupResult {
            segments_removed: removed_count,
            bytes_freed: removed_bytes,
            segments_remaining: segments.len(),
        })
    }

    /// Get active segment metadata (simplified version)
    pub async fn active_segment(&self) -> Option<SegmentMetadata> {
        let guard = self.active_segment.read().await;
        guard.clone()
    }

    /// Get sealed segment by ID
    pub async fn get_sealed_segment(&self, segment_id: u64) -> Option<SealedSegment> {
        let segments = self.sealed_segments.read().await;
        segments.iter()
            .find(|s| s.metadata.metadata.id == segment_id)
            .cloned()
    }

    /// Get all sealed segments
    pub async fn sealed_segments(&self) -> Vec<SealedSegment> {
        self.sealed_segments.read().await.clone()
    }

    /// Get rotation metrics
    pub async fn metrics(&self) -> RotationMetrics {
        self.metrics.read().await.clone()
    }

    /// Trigger manual rotation
    pub async fn force_rotation(&self) -> Result<u64, WalError> {
        let mut metrics = self.metrics.write().await;
        metrics.rotation_triggers.manual_triggered += 1;
        drop(metrics);
        
        self.rotate_segment().await
    }

    /// Shutdown rotator gracefully
    pub async fn shutdown(&mut self) -> Result<(), WalError> {
        self.shutdown.store(true, Ordering::Release);
        
        // Wait for background tasks to complete
        if let Some(rotation_task) = self.rotation_task.take() {
            let _ = rotation_task.await;
        }
        
        if let Some(cleanup_task) = self.cleanup_task.take() {
            let _ = cleanup_task.await;
        }
        
        info!("Segment rotator shutdown completed");
        Ok(())
    }
}

impl Drop for SegmentRotator {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_rotation_config() {
        let config = RotationConfig::default();
        assert_eq!(config.max_segment_size, 64 * 1024 * 1024);
        assert_eq!(config.max_segment_age, Duration::from_secs(3600));
        assert_eq!(config.max_entries_per_segment, 1_000_000);
    }

    #[tokio::test]
    async fn test_segment_rotator_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = RotationConfig {
            wal_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };

        let rotator = SegmentRotator::new(config).await;
        assert!(rotator.is_ok());
        
        let mut rotator = rotator.unwrap();
        rotator.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_manual_rotation() {
        let temp_dir = TempDir::new().unwrap();
        let config = RotationConfig {
            wal_dir: temp_dir.path().to_path_buf(),
            max_segment_size: 1024, // Small for testing
            ..Default::default()
        };

        let mut rotator = SegmentRotator::new(config).await.unwrap();
        
        // Force rotation
        let new_segment_id = rotator.force_rotation().await.unwrap();
        assert!(new_segment_id > 1);
        
        let metrics = rotator.metrics().await;
        assert_eq!(metrics.rotations_total, 1);
        assert_eq!(metrics.rotation_triggers.manual_triggered, 1);
        
        rotator.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_cleanup_by_count() {
        let temp_dir = TempDir::new().unwrap();
        let config = RotationConfig {
            wal_dir: temp_dir.path().to_path_buf(),
            retention_policy: RetentionPolicy::ByCount(2),
            cleanup_interval: Duration::from_millis(100),
            ..Default::default()
        };

        let mut rotator = SegmentRotator::new(config).await.unwrap();
        
        // Create several segments
        for _ in 0..5 {
            rotator.force_rotation().await.unwrap();
            sleep(Duration::from_millis(10)).await;
        }
        
        // Wait for cleanup to run
        sleep(Duration::from_millis(200)).await;
        
        let sealed_segments = rotator.sealed_segments().await;
        assert!(sealed_segments.len() <= 2, "Should have at most 2 segments, got {}", sealed_segments.len());
        
        rotator.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_segment_index() {
        let mut index = SegmentIndex::new();
        
        index.add_offset_entry(1000, 0);
        index.add_offset_entry(2000, 4096);
        index.add_offset_entry(3000, 8192);
        
        assert_eq!(index.find_offset(1000), Some(0));
        assert_eq!(index.find_offset(1500), Some(0)); // Should find closest
        assert_eq!(index.find_offset(2500), Some(4096));
        assert_eq!(index.find_offset(3500), Some(8192));
    }

    #[tokio::test]
    async fn test_retention_policies() {
        // Test ByAge
        let age_policy = RetentionPolicy::ByAge(Duration::from_secs(3600));
        if let RetentionPolicy::ByAge(duration) = age_policy {
            assert_eq!(duration, Duration::from_secs(3600));
        }
        
        // Test ByCount
        let count_policy = RetentionPolicy::ByCount(10);
        if let RetentionPolicy::ByCount(count) = count_policy {
            assert_eq!(count, 10);
        }
        
        // Test BySize
        let size_policy = RetentionPolicy::BySize(1024 * 1024 * 1024); // 1GB
        if let RetentionPolicy::BySize(size) = size_policy {
            assert_eq!(size, 1024 * 1024 * 1024);
        }
    }
}