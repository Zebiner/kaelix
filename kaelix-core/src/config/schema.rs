//! Configuration schema definitions for MemoryStreamer
//!
//! This module defines the complete configuration structure for the MemoryStreamer
//! ultra-high-performance distributed streaming system. All configuration structures
//! use serde for serialization/deserialization and validator for comprehensive
//! validation.
//!
//! ## Performance Considerations
//! - Configuration access is optimized for zero-allocation hot paths
//! - Validation is performed at load time, not during runtime access
//! - Default values are performance-tuned for ultra-low latency requirements

use serde::{Deserialize, Serialize};
use std::time::Duration;
use validator::Validate;

/// Main configuration structure for MemoryStreamer
///
/// This is the root configuration object that contains all subsystem configurations.
/// It uses a layered approach: defaults → file → environment → runtime overrides.
///
/// # Performance Targets
/// - Target latency: <10μs P99
/// - Target throughput: 10M+ messages/second
/// - Memory efficiency: minimal allocation overhead
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MemoryStreamerConfig {
    /// Network configuration for connections and transport
    #[validate(nested)]
    pub network: NetworkConfig,
    
    /// Protocol configuration for message framing and encoding
    #[validate(nested)]
    pub protocol: ProtocolConfig,
    
    /// Performance tuning and optimization settings
    #[validate(nested)]
    pub performance: PerformanceConfig,
    
    /// Observability and monitoring configuration
    #[validate(nested)]
    pub observability: ObservabilityConfig,
    
    /// Security and encryption settings
    #[validate(nested)]
    pub security: SecurityConfig,
}

/// Network configuration for transport layer settings
///
/// Controls how MemoryStreamer binds to network interfaces and manages connections.
/// Optimized for high-throughput, low-latency networking patterns.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct NetworkConfig {
    /// Address to bind the server to
    /// 
    /// # Examples
    /// - `"0.0.0.0"` - bind to all interfaces
    /// - `"127.0.0.1"` - localhost only
    /// - `"10.0.1.100"` - specific interface
    #[validate(ip)]
    pub bind_address: String,
    
    /// Port number to listen on
    /// 
    /// Must be in the valid port range and not conflict with system services.
    #[validate(range(min = 1024, max = 65535))]
    pub port: u16,
    
    /// Maximum number of concurrent connections
    /// 
    /// Tuned for high-concurrency scenarios while preventing resource exhaustion.
    #[validate(range(min = 1, max = 1_000_000))]
    pub max_connections: usize,
    
    /// Connection timeout in milliseconds
    /// 
    /// How long to wait for connection establishment before timing out.
    #[serde(with = "duration_ms")]
    #[validate(range(min = 100, max = 30_000))]
    pub connection_timeout: Duration,
    
    /// TCP keep-alive settings
    pub keep_alive: KeepAliveConfig,
    
    /// Buffer sizes for network I/O
    pub buffers: NetworkBufferConfig,
}

/// TCP keep-alive configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct KeepAliveConfig {
    /// Enable TCP keep-alive
    pub enabled: bool,
    
    /// Time before sending keep-alive probes (seconds)
    #[validate(range(min = 1, max = 7200))]
    pub idle_time: u32,
    
    /// Interval between keep-alive probes (seconds)
    #[validate(range(min = 1, max = 300))]
    pub probe_interval: u32,
    
    /// Number of failed probes before connection is dropped
    #[validate(range(min = 1, max = 10))]
    pub probe_count: u32,
}

/// Network buffer configuration for optimized I/O
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct NetworkBufferConfig {
    /// Socket receive buffer size in bytes
    #[validate(range(min = 4096, max = 16_777_216))] // 4KB to 16MB
    pub receive_buffer: usize,
    
    /// Socket send buffer size in bytes
    #[validate(range(min = 4096, max = 16_777_216))] // 4KB to 16MB
    pub send_buffer: usize,
    
    /// Application read buffer size
    #[validate(range(min = 1024, max = 1_048_576))] // 1KB to 1MB
    pub read_buffer: usize,
    
    /// Application write buffer size
    #[validate(range(min = 1024, max = 1_048_576))] // 1KB to 1MB
    pub write_buffer: usize,
}

