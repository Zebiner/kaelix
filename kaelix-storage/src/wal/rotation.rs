use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info};

use super::{segment::WalSegment, WalConfig, WalError};
use crate::segments::SegmentId;

/// Segment rotation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationConfig {
    /// Maximum segment size before rotation (bytes)
    pub max_segment_size: u64,

    /// Maximum time before rotation (seconds)
    pub max_segment_age: u64,

    /// Maximum number of entries per segment
    pub max_entries_per_segment: u64,

    /// Minimum free disk space threshold (bytes)
    pub min_free_space: u64,

    /// Enable time-based rotation
    pub enable_time_rotation: bool,

    /// Enable size-based rotation
    pub enable_size_rotation: bool,

    /// Enable entry-count-based rotation
    pub enable_count_rotation: bool,

    /// Rotation check interval (milliseconds)
    pub check_interval_ms: u64,
}

impl Default for RotationConfig {
    fn default() -> Self {
        Self {
            max_segment_size: 256 * 1024 * 1024, // 256MB
            max_segment_age: 3600,                // 1 hour
            max_entries_per_segment: 1_000_000,   // 1M entries
            min_free_space: 1024 * 1024 * 1024,   // 1GB
            enable_time_rotation: true,
            enable_size_rotation: true,
            enable_count_rotation: false,
            check_interval_ms: 1000, // 1 second
        }
    }
}

impl RotationConfig {
    /// Validate the rotation configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.max_segment_size == 0 {
            return Err("max_segment_size must be greater than 0".to_string());
        }

        if self.max_segment_age == 0 && self.enable_time_rotation {
            return Err("max_segment_age must be greater than 0 when time rotation is enabled".to_string());
        }

        if self.max_entries_per_segment == 0 && self.enable_count_rotation {
            return Err("max_entries_per_segment must be greater than 0 when count rotation is enabled".to_string());
        }

        if self.check_interval_ms == 0 {
            return Err("check_interval_ms must be greater than 0".to_string());
        }

        Ok(())
    }
}

/// Rotation trigger conditions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RotationTrigger {
    /// Segment size exceeded
    Size,
    /// Segment age exceeded
    Time,
    /// Entry count exceeded
    Count,
    /// Manual trigger
    Manual,
    /// Low disk space
    DiskSpace,
    /// System shutdown
    Shutdown,
}

/// Statistics for rotation operations
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RotationStats {
    /// Total rotations performed
    pub total_rotations: u64,

    /// Rotations by trigger type
    pub size_rotations: u64,
    pub time_rotations: u64,
    pub count_rotations: u64,
    pub manual_rotations: u64,
    pub disk_space_rotations: u64,
    pub shutdown_rotations: u64,

    /// Average rotation time (microseconds)
    pub avg_rotation_time_us: u64,

    /// Peak rotation time (microseconds)
    pub peak_rotation_time_us: u64,

    /// Total time spent rotating (microseconds)
    pub total_rotation_time_us: u64,

    /// Last rotation timestamp
    pub last_rotation_time: Option<SystemTime>,

    /// Current active segment
    pub active_segment_id: Option<SegmentId>,

    /// Segments created
    pub segments_created: u64,

    /// Segments sealed
    pub segments_sealed: u64,
}

/// Manages segment rotation for the WAL
#[derive(Debug)]
pub struct RotationManager {
    /// Rotation configuration
    config: RotationConfig,

    /// Current active segment
    active_segment: Option<Arc<WalSegment>>,

    /// Rotation statistics
    stats: Arc<RwLock<RotationStats>>,

    /// Last rotation check time
    last_check_time: Instant,

    /// Flag indicating if rotation is in progress
    rotation_in_progress: Arc<Mutex<bool>>,

    /// Shutdown flag
    shutdown_flag: bool,
}

impl RotationManager {
    /// Create a new rotation manager
    pub fn new(config: RotationConfig) -> Self {
        Self {
            config,
            active_segment: None,
            stats: Arc::new(RwLock::new(RotationStats::default())),
            last_check_time: Instant::now(),
            rotation_in_progress: Arc::new(Mutex::new(false)),
            shutdown_flag: false,
        }
    }

