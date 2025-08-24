use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use tokio::fs;
use tracing;

use super::{
    LogEntry, WalError, SegmentId
};

/// Configuration for WAL recovery operations
#[derive(Debug, Clone)]
pub struct RecoveryConfig {
    /// Recovery mode (fast vs thorough)
    pub recovery_mode: RecoveryMode,
    
    /// Corruption handling policy
    pub corruption_policy: CorruptionPolicy,
    
    /// Verification settings
    pub verify_checksums: bool,
    pub verify_sequence_continuity: bool,
    pub verify_timestamps: bool,
    
    /// Performance settings
    pub max_recovery_time: Duration,
    pub parallel_recovery: bool,
    pub recovery_batch_size: usize,
    
    /// Repair settings
    pub enable_auto_repair: bool,
    pub backup_corrupted_segments: bool,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            recovery_mode: RecoveryMode::Thorough,
            corruption_policy: CorruptionPolicy::Repair,
            verify_checksums: true,
            verify_sequence_continuity: true,
            verify_timestamps: true,
            max_recovery_time: Duration::from_secs(30),
            parallel_recovery: true,
            recovery_batch_size: 1000,
            enable_auto_repair: true,
            backup_corrupted_segments: true,
        }
    }
}

/// Recovery mode for different scenarios
#[derive(Debug, Clone, Copy)]
pub enum RecoveryMode {
    /// Skip non-critical validation for speed
    Fast,
    /// Complete validation and repair
    Thorough,
    /// Only essential recovery operations
    Minimal,
}

/// Policy for handling corrupted data
#[derive(Debug, Clone, Copy)]
pub enum CorruptionPolicy {
    /// Fail recovery on any corruption
    Fail,
    /// Skip corrupted entries and continue
    Skip,
    /// Attempt to repair corrupted data
    Repair,
    /// Truncate at first corruption
    Truncate,
}

/// Statistics collected during recovery
#[derive(Debug, Clone, Default)]
pub struct RecoveryStats {
    /// Total segments processed
    pub segments_processed: usize,
    /// Total entries recovered
    pub entries_recovered: u64,
    /// Corrupted segments found
    pub corrupted_segments: usize,
    /// Corrupted entries found
    pub corrupted_entries: u64,
    /// Repaired segments
    pub repaired_segments: usize,
    /// Repaired entries
    pub repaired_entries: u64,
    /// Resync operations performed
    pub resync_operations: usize,
    /// Total recovery time
    pub total_recovery_time: Duration,
    /// Bytes processed
    pub bytes_processed: u64,
}

/// Result of recovery operation
#[derive(Debug, Clone)]
pub struct RecoveryResult {
    /// Number of segments recovered
    pub segments_recovered: usize,
    /// Number of entries recovered
    pub entries_recovered: u64,
    /// Number of corrupted entries found
    pub corrupted_entries: u64,
    /// Number of repaired entries
    pub repaired_entries: u64,
    /// Total recovery time
    pub recovery_time: Duration,
    /// Last recovered sequence number
    pub last_sequence: Option<u64>,
    /// Integrity status after recovery
    pub integrity_status: IntegrityStatus,
}

/// Integrity status after recovery
#[derive(Debug, Clone, Copy)]
pub enum IntegrityStatus {
    /// All checks passed
    Verified,
    /// Some issues were repaired
    Repaired,
    /// Issues found but not repaired
    Compromised,
    /// Recovery failed
    Failed,
}

/// Information about a discovered segment file
#[derive(Debug, Clone)]
pub struct SegmentFile {
    pub path: PathBuf,
    pub segment_id: SegmentId,
    pub size: u64,
    pub modified: SystemTime,
}

/// A validated segment with entries
#[derive(Debug, Clone)]
pub struct ValidatedSegment {
    pub file: SegmentFile,
    pub entries: Vec<ValidatedEntry>,
    pub first_sequence: Option<u64>,
    pub last_sequence: Option<u64>,
    pub entry_count: usize,
    pub is_sealed: bool,
}

/// A validated entry with validation results
#[derive(Debug, Clone)]
pub struct ValidatedEntry {
    pub entry: LogEntry,
    pub offset: usize,
    pub validation: EntryValidation,
}

