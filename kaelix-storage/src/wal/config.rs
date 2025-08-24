//! WAL configuration and settings
//!
//! This module provides comprehensive configuration options for the Write-Ahead Log,
//! allowing fine-tuning of performance, durability, and operational characteristics.

use std::{path::PathBuf, time::Duration};

/// WAL Configuration with performance and durability tuning options
///
/// Provides extensive configuration options for optimizing WAL performance
/// based on specific deployment scenarios and requirements.
#[derive(Debug, Clone)]
pub struct WalConfig {
    /// Directory for WAL files
    pub wal_dir: PathBuf,

    /// Maximum size per segment in bytes (default: 64MB)
    pub max_segment_size: u64,

    /// Maximum age of a segment before rotation (default: 1 hour)
    pub max_segment_age: Duration,

    /// Maximum batch size for batch operations (default: 1000)
    pub max_batch_size: usize,

    /// Maximum time to wait before flushing a batch (default: 1ms)
    pub batch_timeout: Duration,

    /// Durability and synchronization policy
    pub sync_policy: SyncPolicy,

    /// Enable memory-mapped I/O for maximum performance (default: true)
    pub use_memory_mapping: bool,

    /// Enable direct I/O to bypass page cache (default: false)
    pub use_direct_io: bool,

    /// Buffer size for I/O operations in bytes (default: 4MB)
    pub buffer_size: usize,
}

/// Synchronization policy for durability guarantees
///
/// Defines when and how the WAL performs fsync operations to ensure
/// data durability with different performance trade-offs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncPolicy {
    /// Never perform fsync - maximum performance, no durability guarantee
    Never,

    /// Fsync after each batch - good balance of performance and durability
    OnBatch,

    /// Fsync at regular intervals - configurable durability with good performance
    Interval(Duration),

    /// Fsync only on explicit flush or shutdown - manual durability control
    OnShutdown,

    /// Fsync after every write - maximum durability, lowest performance
    Always,
}

impl Default for WalConfig {
    /// Create a default WAL configuration optimized for high performance
    fn default() -> Self {
        Self {
            wal_dir: PathBuf::from("./wal"),
            max_segment_size: 64 * 1024 * 1024, // 64MB
            max_segment_age: Duration::from_secs(3600), // 1 hour
            max_batch_size: 1000,
            batch_timeout: Duration::from_millis(1),
            sync_policy: SyncPolicy::OnBatch,
            use_memory_mapping: true,
            use_direct_io: false,
            buffer_size: 4 * 1024 * 1024, // 4MB
        }
    }
}

impl WalConfig {
    /// Create a new WAL configuration with the specified directory
    ///
    /// # Parameters
    ///
    /// * `wal_dir` - Directory path for WAL files
    ///
    /// # Returns
    ///
    /// A new [`WalConfig`] with default settings and the specified directory.
    pub fn new<P: Into<PathBuf>>(wal_dir: P) -> Self {
        Self {
            wal_dir: wal_dir.into(),
            ..Default::default()
        }
    }

    /// Create a configuration optimized for maximum performance
    ///
    /// - Never syncs to disk (no durability guarantee)
    /// - Large batches for high throughput
    /// - Memory mapping enabled
    /// - Minimal batch timeout
    ///
    /// # Parameters
    ///
    /// * `wal_dir` - Directory path for WAL files
    ///
    /// # Returns
    ///
    /// Performance-optimized [`WalConfig`].
    pub fn performance_optimized<P: Into<PathBuf>>(wal_dir: P) -> Self {
        Self {
            wal_dir: wal_dir.into(),
            max_segment_size: 128 * 1024 * 1024, // 128MB for fewer rotations
            max_segment_age: Duration::from_secs(7200), // 2 hours
            max_batch_size: 10000, // Large batches
            batch_timeout: Duration::from_micros(100), // Very short timeout
            sync_policy: SyncPolicy::Never, // No fsync
            use_memory_mapping: true,
            use_direct_io: false,
            buffer_size: 8 * 1024 * 1024, // 8MB buffer
        }
    }

    /// Create a configuration optimized for durability
    ///
    /// - Syncs frequently for maximum durability
    /// - Smaller segments for faster recovery
    /// - Conservative batch sizes
    /// - Direct I/O for immediate persistence
    ///
    /// # Parameters
    ///
    /// * `wal_dir` - Directory path for WAL files
    ///
    /// # Returns
    ///
    /// Durability-optimized [`WalConfig`].
    pub fn durability_optimized<P: Into<PathBuf>>(wal_dir: P) -> Self {
        Self {
            wal_dir: wal_dir.into(),
            max_segment_size: 32 * 1024 * 1024, // 32MB for faster recovery
            max_segment_age: Duration::from_secs(1800), // 30 minutes
            max_batch_size: 100, // Smaller batches
            batch_timeout: Duration::from_millis(5), // Quick flush
            sync_policy: SyncPolicy::OnBatch, // Sync after every batch
            use_memory_mapping: false, // Disable mmap for immediate writes
            use_direct_io: true, // Bypass page cache
            buffer_size: 1024 * 1024, // 1MB buffer
        }
    }