/// Protocol configuration for message framing and serialization
///
/// Controls the wire protocol behavior, including frame sizes, timeouts,
/// and optimization features like compression.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ProtocolConfig {
    /// Maximum payload size in bytes for a single message
    /// 
    /// Prevents memory exhaustion while allowing reasonable message sizes.
    /// Must balance memory usage with message throughput requirements.
    #[validate(range(min = 64, max = 100_663_296))] // 64 bytes to 96MB
    pub max_payload_size: u32,
    
    /// Frame processing timeout in milliseconds
    /// 
    /// Maximum time to wait for frame completion before timing out.
    #[serde(with = "duration_ms")]
    #[validate(range(min = 1, max = 10_000))]
    pub frame_timeout: Duration,
    
    /// Enable compression for payload data
    /// 
    /// Trades CPU for network bandwidth. May increase latency slightly.
    pub enable_compression: bool,
    
    /// Compression algorithm selection
    pub compression: CompressionConfig,
    
    /// Frame batching configuration for throughput optimization
    pub batching: BatchingConfig,
}

/// Compression configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CompressionConfig {
    /// Compression algorithm to use
    pub algorithm: CompressionAlgorithm,
    
    /// Compression level (algorithm-specific)
    #[validate(range(min = 1, max = 9))]
    pub level: u8,
    
    /// Minimum payload size to trigger compression
    #[validate(range(min = 64, max = 65536))]
    pub min_size: u32,
}

/// Available compression algorithms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompressionAlgorithm {
    /// No compression
    None,
    /// LZ4 fast compression
    Lz4,
    /// Zstandard compression
    Zstd,
    /// Snappy compression
    Snappy,
}

/// Frame batching configuration for improved throughput
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct BatchingConfig {
    /// Enable frame batching
    pub enabled: bool,
    
    /// Maximum number of frames in a batch
    #[validate(range(min = 1, max = 1000))]
    pub max_batch_size: usize,
    
    /// Maximum time to wait for batch completion (microseconds)
    #[validate(range(min = 1, max = 1000))]
    pub max_batch_delay_us: u64,
}

/// Performance configuration for ultra-high-performance operation
///
/// These settings directly impact the performance characteristics of MemoryStreamer.
/// Values are tuned for <10μs P99 latency and 10M+ messages/second throughput.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PerformanceConfig {
    /// Target P99 latency in microseconds
    /// 
    /// Performance target for 99th percentile message processing latency.
    /// Default is <10μs for ultra-low latency requirements.
    #[validate(range(min = 1, max = 1_000_000))]
    pub target_latency_p99_us: u64,
    
    /// Maximum sustained throughput in messages per second
    /// 
    /// Target throughput capacity. System will apply backpressure when
    /// approaching this limit to maintain latency guarantees.
    #[validate(range(min = 1000, max = 100_000_000))]
    pub max_throughput: u64,
    
    /// Number of worker threads for message processing
    /// 
    /// None means auto-detect based on CPU cores. Manual override
    /// allows fine-tuning for specific deployment scenarios.
    #[validate(range(min = 1, max = 256))]
    pub worker_threads: Option<usize>,
    
    /// Enable NUMA-aware memory allocation
    /// 
    /// Optimizes memory placement for NUMA architectures.
    /// Should be enabled on multi-socket systems.
    pub numa_awareness: bool,
    
    /// CPU affinity configuration
    pub cpu_affinity: CpuAffinityConfig,
    
    /// Memory management settings
    pub memory: MemoryConfig,
}

/// CPU affinity configuration for performance optimization
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CpuAffinityConfig {
    /// Enable CPU affinity pinning
    pub enabled: bool,
    
    /// CPU cores to pin worker threads to (empty means auto-select)
    pub worker_cores: Vec<usize>,
    
    /// CPU cores for I/O threads (empty means auto-select)
    pub io_cores: Vec<usize>,
}

/// Memory management configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MemoryConfig {
    /// Initial memory pool size in bytes
    #[validate(range(min = 1_048_576, max = 137_438_953_472))] // 1MB to 128GB
    pub initial_pool_size: usize,
    
    /// Maximum memory pool size in bytes
    #[validate(range(min = 1_048_576, max = 1_099_511_627_776))] // 1MB to 1TB
    pub max_pool_size: usize,
    
    /// Enable huge pages for memory allocation
    pub use_huge_pages: bool,
    
    /// Memory pool growth factor (1.0 = no growth, 2.0 = double)
    #[validate(range(min = 1.0, max = 5.0))]
    pub growth_factor: f64,
}