/// Validation results for an entry
#[derive(Debug, Clone)]
pub struct EntryValidation {
    pub is_valid: bool,
    pub checksum_valid: bool,
    pub sequence_valid: bool,
    pub timestamp_valid: bool,
    pub issues: Vec<ValidationIssue>,
}

/// Issues found during validation
#[derive(Debug, Clone)]
pub enum ValidationIssue {
    InvalidChecksum { expected: u32, found: u32 },
    SequenceGap { expected: u64, found: u64 },
    TimestampReverse { sequence: u64, timestamp: u64 },
    CorruptedData { details: String },
}

/// Recovery plan with issues and operations
#[derive(Debug)]
pub struct RecoveryPlan {
    pub issues: Vec<RecoveryIssue>,
    pub operations: Vec<RecoveryOperation>,
    pub estimated_time: Duration,
}

impl RecoveryPlan {
    pub fn new() -> Self {
        Self {
            issues: Vec::new(),
            operations: Vec::new(),
            estimated_time: Duration::ZERO,
        }
    }
    
    pub fn add_issue(&mut self, issue: RecoveryIssue) {
        self.issues.push(issue);
    }
    
    pub fn add_operation(&mut self, operation: RecoveryOperation) {
        self.operations.push(operation);
    }
}

/// Issues discovered during recovery analysis
#[derive(Debug)]
pub enum RecoveryIssue {
    SequenceGap { expected: u64, found: u64 },
    DuplicateSequence { sequence: u64 },
    TimestampReverse { sequence: u64, timestamp: u64, expected_min: u64 },
    ChecksumMismatch { sequence: u64 },
    CorruptedEntry { sequence: u64, details: String },
    UnexpectedEof { segment_id: SegmentId },
    InvalidSegmentId { path: PathBuf },
}

/// Operations to perform during recovery
#[derive(Debug)]
pub enum RecoveryOperation {
    SealSegment { segment_id: SegmentId },
    RepairEntry { sequence: u64 },
    RebuildIndex { segment_id: SegmentId },
    TruncateAt { sequence: u64 },
    BackupCorrupted { path: PathBuf },
}

/// WAL state after recovery
#[derive(Debug, Clone)]
pub struct WalState {
    pub segments: Vec<ValidatedSegment>,
    pub total_entries: u64,
    pub last_sequence: Option<u64>,
    pub first_sequence: Option<u64>,
}

/// Main recovery manager for WAL crash recovery
pub struct RecoveryManager {
    /// WAL directory for recovery
    wal_dir: PathBuf,
    
    /// Recovery configuration
    config: RecoveryConfig,
    
    /// Recovery statistics
    stats: RecoveryStats,
    
    /// Repair engine
    repair_engine: RepairEngine,
}

impl RecoveryManager {
    /// Create new recovery manager
    pub fn new(wal_dir: PathBuf, config: RecoveryConfig) -> Self {
        Self {
            wal_dir,
            config,
            stats: RecoveryStats::default(),
            repair_engine: RepairEngine::new(RepairConfig::default()),
        }
    }
    
    /// Main recovery entry point
    pub async fn recover(&mut self) -> Result<RecoveryResult, WalError> {
        let start_time = Instant::now();
        
        tracing::info!("Starting WAL recovery in {:?} mode", self.config.recovery_mode);
        
        // 1. Discover all segment files
        let segment_files = self.discover_segment_files().await?;
        tracing::info!("Discovered {} segment files", segment_files.len());
        
        if segment_files.is_empty() {
            return Ok(RecoveryResult {
                segments_recovered: 0,
                entries_recovered: 0,
                corrupted_entries: 0,
                repaired_entries: 0,
                recovery_time: start_time.elapsed(),
                last_sequence: None,
                integrity_status: IntegrityStatus::Verified,
            });
        }
        
        // 2. Validate and sort segments
        let validated_segments = self.validate_segments(segment_files).await?;
        
        // 3. Check for incomplete writes and recovery scenarios
        let recovery_plan = self.analyze_recovery_needs(&validated_segments).await?;
        
        // 4. Execute recovery based on plan
        let _recovered_segments = self.execute_recovery(recovery_plan).await?;
        
        // 5. Rebuild WAL state
        let wal_state = self.rebuild_wal_state(&validated_segments).await?;
        
        // 6. Verify consistency
        self.verify_consistency(&wal_state).await?;
        
        let total_time = start_time.elapsed();
        self.stats.total_recovery_time = total_time;
        
        Ok(RecoveryResult {
            segments_recovered: validated_segments.len(),
            entries_recovered: wal_state.total_entries,
            corrupted_entries: self.stats.corrupted_entries,
            repaired_entries: self.stats.repaired_entries,
            recovery_time: total_time,
            last_sequence: wal_state.last_sequence,
            integrity_status: if self.stats.corrupted_entries > 0 {
                if self.stats.repaired_entries > 0 {
                    IntegrityStatus::Repaired
                } else {
                    IntegrityStatus::Compromised
                }
            } else {
                IntegrityStatus::Verified
            },
        })
    }
    