    /// Set the active segment
    pub fn set_active_segment(&mut self, segment: Arc<WalSegment>) {
        let segment_id = segment.id();
        self.active_segment = Some(segment);

        // Update stats
        tokio::spawn({
            let stats = Arc::clone(&self.stats);
            async move {
                let mut stats = stats.write().await;
                stats.active_segment_id = Some(segment_id);
            }
        });
    }

    /// Check if rotation is needed
    pub async fn check_rotation_needed(&mut self) -> Result<Option<RotationTrigger>, WalError> {
        // Don't check too frequently
        let now = Instant::now();
        if now.duration_since(self.last_check_time) < Duration::from_millis(self.config.check_interval_ms) {
            return Ok(None);
        }
        self.last_check_time = now;

        if self.shutdown_flag {
            return Ok(Some(RotationTrigger::Shutdown));
        }

        let Some(segment) = &self.active_segment else {
            return Ok(None);
        };

        // Check size-based rotation
        if self.config.enable_size_rotation {
            let segment_size = segment.size();
            if segment_size >= self.config.max_segment_size {
                debug!("Size-based rotation triggered: {} >= {}", segment_size, self.config.max_segment_size);
                return Ok(Some(RotationTrigger::Size));
            }
        }

        // Check time-based rotation
        if self.config.enable_time_rotation {
            let created_at = segment.metadata().read().unwrap().created_at;
            let age = SystemTime::now()
                .duration_since(created_at)
                .map_err(|e| WalError::InvalidOperation(format!("Clock error: {}", e)))?;

            if age.as_secs() >= self.config.max_segment_age {
                debug!("Time-based rotation triggered: {}s >= {}s", age.as_secs(), self.config.max_segment_age);
                return Ok(Some(RotationTrigger::Time));
            }
        }

        // Check count-based rotation
        if self.config.enable_count_rotation {
            let entry_count = segment.entry_count();
            if entry_count >= self.config.max_entries_per_segment {
                debug!("Count-based rotation triggered: {} >= {}", entry_count, self.config.max_entries_per_segment);
                return Ok(Some(RotationTrigger::Count));
            }
        }

        Ok(None)
    }

    /// Perform segment rotation
    pub async fn rotate_segment(&mut self, trigger: RotationTrigger, wal_config: &WalConfig) -> Result<Arc<WalSegment>, WalError> {
        let rotation_start = Instant::now();

        // Check if rotation is already in progress
        {
            let mut in_progress = self.rotation_in_progress.lock().await;
            if *in_progress {
                return Err(WalError::InvalidOperation("Rotation already in progress".to_string()));
            }
            *in_progress = true;
        }

        let result = self._perform_rotation(trigger, wal_config).await;

        // Clear rotation in progress flag
        {
            let mut in_progress = self.rotation_in_progress.lock().await;
            *in_progress = false;
        }

        // Update rotation stats
        let rotation_time = rotation_start.elapsed();
        self._update_rotation_stats(trigger, rotation_time).await;

        result
    }

    /// Internal rotation implementation
    async fn _perform_rotation(&mut self, trigger: RotationTrigger, wal_config: &WalConfig) -> Result<Arc<WalSegment>, WalError> {
        info!("Starting segment rotation (trigger: {:?})", trigger);

        // Seal the current segment
        if let Some(current_segment) = &self.active_segment {
            debug!("Sealing current segment: {}", current_segment.id());
            current_segment.seal().await?;
            
            // Update stats
            {
                let mut stats = self.stats.write().await;
                stats.segments_sealed += 1;
            }
        }

        // Create new segment
        let new_segment_id = self._get_next_segment_id().await;
        debug!("Creating new segment: {}", new_segment_id);
        
        let segment_path = wal_config.wal_dir.join(format!("{}.wal", new_segment_id));
        let new_segment = Arc::new(WalSegment::create(
            new_segment_id,
            segment_path,
            wal_config.segment_size,
            wal_config.use_mmap,
        )?);
        self.set_active_segment(Arc::clone(&new_segment));

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.segments_created += 1;
        }

        info!("Segment rotation completed (old -> new: {:?} -> {})", 
              self.active_segment.as_ref().map(|s| s.id()), new_segment_id);