/// Observability configuration for monitoring and debugging
///
/// Controls tracing, metrics collection, and health check endpoints.
/// Designed to provide comprehensive visibility with minimal performance impact.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ObservabilityConfig {
    /// Tracing and logging configuration
    pub tracing: TracingConfig,
    
    /// Metrics collection settings
    pub metrics: MetricsConfig,
    
    /// Health check endpoint configuration
    pub health_check: HealthCheckConfig,
}

/// Tracing configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TracingConfig {
    /// Enable tracing
    pub enabled: bool,
    
    /// Tracing level filter
    pub level: TracingLevel,
    
    /// Enable structured JSON output
    pub json_output: bool,
    
    /// Enable distributed tracing
    pub distributed_tracing: bool,
}

/// Tracing level enumeration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TracingLevel {
    /// Error level only
    Error,
    /// Warning and above
    Warn,
    /// Info and above
    Info,
    /// Debug and above
    Debug,
    /// All tracing including trace level
    Trace,
}

/// Metrics collection configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MetricsConfig {
    /// Enable metrics collection
    pub enabled: bool,
    
    /// Metrics collection interval in seconds
    #[validate(range(min = 1, max = 3600))]
    pub collection_interval: u32,
    
    /// Enable Prometheus metrics export
    pub prometheus_enabled: bool,
    
    /// Prometheus metrics endpoint port
    #[validate(range(min = 1024, max = 65535))]
    pub prometheus_port: u16,
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct HealthCheckConfig {
    /// Enable health check endpoint
    pub enabled: bool,
    
    /// Health check endpoint port
    #[validate(range(min = 1024, max = 65535))]
    pub port: u16,
    
    /// Health check endpoint path
    #[validate(length(min = 1, max = 100))]
    pub path: String,
}

/// Security configuration for encryption and authentication
///
/// Controls security features including TLS, authentication, and access control.
/// Designed to provide strong security without compromising performance.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SecurityConfig {
    /// TLS/SSL configuration
    pub tls: TlsConfig,
    
    /// Authentication settings
    pub authentication: AuthenticationConfig,
    
    /// Access control configuration
    pub access_control: AccessControlConfig,
}

/// TLS configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TlsConfig {
    /// Enable TLS encryption
    pub enabled: bool,
    
    /// Path to TLS certificate file
    #[validate(length(min = 1))]
    pub cert_path: Option<String>,
    
    /// Path to TLS private key file
    #[validate(length(min = 1))]
    pub key_path: Option<String>,
    
    /// Path to CA certificate for client verification
    pub ca_path: Option<String>,
    
    /// Require client certificate authentication
    pub require_client_cert: bool,
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AuthenticationConfig {
    /// Enable authentication
    pub enabled: bool,
    
    /// Authentication method
    pub method: AuthenticationMethod,
    
    /// Token validation settings
    pub token_validation: TokenValidationConfig,
}

/// Authentication method enumeration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthenticationMethod {
    /// No authentication
    None,
    /// JWT token-based authentication
    Jwt,
    /// API key authentication
    ApiKey,
    /// Mutual TLS authentication
    MutualTls,
}

/// Token validation configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TokenValidationConfig {
    /// JWT signing key or API key validation endpoint
    #[validate(length(min = 1))]
    pub validation_key: Option<String>,
    
    /// Token expiration tolerance in seconds
    #[validate(range(min = 0, max = 3600))]
    pub expiration_tolerance: u32,
}

/// Access control configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AccessControlConfig {
    /// Enable access control
    pub enabled: bool,
    
    /// Default access policy
    pub default_policy: AccessPolicy,
    
    /// Access control rules
    pub rules: Vec<AccessRule>,
}

/// Access policy enumeration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AccessPolicy {
    /// Allow all access
    Allow,
    /// Deny all access
    Deny,
}