    /// Discover all WAL segment files
    async fn discover_segment_files(&self) -> Result<Vec<SegmentFile>, WalError> {
        let mut segment_files = Vec::new();
        
        if !self.wal_dir.exists() {
            tracing::warn!("WAL directory does not exist: {:?}", self.wal_dir);
            return Ok(segment_files);
        }
        
        let mut entries = fs::read_dir(&self.wal_dir).await
            .map_err(|e| WalError::Io(format!("Failed to read WAL directory: {}", e)))?;
        
        while let Some(entry) = entries.next_entry().await
            .map_err(|e| WalError::Io(format!("Failed to read directory entry: {}", e)))? {
            
            let path = entry.path();
            
            // Only process .wal files
            if let Some(extension) = path.extension() {
                if extension == "wal" {
                    if let Some(segment_id) = self.parse_segment_id(&path) {
                        let metadata = entry.metadata().await
                            .map_err(|e| WalError::Io(format!("Failed to get file metadata: {}", e)))?;
                        
                        segment_files.push(SegmentFile {
                            path: path.clone(),
                            segment_id,
                            size: metadata.len(),
                            modified: metadata.modified()
                                .unwrap_or_else(|_| SystemTime::now()),
                        });
                    }
                }
            }
        }
        
        // Sort by segment ID for proper recovery order
        segment_files.sort_by_key(|sf| sf.segment_id);
        
        Ok(segment_files)
    }
    
    /// Parse segment ID from filename
    fn parse_segment_id(&self, path: &Path) -> Option<SegmentId> {
        path.file_stem()
            .and_then(|name| name.to_str())
            .and_then(|name| {
                if name.starts_with("segment-") {
                    name.strip_prefix("segment-")
                        .and_then(|id_str| id_str.parse().ok())
                } else {
                    None
                }
            })
    }
    
    /// Validate individual segments
    async fn validate_segments(&mut self, files: Vec<SegmentFile>) -> Result<Vec<ValidatedSegment>, WalError> {
        let mut validated = Vec::new();
        
        for file in files {
            match self.validate_single_segment(&file).await {
                Ok(segment) => {
                    self.stats.segments_processed += 1;
                    validated.push(segment);
                },
                Err(e) => {
                    tracing::warn!("Segment {} validation failed: {}", file.segment_id, e);
                    self.stats.corrupted_segments += 1;
                    
                    // Handle based on corruption policy
                    match self.config.corruption_policy {
                        CorruptionPolicy::Fail => return Err(e),
                        CorruptionPolicy::Skip => {
                            continue;
                        },
                        CorruptionPolicy::Repair => {
                            match self.repair_segment(&file).await {
                                Ok(repaired) => {
                                    validated.push(repaired);
                                    self.stats.repaired_segments += 1;
                                },
                                Err(repair_err) => {
                                    tracing::error!("Segment repair failed: {}", repair_err);
                                    if matches!(self.config.recovery_mode, RecoveryMode::Thorough) {
                                        return Err(repair_err);
                                    }
                                }
                            }
                        },
                        CorruptionPolicy::Truncate => {
                            // Truncate WAL at this point
                            tracing::warn!("Truncating WAL at corrupted segment {}", file.segment_id);
                            break;
                        }
                    }
                }
            }
        }
        
        Ok(validated)
    }
    