        Ok(new_segment)
    }

    /// Get the next segment ID
    async fn _get_next_segment_id(&self) -> SegmentId {
        if let Some(current) = &self.active_segment {
            current.id() + 1
        } else {
            1
        }
    }

    /// Update rotation statistics
    async fn _update_rotation_stats(&self, trigger: RotationTrigger, duration: Duration) {
        let mut stats = self.stats.write().await;
        
        stats.total_rotations += 1;
        
        match trigger {
            RotationTrigger::Size => stats.size_rotations += 1,
            RotationTrigger::Time => stats.time_rotations += 1,
            RotationTrigger::Count => stats.count_rotations += 1,
            RotationTrigger::Manual => stats.manual_rotations += 1,
            RotationTrigger::DiskSpace => stats.disk_space_rotations += 1,
            RotationTrigger::Shutdown => stats.shutdown_rotations += 1,
        }

        let duration_us = duration.as_micros() as u64;
        stats.total_rotation_time_us += duration_us;
        
        if duration_us > stats.peak_rotation_time_us {
            stats.peak_rotation_time_us = duration_us;
        }

        // Update average (simple moving average)
        if stats.total_rotations > 0 {
            stats.avg_rotation_time_us = stats.total_rotation_time_us / stats.total_rotations;
        }

        stats.last_rotation_time = Some(SystemTime::now());
    }

    /// Get rotation statistics
    pub async fn stats(&self) -> RotationStats {
        self.stats.read().await.clone()
    }

    /// Manually trigger rotation
    pub async fn force_rotation(&mut self, wal_config: &WalConfig) -> Result<Arc<WalSegment>, WalError> {
        info!("Manual rotation triggered");
        self.rotate_segment(RotationTrigger::Manual, wal_config).await
    }

    /// Stop the rotation manager
    pub async fn stop(&mut self) -> Result<(), WalError> {
        info!("Stopping rotation manager");
        self.shutdown_flag = true;

        // If we have an active segment, seal it
        if let Some(segment) = &self.active_segment {
            info!("Sealing final segment during shutdown: {}", segment.id());
            segment.seal().await?;
            
            // Update stats
            {
                let mut stats = self.stats.write().await;
                stats.segments_sealed += 1;
                stats.shutdown_rotations += 1;
            }
        }

        Ok(())
    }

    /// Get the current active segment
    pub fn active_segment(&self) -> Option<&Arc<WalSegment>> {
        self.active_segment.as_ref()
    }

    /// Check if rotation is in progress
    pub async fn is_rotation_in_progress(&self) -> bool {
        *self.rotation_in_progress.lock().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> RotationConfig {
        RotationConfig {
            max_segment_size: 1024, // 1KB for quick testing
            max_segment_age: 1,     // 1 second
            max_entries_per_segment: 10,
            min_free_space: 100,
            enable_time_rotation: true,
            enable_size_rotation: true,
            enable_count_rotation: true,
            check_interval_ms: 100, // 100ms for quick testing
        }
    }

    #[tokio::test]
    async fn test_rotation_config_validation() {
        let config = create_test_config();
        assert!(config.validate().is_ok());

        let mut invalid_config = config.clone();
        invalid_config.max_segment_size = 0;
        assert!(invalid_config.validate().is_err());
    }

    #[tokio::test]
    async fn test_rotation_manager_creation() {
        let config = create_test_config();
        let manager = RotationManager::new(config);
        
        let stats = manager.stats().await;
        assert_eq!(stats.total_rotations, 0);
        assert_eq!(stats.active_segment_id, None);
    }

    #[tokio::test]
    async fn test_rotation_trigger_detection() {
        let config = create_test_config();
        let mut manager = RotationManager::new(config);
        
        // No segment initially
        let trigger = manager.check_rotation_needed().await.unwrap();
        assert_eq!(trigger, None);
    }

    #[tokio::test]
    async fn test_rotation_stats() {
        let config = create_test_config();
        let manager = RotationManager::new(config);
        
        let stats = manager.stats().await;
        assert_eq!(stats.total_rotations, 0);
        assert_eq!(stats.segments_created, 0);
        assert_eq!(stats.segments_sealed, 0);
    }
}

/// Duration conversion helper for testing
#[cfg(test)]
mod duration_seconds {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_secs().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let seconds = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(seconds))
    }
}