    /// Create a configuration balanced between performance and durability
    ///
    /// - Periodic syncing for reasonable durability
    /// - Medium-sized batches and segments
    /// - Memory mapping enabled with periodic sync
    ///
    /// # Parameters
    ///
    /// * `wal_dir` - Directory path for WAL files
    ///
    /// # Returns
    ///
    /// Balanced [`WalConfig`].
    pub fn balanced<P: Into<PathBuf>>(wal_dir: P) -> Self {
        Self {
            wal_dir: wal_dir.into(),
            max_segment_size: 64 * 1024 * 1024, // 64MB
            max_segment_age: Duration::from_secs(3600), // 1 hour
            max_batch_size: 1000,
            batch_timeout: Duration::from_millis(1),
            sync_policy: SyncPolicy::Interval(Duration::from_millis(100)), // Sync every 100ms
            use_memory_mapping: true,
            use_direct_io: false,
            buffer_size: 4 * 1024 * 1024, // 4MB
        }
    }

    /// Create a configuration optimized for development/testing
    ///
    /// - Small segments for quick testing
    /// - Frequent rotation for testing rotation logic
    /// - No memory mapping to avoid test artifacts
    /// - Fast sync for quick test completion
    ///
    /// # Parameters
    ///
    /// * `wal_dir` - Directory path for WAL files
    ///
    /// # Returns
    ///
    /// Development-optimized [`WalConfig`].
    pub fn development<P: Into<PathBuf>>(wal_dir: P) -> Self {
        Self {
            wal_dir: wal_dir.into(),
            max_segment_size: 1024 * 1024, // 1MB for quick rotation
            max_segment_age: Duration::from_secs(60), // 1 minute
            max_batch_size: 10, // Small batches
            batch_timeout: Duration::from_millis(10),
            sync_policy: SyncPolicy::OnBatch,
            use_memory_mapping: false, // Avoid mmap in tests
            use_direct_io: false,
            buffer_size: 64 * 1024, // 64KB buffer
        }
    }

    /// Validate the configuration for correctness
    ///
    /// # Returns
    ///
    /// `Ok(())` if configuration is valid, `Err(String)` with error description otherwise.
    pub fn validate(&self) -> Result<(), String> {
        // Validate directory path
        if self.wal_dir.as_os_str().is_empty() {
            return Err("WAL directory path cannot be empty".to_string());
        }

        // Validate segment size
        if self.max_segment_size == 0 {
            return Err("Maximum segment size must be greater than 0".to_string());
        }

        if self.max_segment_size < 1024 {
            return Err("Maximum segment size should be at least 1KB".to_string());
        }

        // Validate batch settings
        if self.max_batch_size == 0 {
            return Err("Maximum batch size must be greater than 0".to_string());
        }

        if self.max_batch_size > 100_000 {
            return Err("Maximum batch size should not exceed 100,000 entries".to_string());
        }

        // Validate timeout
        if self.batch_timeout.is_zero() {
            return Err("Batch timeout must be greater than 0".to_string());
        }

        // Validate buffer size
        if self.buffer_size == 0 {
            return Err("Buffer size must be greater than 0".to_string());
        }

        if self.buffer_size < 4096 {
            return Err("Buffer size should be at least 4KB for efficiency".to_string());
        }

        // Validate segment age
        if self.max_segment_age.is_zero() {
            return Err("Maximum segment age must be greater than 0".to_string());
        }

        // Validate sync policy
        if let SyncPolicy::Interval(interval) = self.sync_policy {
            if interval.is_zero() {
                return Err("Sync interval must be greater than 0".to_string());
            }
        }

        Ok(())
    }

    /// Set the WAL directory
    ///
    /// # Parameters
    ///
    /// * `dir` - New directory path
    ///
    /// # Returns
    ///
    /// Modified configuration for method chaining.
    pub fn with_wal_dir<P: Into<PathBuf>>(mut self, dir: P) -> Self {
        self.wal_dir = dir.into();
        self
    }

    /// Set the maximum segment size
    ///
    /// # Parameters
    ///
    /// * `size` - Maximum segment size in bytes
    ///
    /// # Returns
    ///
    /// Modified configuration for method chaining.
    pub fn with_max_segment_size(mut self, size: u64) -> Self {
        self.max_segment_size = size;
        self
    }