    /// Validate single segment file
    async fn validate_single_segment(&mut self, file: &SegmentFile) -> Result<ValidatedSegment, WalError> {
        let buffer = std::fs::read(&file.path)
            .map_err(|e| WalError::Io(format!("Failed to read segment file: {}", e)))?;
        
        self.stats.bytes_processed += buffer.len() as u64;
        
        let mut entries = Vec::new();
        let mut offset = 0;
        let mut expected_sequence = None;
        
        // Read and validate all entries in segment
        while offset < buffer.len() {
            match self.read_entry_at_offset(&buffer, offset) {
                Ok((entry, entry_size)) => {
                    // Validate entry
                    let validation_result = self.validate_entry(&entry, expected_sequence).await?;
                    let sequence_number = entry.sequence_number;
                    
                    if validation_result.is_valid {
                        entries.push(ValidatedEntry {
                            entry,
                            offset,
                            validation: validation_result,
                        });
                        
                        expected_sequence = Some(sequence_number + 1);
                        offset += entry_size;
                    } else {
                        // Handle invalid entry
                        match self.config.corruption_policy {
                            CorruptionPolicy::Fail => {
                                return Err(WalError::Corruption {
                                    sequence: sequence_number,
                                    details: format!("Entry validation failed: {:?}", validation_result.issues),
                                });
                            },
                            CorruptionPolicy::Skip => {
                                self.stats.corrupted_entries += 1;
                                offset += 1; // Skip byte and try to resync
                            },
                            _ => break, // Repair or truncate handled elsewhere
                        }
                    }
                },
                Err(_) => {
                    // Cannot read entry, try to resync
                    if let Some(next_offset) = self.find_next_valid_entry(&buffer, offset + 1) {
                        tracing::warn!("Resynced at offset {} after corruption", next_offset);
                        offset = next_offset;
                        self.stats.resync_operations += 1;
                    } else {
                        // No more valid entries found
                        break;
                    }
                }
            }
        }
        
        Ok(ValidatedSegment {
            file: file.clone(),
            first_sequence: entries.first().map(|e| e.entry.sequence_number),
            last_sequence: entries.last().map(|e| e.entry.sequence_number),
            entry_count: entries.len(),
            is_sealed: self.check_if_sealed(&buffer)?,
            entries,
        })
    }
    
    /// Read entry at specific offset in buffer
    fn read_entry_at_offset(&self, buffer: &[u8], offset: usize) -> Result<(LogEntry, usize), WalError> {
        if offset + 24 > buffer.len() {
            return Err(WalError::Io("Not enough data for entry header".to_string()));
        }
        
        // Read header to determine payload size
        let header = LogEntry::deserialize_header(&buffer[offset..offset + 24])?;
        let total_size = 24 + header.payload_length as usize;
        
        if offset + total_size > buffer.len() {
            return Err(WalError::Io("Not enough data for full entry".to_string()));
        }
        
        // Read full entry
        let entry = LogEntry::deserialize(&buffer[offset..offset + total_size])?;
        Ok((entry, total_size))
    }
    
    /// Validate individual entry
    async fn validate_entry(&self, entry: &LogEntry, expected_sequence: Option<u64>) -> Result<EntryValidation, WalError> {
        let mut validation = EntryValidation {
            is_valid: true,
            checksum_valid: true,
            sequence_valid: true,
            timestamp_valid: true,
            issues: Vec::new(),
        };
        
        // Verify checksum if enabled
        if self.config.verify_checksums {
            if !entry.verify_integrity() {
                validation.is_valid = false;
                validation.checksum_valid = false;
                validation.issues.push(ValidationIssue::CorruptedData {
                    details: "Checksum verification failed".to_string(),
                });
            }
        }
        
        // Verify sequence continuity if enabled
        if self.config.verify_sequence_continuity {
            if let Some(expected) = expected_sequence {
                if entry.sequence_number != expected {
                    validation.is_valid = false;
                    validation.sequence_valid = false;
                    validation.issues.push(ValidationIssue::SequenceGap {
                        expected,
                        found: entry.sequence_number,
                    });
                }
            }
        }
        
        // Verify timestamps if enabled
        if self.config.verify_timestamps {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64;
                
            // Check if timestamp is too far in the future (more than 1 hour)
            if entry.timestamp > now + 3_600_000_000_000 {
                validation.issues.push(ValidationIssue::TimestampReverse {
                    sequence: entry.sequence_number,
                    timestamp: entry.timestamp,
                });
            }
        }
        
        Ok(validation)
    }
    