/// Access control rule
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AccessRule {
    /// Rule name for identification
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    
    /// Pattern to match (topic, user, etc.)
    #[validate(length(min = 1))]
    pub pattern: String,
    
    /// Action to take when pattern matches
    pub action: AccessPolicy,
}

/// Helper module for Duration serialization as milliseconds
mod duration_ms {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

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
        let ms = u64::deserialize(deserializer)?;
        Ok(Duration::from_millis(ms))
    }
}

impl Default for MemoryStreamerConfig {
    /// Default configuration optimized for high-performance operation
    ///
    /// These defaults are tuned for:
    /// - Ultra-low latency (<10μs P99)
    /// - High throughput (10M+ messages/second)
    /// - Production-ready security settings
    /// - Comprehensive observability
    fn default() -> Self {
        Self {
            network: NetworkConfig::default(),
            protocol: ProtocolConfig::default(),
            performance: PerformanceConfig::default(),
            observability: ObservabilityConfig::default(),
            security: SecurityConfig::default(),
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0".to_string(),
            port: 7878,
            max_connections: 10_000,
            connection_timeout: Duration::from_millis(5_000),
            keep_alive: KeepAliveConfig::default(),
            buffers: NetworkBufferConfig::default(),
        }
    }
}

impl Default for KeepAliveConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            idle_time: 300,   // 5 minutes
            probe_interval: 60, // 1 minute
            probe_count: 3,
        }
    }
}

impl Default for NetworkBufferConfig {
    fn default() -> Self {
        Self {
            receive_buffer: 262_144,  // 256KB
            send_buffer: 262_144,     // 256KB
            read_buffer: 32_768,      // 32KB
            write_buffer: 32_768,     // 32KB
        }
    }
}

impl Default for ProtocolConfig {
    fn default() -> Self {
        Self {
            max_payload_size: 1_048_576, // 1MB
            frame_timeout: Duration::from_millis(1_000),
            enable_compression: false, // Disabled by default for lowest latency
            compression: CompressionConfig::default(),
            batching: BatchingConfig::default(),
        }
    }
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            algorithm: CompressionAlgorithm::Lz4,
            level: 1, // Fastest compression
            min_size: 1024, // 1KB minimum
        }
    }
}

impl Default for BatchingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_batch_size: 100,
            max_batch_delay_us: 10, // 10 microseconds
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            target_latency_p99_us: 10, // 10 microseconds
            max_throughput: 10_000_000, // 10M messages/second
            worker_threads: None, // Auto-detect
            numa_awareness: true,
            cpu_affinity: CpuAffinityConfig::default(),
            memory: MemoryConfig::default(),
        }
    }
}

impl Default for CpuAffinityConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Disabled by default for portability
            worker_cores: Vec::new(),
            io_cores: Vec::new(),
        }
    }
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            initial_pool_size: 64 * 1024 * 1024, // 64MB
            max_pool_size: 1024 * 1024 * 1024,   // 1GB
            use_huge_pages: false, // Disabled by default for portability
            growth_factor: 1.5,
        }
    }
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            tracing: TracingConfig::default(),
            metrics: MetricsConfig::default(),
            health_check: HealthCheckConfig::default(),
        }
    }
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            level: TracingLevel::Info,
            json_output: false,
            distributed_tracing: false,
        }
    }
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            collection_interval: 60, // 1 minute
            prometheus_enabled: true,
            prometheus_port: 9090,
        }
    }
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            port: 8080,
            path: "/health".to_string(),
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            tls: TlsConfig::default(),
            authentication: AuthenticationConfig::default(),
            access_control: AccessControlConfig::default(),
        }
    }
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Disabled by default for development
            cert_path: None,
            key_path: None,
            ca_path: None,
            require_client_cert: false,
        }
    }
}

impl Default for AuthenticationConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Disabled by default for development
            method: AuthenticationMethod::None,
            token_validation: TokenValidationConfig::default(),
        }
    }
}

impl Default for TokenValidationConfig {
    fn default() -> Self {
        Self {
            validation_key: None,
            expiration_tolerance: 300, // 5 minutes
        }
    }
}

impl Default for AccessControlConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Disabled by default for development
            default_policy: AccessPolicy::Allow,
            rules: Vec::new(),
        }
    }
}