    /// Set the maximum segment age
    ///
    /// # Parameters
    ///
    /// * `age` - Maximum segment age duration
    ///
    /// # Returns
    ///
    /// Modified configuration for method chaining.
    pub fn with_max_segment_age(mut self, age: Duration) -> Self {
        self.max_segment_age = age;
        self
    }

    /// Set the maximum batch size
    ///
    /// # Parameters
    ///
    /// * `size` - Maximum number of entries per batch
    ///
    /// # Returns
    ///
    /// Modified configuration for method chaining.
    pub fn with_max_batch_size(mut self, size: usize) -> Self {
        self.max_batch_size = size;
        self
    }

    /// Set the batch timeout
    ///
    /// # Parameters
    ///
    /// * `timeout` - Maximum time to wait before flushing a batch
    ///
    /// # Returns
    ///
    /// Modified configuration for method chaining.
    pub fn with_batch_timeout(mut self, timeout: Duration) -> Self {
        self.batch_timeout = timeout;
        self
    }

    /// Set the sync policy
    ///
    /// # Parameters
    ///
    /// * `policy` - Synchronization policy for durability
    ///
    /// # Returns
    ///
    /// Modified configuration for method chaining.
    pub fn with_sync_policy(mut self, policy: SyncPolicy) -> Self {
        self.sync_policy = policy;
        self
    }

    /// Enable or disable memory mapping
    ///
    /// # Parameters
    ///
    /// * `enabled` - Whether to enable memory-mapped I/O
    ///
    /// # Returns
    ///
    /// Modified configuration for method chaining.
    pub fn with_memory_mapping(mut self, enabled: bool) -> Self {
        self.use_memory_mapping = enabled;
        self
    }

    /// Enable or disable direct I/O
    ///
    /// # Parameters
    ///
    /// * `enabled` - Whether to enable direct I/O
    ///
    /// # Returns
    ///
    /// Modified configuration for method chaining.
    pub fn with_direct_io(mut self, enabled: bool) -> Self {
        self.use_direct_io = enabled;
        self
    }

    /// Set the buffer size
    ///
    /// # Parameters
    ///
    /// * `size` - Buffer size in bytes
    ///
    /// # Returns
    ///
    /// Modified configuration for method chaining.
    pub fn with_buffer_size(mut self, size: usize) -> Self {
        self.buffer_size = size;
        self
    }

    /// Get estimated memory usage for this configuration
    ///
    /// # Returns
    ///
    /// Estimated memory usage in bytes.
    #[must_use]
    pub fn estimated_memory_usage(&self) -> usize {
        let mut usage = 0;

        // Buffer memory
        usage += self.buffer_size;

        // Batch buffer (rough estimate)
        usage += self.max_batch_size * 1024; // Assume 1KB per entry

        // Memory mapping overhead (if enabled)
        if self.use_memory_mapping {
            usage += self.max_segment_size as usize;
        }

        // Base WAL structure overhead
        usage += 8192; // Rough estimate

        usage
    }

    /// Get estimated disk space usage per day
    ///
    /// # Parameters
    ///
    /// * `messages_per_second` - Expected message rate
    /// * `avg_message_size` - Average message size in bytes
    ///
    /// # Returns
    ///
    /// Estimated disk usage per day in bytes.
    #[must_use]
    pub fn estimated_daily_disk_usage(&self, messages_per_second: u64, avg_message_size: usize) -> u64 {
        let seconds_per_day = 24 * 60 * 60;
        let messages_per_day = messages_per_second * seconds_per_day;
        let header_size = 24; // Fixed header size
        let entry_size = header_size + avg_message_size;
        
        messages_per_day * entry_size as u64
    }
}

impl SyncPolicy {
    /// Check if this policy requires immediate synchronization
    #[must_use]
    pub fn is_immediate(&self) -> bool {
        matches!(self, Self::Always)
    }

    /// Check if this policy never synchronizes
    #[must_use]
    pub fn is_never(&self) -> bool {
        matches!(self, Self::Never)
    }

    /// Get the sync interval if applicable
    #[must_use]
    pub fn interval(&self) -> Option<Duration> {
        match self {
            Self::Interval(duration) => Some(*duration),
            _ => None,
        }
    }

    /// Get a human-readable description of the policy
    #[must_use]
    pub fn description(&self) -> &'static str {
        match self {
            Self::Never => "Never sync (maximum performance, no durability)",
            Self::OnBatch => "Sync after each batch (balanced performance/durability)",
            Self::Interval(_) => "Sync at regular intervals (configurable durability)",
            Self::OnShutdown => "Sync only on shutdown (manual durability control)",
            Self::Always => "Sync after every write (maximum durability)",
        }
    }
}