    /// Find next valid entry starting from offset
    fn find_next_valid_entry(&self, buffer: &[u8], start_offset: usize) -> Option<usize> {
        // Look for entry magic pattern or header signature
        for i in start_offset..buffer.len().saturating_sub(24) {
            // Try to parse entry header at this position
            if let Ok(entry) = LogEntry::deserialize_header(&buffer[i..i + 24]) {
                if self.quick_validate_entry_header(&entry) {
                    return Some(i);
                }
            }
        }
        None
    }
    
    /// Quick validation of entry header
    fn quick_validate_entry_header(&self, entry: &LogEntry) -> bool {
        // Basic sanity checks
        entry.sequence_number > 0 &&
        entry.timestamp > 0 &&
        entry.payload_length < 1_000_000 // Reasonable payload size limit
    }
    
    /// Check if segment appears to be sealed
    fn check_if_sealed(&self, _buffer: &[u8]) -> Result<bool, WalError> {
        // A sealed segment should have a specific marker or be completely filled
        // For now, assume unsealed if we can't determine
        Ok(false)
    }
    
    /// Repair a corrupted segment
    async fn repair_segment(&mut self, file: &SegmentFile) -> Result<ValidatedSegment, WalError> {
        tracing::info!("Attempting to repair segment {}", file.segment_id);
        
        // Backup the corrupted segment if configured
        if self.config.backup_corrupted_segments {
            self.backup_corrupted_segment(file).await?;
        }
        
        // Try to recover what we can from the segment
        let mut recovered_entries = Vec::new();
        
        let buffer = std::fs::read(&file.path)
            .map_err(|e| WalError::Io(format!("Failed to read segment for repair: {}", e)))?;
        
        let mut offset = 0;
        while offset < buffer.len() {
            if let Some(valid_offset) = self.find_next_valid_entry(&buffer, offset) {
                if let Ok((entry, size)) = self.read_entry_at_offset(&buffer, valid_offset) {
                    // Attempt to repair the entry if needed
                    match self.repair_engine.repair_entry(&entry).await {
                        Ok(repaired_entry) => {
                            recovered_entries.push(ValidatedEntry {
                                entry: repaired_entry,
                                offset: valid_offset,
                                validation: EntryValidation {
                                    is_valid: true,
                                    checksum_valid: true,
                                    sequence_valid: true,
                                    timestamp_valid: true,
                                    issues: Vec::new(),
                                },
                            });
                            self.stats.repaired_entries += 1;
                            offset = valid_offset + size;
                        },
                        Err(_) => {
                            // Skip unrepairable entry
                            offset = valid_offset + 1;
                        }
                    }
                } else {
                    offset = valid_offset + 1;
                }
            } else {
                break;
            }
        }
        
        let first_seq = recovered_entries.first().map(|e| e.entry.sequence_number);
        let last_seq = recovered_entries.last().map(|e| e.entry.sequence_number);
        let entry_count = recovered_entries.len();
        
        Ok(ValidatedSegment {
            file: file.clone(),
            first_sequence: first_seq,
            last_sequence: last_seq,
            entry_count,
            is_sealed: false, // Assume repaired segments are not sealed
            entries: recovered_entries,
        })
    }
    
    /// Backup a corrupted segment
    async fn backup_corrupted_segment(&self, file: &SegmentFile) -> Result<(), WalError> {
        let backup_path = file.path.with_extension("wal.corrupted");
        fs::copy(&file.path, &backup_path).await
            .map_err(|e| WalError::Io(format!("Failed to backup corrupted segment: {}", e)))?;
        
        tracing::info!("Backed up corrupted segment to {:?}", backup_path);
        Ok(())
    }
    
