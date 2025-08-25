use serde::{Deserialize, Serialize, Deserializer, Serializer};
use std::{path::PathBuf, time::Duration};

/// WAL Configuration with performance and durability tuning options
///
/// Provides extensive configuration options for optimizing WAL performance
/// based on specific deployment scenarios and requirements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalConfig {
    /// Directory for WAL files
    pub wal_dir: PathBuf,

    /// Maximum size per segment in bytes (default: 64MB)
    pub segment_size: u64,

    /// Sync policy for durability vs performance trade-offs
    pub sync_policy: SyncPolicy,

    /// Enable memory-mapped I/O for segments
    pub use_mmap: bool,

    /// Buffer size for write operations (default: 8KB)
    pub buffer_size: u64,

    /// Maximum batch size before forced flush (default: 1000 entries)
    pub max_batch_size: u64,

    /// Maximum time to wait before forced flush
    #[serde(with = "duration_millis")]
    pub max_batch_wait: Duration,

    /// Enable compression for segment data
    pub enable_compression: bool,

    /// Compression algorithm to use
    pub compression_algorithm: CompressionAlgorithm,

    /// Level of compression to apply (0-9, higher = more compression)
    pub compression_level: u32,

    /// Number of background threads for async operations (default: 2)
    pub async_threads: u32,

    /// Enable asynchronous writes to reduce blocking
    pub async_writes: bool,

    /// Rotation configuration
    pub rotation: super::rotation::RotationConfig,
}

/// Synchronization policy for write operations
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SyncPolicy {
    /// No explicit sync calls (OS decides when to flush)
    Never,
    /// Sync on each write (slowest, most durable)
    Always,
    /// Sync on each batch write
    OnBatch,
    /// Sync at regular intervals
    Interval(#[serde(with = "duration_millis")] Duration),
    /// Sync only on shutdown
    OnShutdown,
}

/// Compression algorithms supported by WAL
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CompressionAlgorithm {
    /// No compression
    None,
    /// LZ4 (fast compression/decompression)
    Lz4,
    /// Zstd (balanced compression ratio and speed)
    Zstd,
    /// Gzip (high compression ratio, slower)
    Gzip,
}

/// Serialize/deserialize Duration as milliseconds
mod duration_millis {
    use super::*;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(duration.as_millis() as u64)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = <u64 as Deserialize>::deserialize(deserializer)?;
        Ok(Duration::from_millis(millis))
    }
}

impl Default for WalConfig {
    fn default() -> Self {
        Self {
            wal_dir: PathBuf::from("./wal"),
            segment_size: 64 * 1024 * 1024, // 64MB
            sync_policy: SyncPolicy::OnBatch,
            use_mmap: true,
            buffer_size: 8 * 1024, // 8KB
            max_batch_size: 1000,
            max_batch_wait: Duration::from_millis(10),
            enable_compression: false,
            compression_algorithm: CompressionAlgorithm::Lz4,
            compression_level: 1,
            async_threads: 2,
            async_writes: true,
            rotation: super::rotation::RotationConfig::default(),
        }
    }
}

impl WalConfig {
    /// Create a new WAL configuration optimized for high performance
    pub fn high_performance() -> Self {
        Self {
            sync_policy: SyncPolicy::OnBatch,
            use_mmap: true,
            buffer_size: 64 * 1024, // 64KB
            max_batch_size: 10000,
            max_batch_wait: Duration::from_millis(1), // 1ms
            enable_compression: false,
            async_writes: true,
            async_threads: 4,
            ..Default::default()
        }
    }

    /// Create a new WAL configuration optimized for maximum durability
    pub fn max_durability() -> Self {
        Self {
            sync_policy: SyncPolicy::Always,
            use_mmap: false,
            buffer_size: 4 * 1024, // 4KB
            max_batch_size: 100,
            max_batch_wait: Duration::from_millis(1),
            enable_compression: true,
            compression_algorithm: CompressionAlgorithm::Zstd,
            compression_level: 3,
            async_writes: false,
            async_threads: 1,
            ..Default::default()
        }
    }

    /// Create a new WAL configuration optimized for space efficiency
    pub fn space_efficient() -> Self {
        Self {
            sync_policy: SyncPolicy::Interval(Duration::from_millis(100)),
            use_mmap: true,
            enable_compression: true,
            compression_algorithm: CompressionAlgorithm::Zstd,
            compression_level: 6,
            segment_size: 32 * 1024 * 1024, // 32MB
            ..Default::default()
        }
    }

    /// Validate configuration parameters
    pub fn validate(&self) -> Result<(), String> {
        if self.segment_size == 0 {
            return Err("Segment size must be greater than 0".to_string());
        }

        if self.buffer_size == 0 {
            return Err("Buffer size must be greater than 0".to_string());
        }

        if self.max_batch_size == 0 {
            return Err("Max batch size must be greater than 0".to_string());
        }

        if self.async_threads == 0 {
            return Err("Async threads must be greater than 0".to_string());
        }

        if self.compression_level > 9 {
            return Err("Compression level must be between 0 and 9".to_string());
        }

        // Validate directory exists or can be created
        if !self.wal_dir.exists() && std::fs::create_dir_all(&self.wal_dir).is_err() {
            return Err(format!("Cannot create WAL directory: {}", self.wal_dir.display()));
        }

        Ok(())
    }