impl std::fmt::Display for SyncPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Never => write!(f, "Never"),
            Self::OnBatch => write!(f, "OnBatch"),
            Self::Interval(duration) => write!(f, "Interval({}ms)", duration.as_millis()),
            Self::OnShutdown => write!(f, "OnShutdown"),
            Self::Always => write!(f, "Always"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = WalConfig::default();
        
        assert_eq!(config.wal_dir, PathBuf::from("./wal"));
        assert_eq!(config.max_segment_size, 64 * 1024 * 1024);
        assert_eq!(config.max_segment_age, Duration::from_secs(3600));
        assert_eq!(config.max_batch_size, 1000);
        assert_eq!(config.batch_timeout, Duration::from_millis(1));
        assert_eq!(config.sync_policy, SyncPolicy::OnBatch);
        assert!(config.use_memory_mapping);
        assert!(!config.use_direct_io);
        assert_eq!(config.buffer_size, 4 * 1024 * 1024);
    }

    #[test]
    fn test_config_validation() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig::new(temp_dir.path());
        assert!(config.validate().is_ok());

        // Test invalid configurations
        let invalid_config = WalConfig {
            max_segment_size: 0,
            ..config.clone()
        };
        assert!(invalid_config.validate().is_err());

        let invalid_config = WalConfig {
            max_batch_size: 0,
            ..config.clone()
        };
        assert!(invalid_config.validate().is_err());

        let invalid_config = WalConfig {
            buffer_size: 0,
            ..config
        };
        assert!(invalid_config.validate().is_err());
    }

    #[test]
    fn test_performance_optimized_config() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig::performance_optimized(temp_dir.path());
        
        assert_eq!(config.sync_policy, SyncPolicy::Never);
        assert!(config.use_memory_mapping);
        assert_eq!(config.max_batch_size, 10000);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_durability_optimized_config() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig::durability_optimized(temp_dir.path());
        
        assert_eq!(config.sync_policy, SyncPolicy::OnBatch);
        assert!(!config.use_memory_mapping);
        assert!(config.use_direct_io);
        assert_eq!(config.max_batch_size, 100);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_balanced_config() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig::balanced(temp_dir.path());
        
        match config.sync_policy {
            SyncPolicy::Interval(duration) => assert_eq!(duration, Duration::from_millis(100)),
            _ => panic!("Expected interval sync policy"),
        }
        assert!(config.use_memory_mapping);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_development_config() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig::development(temp_dir.path());
        
        assert_eq!(config.max_segment_size, 1024 * 1024);
        assert!(!config.use_memory_mapping);
        assert_eq!(config.max_batch_size, 10);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_builder_pattern() {
        let temp_dir = TempDir::new().unwrap();
        let config = WalConfig::new(temp_dir.path())
            .with_max_segment_size(128 * 1024 * 1024)
            .with_max_batch_size(5000)
            .with_sync_policy(SyncPolicy::Never)
            .with_memory_mapping(false);

        assert_eq!(config.max_segment_size, 128 * 1024 * 1024);
        assert_eq!(config.max_batch_size, 5000);
        assert_eq!(config.sync_policy, SyncPolicy::Never);
        assert!(!config.use_memory_mapping);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_sync_policy_properties() {
        assert!(SyncPolicy::Always.is_immediate());
        assert!(!SyncPolicy::Never.is_immediate());
        assert!(SyncPolicy::Never.is_never());
        assert!(!SyncPolicy::Always.is_never());

        let interval_policy = SyncPolicy::Interval(Duration::from_millis(100));
        assert_eq!(interval_policy.interval(), Some(Duration::from_millis(100)));
        assert!(SyncPolicy::Never.interval().is_none());
    }

    #[test]
    fn test_sync_policy_display() {
        assert_eq!(format!("{}", SyncPolicy::Never), "Never");
        assert_eq!(format!("{}", SyncPolicy::OnBatch), "OnBatch");
        assert_eq!(format!("{}", SyncPolicy::Always), "Always");
        assert_eq!(
            format!("{}", SyncPolicy::Interval(Duration::from_millis(500))),
            "Interval(500ms)"
        );
    }

    #[test]
    fn test_memory_usage_estimation() {
        let config = WalConfig::default();
        let usage = config.estimated_memory_usage();
        
        // Should include buffer size, batch buffer, and overhead
        assert!(usage > config.buffer_size);
        assert!(usage > 1024 * 1024); // Should be at least 1MB
    }

    #[test]
    fn test_disk_usage_estimation() {
        let config = WalConfig::default();
        let usage = config.estimated_daily_disk_usage(1000, 512); // 1000 msgs/sec, 512 bytes avg
        
        // Should be reasonable: 1000 * 86400 * (24 + 512) bytes per day
        let expected = 1000 * 86400 * (24 + 512);
        assert_eq!(usage, expected);
    }
}