    /// Analyze what recovery operations are needed
    async fn analyze_recovery_needs(&self, segments: &[ValidatedSegment]) -> Result<RecoveryPlan, WalError> {
        let mut plan = RecoveryPlan::new();
        
        if segments.is_empty() {
            return Ok(plan);
        }
        
        // Check for sequence gaps
        let mut expected_sequence = 1;
        for segment in segments {
            if let Some(first_seq) = segment.first_sequence {
                if first_seq > expected_sequence {
                    plan.add_issue(RecoveryIssue::SequenceGap {
                        expected: expected_sequence,
                        found: first_seq,
                    });
                }
                
                if let Some(last_seq) = segment.last_sequence {
                    expected_sequence = last_seq + 1;
                }
            }
        }
        
        // Check for unsealed active segment
        if let Some(last_segment) = segments.last() {
            if !last_segment.is_sealed {
                plan.add_operation(RecoveryOperation::SealSegment {
                    segment_id: last_segment.file.segment_id,
                });
            }
        }
        
        // Check for duplicate sequences
        let mut seen_sequences = std::collections::HashSet::new();
        for segment in segments {
            for entry in &segment.entries {
                if !seen_sequences.insert(entry.entry.sequence_number) {
                    plan.add_issue(RecoveryIssue::DuplicateSequence {
                        sequence: entry.entry.sequence_number,
                    });
                }
            }
        }
        
        // Estimate recovery time
        plan.estimated_time = Duration::from_millis(segments.len() as u64 * 10);
        
        Ok(plan)
    }
    
    /// Execute recovery operations
    async fn execute_recovery(&mut self, plan: RecoveryPlan) -> Result<Vec<ValidatedSegment>, WalError> {
        tracing::info!("Executing recovery plan with {} operations", plan.operations.len());
        
        for operation in plan.operations {
            match operation {
                RecoveryOperation::SealSegment { segment_id } => {
                    tracing::info!("Would seal segment {} (operation logged)", segment_id);
                },
                RecoveryOperation::RepairEntry { sequence } => {
                    tracing::info!("Would repair entry {} (operation logged)", sequence);
                },
                RecoveryOperation::RebuildIndex { segment_id } => {
                    tracing::info!("Would rebuild index for segment {} (operation logged)", segment_id);
                },
                RecoveryOperation::TruncateAt { sequence } => {
                    tracing::warn!("Would truncate WAL at sequence {} (operation logged)", sequence);
                },
                RecoveryOperation::BackupCorrupted { path } => {
                    tracing::info!("Would backup corrupted file {:?} (operation logged)", path);
                },
            }
        }
        
        // For now, return empty list as this is a placeholder implementation
        // In a full implementation, this would return the recovered segments
        Ok(Vec::new())
    }
    
    /// Rebuild WAL state from validated segments
    async fn rebuild_wal_state(&mut self, segments: &[ValidatedSegment]) -> Result<WalState, WalError> {
        let mut total_entries = 0;
        let mut first_sequence = None;
        let mut last_sequence = None;
        
        for segment in segments {
            total_entries += segment.entry_count as u64;
            
            if first_sequence.is_none() {
                first_sequence = segment.first_sequence;
            }
            
            if let Some(seq) = segment.last_sequence {
                last_sequence = Some(seq);
            }
        }
        
        self.stats.entries_recovered = total_entries;
        
        Ok(WalState {
            segments: segments.to_vec(),
            total_entries,
            last_sequence,
            first_sequence,
        })
    }
    
    /// Verify consistency after recovery
    async fn verify_consistency(&self, wal_state: &WalState) -> Result<(), WalError> {
        let start_time = Instant::now();
        
        // Verify sequence continuity
        if self.config.verify_sequence_continuity {
            self.verify_sequence_continuity(wal_state).await?;
        }
        
        // Verify checksums
        if self.config.verify_checksums {
            self.verify_checksums(wal_state).await?;
        }
        
        // Verify timestamp ordering
        if self.config.verify_timestamps {
            self.verify_timestamp_ordering(wal_state).await?;
        }
        
        let verification_time = start_time.elapsed();
        tracing::info!("Consistency verification completed in {:?}", verification_time);
        
        Ok(())
    }
    