    /// Get recommended buffer size based on configuration
    pub fn effective_buffer_size(&self) -> u64 {
        match self.sync_policy {
            SyncPolicy::Always => self.buffer_size.min(4 * 1024), // Cap at 4KB for frequent syncs
            SyncPolicy::OnBatch => self.buffer_size,
            SyncPolicy::Interval(_) => self.buffer_size.max(16 * 1024), // Min 16KB for interval syncs
            SyncPolicy::Never => self.buffer_size.max(32 * 1024), // Min 32KB for no syncs
            SyncPolicy::OnShutdown => self.buffer_size.max(64 * 1024), // Min 64KB for shutdown-only syncs
        }
    }

    /// Get recommended batch size based on configuration
    pub fn effective_batch_size(&self) -> u64 {
        match self.sync_policy {
            SyncPolicy::Always => self.max_batch_size.min(100), // Small batches for frequent syncs
            SyncPolicy::OnBatch => self.max_batch_size,
            SyncPolicy::Interval(_) => self.max_batch_size,
            SyncPolicy::Never => self.max_batch_size.max(1000), // Larger batches for no syncs
            SyncPolicy::OnShutdown => self.max_batch_size.max(5000), // Larger batches for shutdown-only syncs
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
        assert!(config.validate().is_ok());
        assert!(matches!(config.sync_policy, SyncPolicy::OnBatch));
        assert!(config.use_mmap);
        assert!(!config.enable_compression);
    }

    #[test]
    fn test_high_performance_config() {
        let config = WalConfig::high_performance();
        assert!(config.validate().is_ok());
        assert!(matches!(config.sync_policy, SyncPolicy::OnBatch));
        assert!(config.async_writes);
        assert_eq!(config.async_threads, 4);
    }

    #[test]
    fn test_max_durability_config() {
        let config = WalConfig::max_durability();
        assert!(config.validate().is_ok());
        assert!(matches!(config.sync_policy, SyncPolicy::Always));
        assert!(!config.async_writes);
        assert!(config.enable_compression);
    }

    #[test]
    fn test_space_efficient_config() {
        let config = WalConfig::space_efficient();
        assert!(config.validate().is_ok());
        assert!(config.enable_compression);
        assert!(matches!(config.compression_algorithm, CompressionAlgorithm::Zstd));
        assert_eq!(config.compression_level, 6);
        if let SyncPolicy::Interval(duration) = config.sync_policy {
            assert_eq!(duration, Duration::from_millis(100));
        } else {
            panic!("Expected Interval sync policy");
        }
    }

    #[test]
    fn test_config_validation() {
        let mut config = WalConfig::default();
        assert!(config.validate().is_ok());

        config.segment_size = 0;
        assert!(config.validate().is_err());

        config.segment_size = 1024;
        config.buffer_size = 0;
        assert!(config.validate().is_err());

        config.buffer_size = 1024;
        config.max_batch_size = 0;
        assert!(config.validate().is_err());

        config.max_batch_size = 100;
        config.compression_level = 10;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_effective_sizes() {
        let mut config = WalConfig::default();
        config.buffer_size = 16 * 1024;
        config.max_batch_size = 500;

        config.sync_policy = SyncPolicy::Always;
        assert_eq!(config.effective_buffer_size(), 4 * 1024);
        assert_eq!(config.effective_batch_size(), 100);

        config.sync_policy = SyncPolicy::OnBatch;
        assert_eq!(config.effective_buffer_size(), 16 * 1024);
        assert_eq!(config.effective_batch_size(), 500);

        config.sync_policy = SyncPolicy::Never;
        assert!(config.effective_buffer_size() >= 32 * 1024);
        assert!(config.effective_batch_size() >= 1000);
    }

    #[test]
    fn test_serialization() {
        let config = WalConfig::default();
        let serialized = serde_json::to_string(&config).unwrap();
        let deserialized: WalConfig = serde_json::from_str(&serialized).unwrap();
        
        assert_eq!(config.segment_size, deserialized.segment_size);
        assert_eq!(config.buffer_size, deserialized.buffer_size);
        assert_eq!(config.max_batch_wait, deserialized.max_batch_wait);
    }

    #[test]
    fn test_directory_validation() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = WalConfig::default();
        config.wal_dir = temp_dir.path().to_path_buf();
        
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_sync_policy_serialization() {
        let policies = vec![
            SyncPolicy::Never,
            SyncPolicy::Always,
            SyncPolicy::OnBatch,
            SyncPolicy::Interval(Duration::from_millis(100)),
            SyncPolicy::OnShutdown,
        ];

        for policy in policies {
            let serialized = serde_json::to_string(&policy).unwrap();
            let deserialized: SyncPolicy = serde_json::from_str(&serialized).unwrap();
            
            match (policy, deserialized) {
                (SyncPolicy::Never, SyncPolicy::Never) => {},
                (SyncPolicy::Always, SyncPolicy::Always) => {},
                (SyncPolicy::OnBatch, SyncPolicy::OnBatch) => {},
                (SyncPolicy::Interval(d1), SyncPolicy::Interval(d2)) => assert_eq!(d1, d2),
                (SyncPolicy::OnShutdown, SyncPolicy::OnShutdown) => {},
                _ => panic!("Sync policy serialization mismatch"),
            }
        }
    }
}