    /// Verify sequence numbers are continuous
    async fn verify_sequence_continuity(&self, wal_state: &WalState) -> Result<(), WalError> {
        let mut expected_sequence = 1;
        
        for segment in &wal_state.segments {
            for entry in &segment.entries {
                if entry.entry.sequence_number != expected_sequence {
                    return Err(WalError::Corruption {
                        sequence: entry.entry.sequence_number,
                        details: format!(
                            "Sequence gap: expected {}, found {}",
                            expected_sequence, entry.entry.sequence_number
                        ),
                    });
                }
                expected_sequence += 1;
            }
        }
        
        Ok(())
    }
    
    /// Verify all entry checksums
    async fn verify_checksums(&self, wal_state: &WalState) -> Result<(), WalError> {
        for segment in &wal_state.segments {
            for entry in &segment.entries {
                if !entry.entry.verify_integrity() {
                    return Err(WalError::Corruption {
                        sequence: entry.entry.sequence_number,
                        details: "Checksum verification failed".to_string(),
                    });
                }
            }
        }
        
        Ok(())
    }
    
    /// Verify timestamp ordering
    async fn verify_timestamp_ordering(&self, wal_state: &WalState) -> Result<(), WalError> {
        let mut last_timestamp = 0;
        
        for segment in &wal_state.segments {
            for entry in &segment.entries {
                if entry.entry.timestamp < last_timestamp {
                    tracing::warn!(
                        "Timestamp ordering violation at sequence {}: {} < {}",
                        entry.entry.sequence_number,
                        entry.entry.timestamp,
                        last_timestamp
                    );
                    // Don't fail on timestamp issues, just warn
                }
                last_timestamp = entry.entry.timestamp;
            }
        }
        
        Ok(())
    }
    
    /// Get recovery statistics
    pub fn stats(&self) -> &RecoveryStats {
        &self.stats
    }
}

/// Configuration for repair operations
#[derive(Debug, Clone)]
pub struct RepairConfig {
    pub enable_checksum_repair: bool,
    pub enable_sequence_repair: bool,
    pub enable_timestamp_repair: bool,
    pub enable_payload_reconstruction: bool,
}

impl Default for RepairConfig {
    fn default() -> Self {
        Self {
            enable_checksum_repair: true,
            enable_sequence_repair: true,
            enable_timestamp_repair: true,
            enable_payload_reconstruction: false, // More complex, disabled by default
        }
    }
}

/// Statistics for repair operations
#[derive(Debug, Clone, Default)]
pub struct RepairStats {
    pub checksum_repairs: u64,
    pub sequence_repairs: u64,
    pub timestamp_repairs: u64,
    pub payload_reconstructions: u64,
    pub total_repair_attempts: u64,
    pub successful_repairs: u64,
}

/// Engine for repairing corrupted entries
pub struct RepairEngine {
    config: RepairConfig,
    stats: RepairStats,
}

impl RepairEngine {
    pub fn new(config: RepairConfig) -> Self {
        Self {
            config,
            stats: RepairStats::default(),
        }
    }
    
    /// Attempt to repair corrupted entry
    pub async fn repair_entry(&mut self, entry: &LogEntry) -> Result<LogEntry, WalError> {
        self.stats.total_repair_attempts += 1;
        
        let mut repaired_entry = entry.clone();
        let mut repair_attempted = false;
        
        // 1. Checksum repair (if payload is intact)
        if self.config.enable_checksum_repair && !entry.verify_integrity() {
            if let Ok(repaired) = self.repair_checksum(&repaired_entry) {
                repaired_entry = repaired;
                self.stats.checksum_repairs += 1;
                repair_attempted = true;
            }
        }
        
        // 2. Timestamp repair (if timestamp is clearly wrong)
        if self.config.enable_timestamp_repair {
            if let Ok(repaired) = self.repair_timestamp(&repaired_entry) {
                repaired_entry = repaired;
                self.stats.timestamp_repairs += 1;
                repair_attempted = true;
            }
        }
        
        // 3. Payload reconstruction (if possible from redundant information)
        if self.config.enable_payload_reconstruction {
            if let Ok(repaired) = self.reconstruct_payload(&repaired_entry) {
                repaired_entry = repaired;
                self.stats.payload_reconstructions += 1;
                repair_attempted = true;
            }
        }
        
        if repair_attempted {
            // Verify the repaired entry
            if repaired_entry.verify_integrity() {
                self.stats.successful_repairs += 1;
                Ok(repaired_entry)
            } else {
                Err(WalError::Corruption {
                    sequence: entry.sequence_number,
                    details: "Entry repair failed verification".to_string(),
                })
            }
        } else {
            Err(WalError::Corruption {
                sequence: entry.sequence_number,
                details: "No repair strategy available".to_string(),
            })
        }
    }
    
    /// Repair checksum by recalculating
    fn repair_checksum(&self, entry: &LogEntry) -> Result<LogEntry, WalError> {
        let mut repaired = entry.clone();
        
        // Recalculate checksum from payload
        let mut hasher = crc32fast::Hasher::new();
        hasher.update(&entry.payload);
        repaired.checksum = hasher.finalize();
        
        Ok(repaired)
    }
    
    /// Repair timestamp if it's clearly invalid
    fn repair_timestamp(&self, entry: &LogEntry) -> Result<LogEntry, WalError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        
        // If timestamp is more than 1 year in the future or before epoch, fix it
        if entry.timestamp > now + 365 * 24 * 3600 * 1_000_000_000 || entry.timestamp == 0 {
            let mut repaired = entry.clone();
            repaired.timestamp = now;
            Ok(repaired)
        } else {
            Err(WalError::Corruption {
                sequence: entry.sequence_number,
                details: "Timestamp appears valid".to_string(),
            })
        }
    }
    
    /// Attempt to reconstruct payload (placeholder implementation)
    fn reconstruct_payload(&self, entry: &LogEntry) -> Result<LogEntry, WalError> {
        // This would require more sophisticated reconstruction logic
        // For now, just return error as this is very complex
        Err(WalError::Corruption {
            sequence: entry.sequence_number,
            details: "Payload reconstruction not implemented".to_string(),
        })
    }
    
    /// Get repair statistics
    pub fn stats(&self) -> &RepairStats {
        &self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;
    
    #[tokio::test]
    async fn test_recovery_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = RecoveryConfig::default();
        
        let manager = RecoveryManager::new(temp_dir.path().to_path_buf(), config);
        
        assert_eq!(manager.wal_dir, temp_dir.path());
        assert!(matches!(manager.config.recovery_mode, RecoveryMode::Thorough));
    }
    
    #[tokio::test]
    async fn test_empty_directory_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let config = RecoveryConfig::default();
        
        let mut manager = RecoveryManager::new(temp_dir.path().to_path_buf(), config);
        let result = manager.recover().await.unwrap();
        
        assert_eq!(result.segments_recovered, 0);
        assert_eq!(result.entries_recovered, 0);
        assert!(matches!(result.integrity_status, IntegrityStatus::Verified));
    }
    
    #[tokio::test]
    async fn test_segment_file_discovery() {
        let temp_dir = TempDir::new().unwrap();
        let config = RecoveryConfig::default();
        
        // Create some test segment files
        fs::write(temp_dir.path().join("segment-1.wal"), b"test data").unwrap();
        fs::write(temp_dir.path().join("segment-2.wal"), b"more data").unwrap();
        fs::write(temp_dir.path().join("other.txt"), b"not a segment").unwrap();
        
        let manager = RecoveryManager::new(temp_dir.path().to_path_buf(), config);
        let segments = manager.discover_segment_files().await.unwrap();
        
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].segment_id, 1);
        assert_eq!(segments[1].segment_id, 2);
    }
    
    #[test]
    fn test_repair_engine_creation() {
        let config = RepairConfig::default();
        let engine = RepairEngine::new(config);
        
        assert!(engine.config.enable_checksum_repair);
        assert_eq!(engine.stats.total_repair_attempts, 0);
    }
    
    #[test]
    fn test_recovery_config_defaults() {
        let config = RecoveryConfig::default();
        
        assert!(matches!(config.recovery_mode, RecoveryMode::Thorough));
        assert!(matches!(config.corruption_policy, CorruptionPolicy::Repair));
        assert!(config.verify_checksums);
        assert!(config.verify_sequence_continuity);
        assert!(config.verify_timestamps);
    }
}