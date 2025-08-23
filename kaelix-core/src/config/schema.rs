//! # Configuration Schema
//!
//! This module defines the configuration schema for MemoryStreamer with comprehensive
//! validation, serialization support, and hot-reload capabilities.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use validator::Validate;

/// Create an alias for backward compatibility
pub type KaelixConfig = MemoryStreamerConfig;

/// Root configuration structure for MemoryStreamer
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct MemoryStreamerConfig {
    /// Network configuration
    #[validate(nested)]
    pub network: NetworkConfig,

    /// Security configuration
    #[validate(nested)]
    pub security: SecurityConfig,

    /// Telemetry configuration
    #[validate(nested)]
    pub telemetry: TelemetryConfig,

    /// Runtime configuration
    #[validate(nested)]
    pub runtime: RuntimeConfig,
}

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct NetworkConfig {
    /// Server host to bind to
    #[validate(length(min = 1, max = 255))]
    pub host: String,

    /// Server port to bind to
    #[validate(range(min = 1, max = 65535))]
    pub port: u16,

    /// Maximum number of concurrent connections
    #[validate(range(min = 1, max = 1000000))]
    pub max_connections: usize,

    /// Maximum frame size in bytes
    #[validate(range(min = 1024, max = 67108864))] // 1KB to 64MB
    pub max_frame_size: usize,

    /// Read timeout in milliseconds
    #[validate(range(min = 100, max = 300000))] // 100ms to 5 minutes
    pub read_timeout_ms: u64,

    /// Write timeout in milliseconds
    #[validate(range(min = 100, max = 300000))] // 100ms to 5 minutes
    pub write_timeout_ms: u64,

    /// Keep-alive interval in seconds
    #[validate(range(min = 10, max = 3600))] // 10 seconds to 1 hour
    pub keep_alive_seconds: u64,

    /// Protocol-specific configuration
    #[validate(nested)]
    pub protocol: ProtocolConfig,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
            max_connections: 10000,
            max_frame_size: 16777216, // 16MB
            read_timeout_ms: 30000,   // 30 seconds
            write_timeout_ms: 30000,  // 30 seconds
            keep_alive_seconds: 300,  // 5 minutes
            protocol: ProtocolConfig::default(),
        }
    }
}

/// Protocol configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ProtocolConfig {
    /// Protocol version
    #[validate(length(min = 1, max = 10))]
    pub version: String,

    /// Compression settings
    #[validate(nested)]
    pub compression: CompressionSettings,

    /// Batching settings
    #[validate(nested)]
    pub batching: BatchingSettings,

    /// Flow control settings
    #[validate(nested)]
    pub flow_control: FlowControlSettings,
}

impl Default for ProtocolConfig {
    fn default() -> Self {
        Self {
            version: "1.0".to_string(),
            compression: CompressionSettings::default(),
            batching: BatchingSettings::default(),
            flow_control: FlowControlSettings::default(),
        }
    }
}

/// Compression settings
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CompressionSettings {
    /// Whether compression is enabled
    pub enabled: bool,

    /// Compression algorithm
    pub algorithm: CompressionAlgorithm,

    /// Compression level (0-9)
    #[validate(range(min = 0, max = 9))]
    pub level: u8,

    /// Minimum size threshold for compression
    #[validate(range(min = 1, max = 1048576))] // 1 byte to 1MB
    pub threshold_bytes: usize,
}

impl Default for CompressionSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            algorithm: CompressionAlgorithm::Lz4,
            level: 3,
            threshold_bytes: 1024, // 1KB
        }
    }
}

/// Supported compression algorithms
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompressionAlgorithm {
    /// No compression
    None,
    /// LZ4 compression (fast)
    Lz4,
    /// Snappy compression (balanced)
    Snappy,
    /// Zstandard compression (high ratio)
    Zstd,
    /// Gzip compression (widely supported)
    Gzip,
}

impl Default for CompressionAlgorithm {
    fn default() -> Self {
        Self::Lz4
    }
}

/// Batching settings
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct BatchingSettings {
    /// Whether batching is enabled
    pub enabled: bool,

    /// Maximum batch size in number of messages
    #[validate(range(min = 1, max = 10000))]
    pub max_batch_size: u32,

    /// Maximum batch size in bytes
    #[validate(range(min = 1024, max = 16777216))] // 1KB to 16MB
    pub max_batch_bytes: usize,

    /// Maximum time to wait before sending partial batch (ms)
    #[validate(range(min = 1, max = 10000))] // 1ms to 10 seconds
    pub flush_interval_ms: u64,
}

impl Default for BatchingSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            max_batch_size: 100,
            max_batch_bytes: 1048576, // 1MB
            flush_interval_ms: 10,    // 10ms
        }
    }
}

/// Flow control settings
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct FlowControlSettings {
    /// Whether flow control is enabled
    pub enabled: bool,

    /// Window size for flow control
    #[validate(range(min = 1, max = 1048576))] // 1 to 1M messages
    pub window_size: u32,

    /// Low watermark for flow control (percentage)
    #[validate(range(min = 10, max = 90))]
    pub low_watermark_percent: u8,

    /// High watermark for flow control (percentage)
    #[validate(range(min = 10, max = 100))]
    pub high_watermark_percent: u8,
}

impl Default for FlowControlSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            window_size: 10000,
            low_watermark_percent: 25,
            high_watermark_percent: 75,
        }
    }
}

/// Security configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct SecurityConfig {
    /// TLS/SSL configuration
    #[validate(nested)]
    pub tls: TlsConfig,

    /// Authentication configuration
    #[validate(nested)]
    pub authentication: AuthenticationConfig,

    /// Authorization configuration
    #[validate(nested)]
    pub authorization: AuthorizationConfig,

    /// Rate limiting configuration
    #[validate(nested)]
    pub rate_limiting: RateLimitingConfig,

    /// IP filtering configuration
    #[validate(nested)]
    pub ip_filtering: IpFilteringConfig,

    /// Encryption configuration
    #[validate(nested)]
    pub encryption: EncryptionConfig,
}

/// TLS configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TlsConfig {
    /// Whether TLS is enabled
    pub enabled: bool,

    /// Path to TLS certificate file
    #[validate(length(min = 1, max = 512))]
    pub cert_path: String,

    /// Path to TLS private key file
    #[validate(length(min = 1, max = 512))]
    pub key_path: String,

    /// Path to CA certificate file (for client verification)
    #[validate(length(max = 512))]
    pub ca_path: Option<String>,

    /// TLS version to use
    pub version: TlsVersion,

    /// Cipher suites to allow
    pub cipher_suites: Vec<String>,

    /// Whether to require client certificates
    pub require_client_cert: bool,

    /// Session timeout in seconds
    #[validate(range(min = 60, max = 86400))] // 1 minute to 1 day
    pub session_timeout_seconds: u64,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cert_path: String::new(),
            key_path: String::new(),
            ca_path: None,
            version: TlsVersion::V1_3,
            cipher_suites: Vec::new(),
            require_client_cert: false,
            session_timeout_seconds: 3600, // 1 hour
        }
    }
}

/// Supported TLS versions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TlsVersion {
    /// TLS 1.2
    V1_2,
    /// TLS 1.3 (recommended)
    V1_3,
}

impl Default for TlsVersion {
    fn default() -> Self {
        Self::V1_3
    }
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AuthenticationConfig {
    /// Whether authentication is enabled
    pub enabled: bool,

    /// Authentication method to use
    #[validate(length(min = 1, max = 50))]
    pub method: String,

    /// Token-based authentication settings
    #[validate(nested)]
    pub token: Option<TokenAuthConfig>,

    /// JWT authentication settings
    #[validate(nested)]
    pub jwt: Option<JwtAuthConfig>,

    /// OAuth2 authentication settings
    #[validate(nested)]
    pub oauth: Option<OAuthConfig>,

    /// LDAP authentication settings
    #[validate(nested)]
    pub ldap: Option<LdapConfig>,

    /// Session timeout in seconds
    #[validate(range(min = 300, max = 86400))] // 5 minutes to 1 day
    pub session_timeout_seconds: u64,

    /// Maximum concurrent sessions per user
    #[validate(range(min = 1, max = 100))]
    pub max_sessions_per_user: u32,
}

impl Default for AuthenticationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            method: "none".to_string(),
            token: None,
            jwt: None,
            oauth: None,
            ldap: None,
            session_timeout_seconds: 3600, // 1 hour
            max_sessions_per_user: 5,
        }
    }
}

/// Authentication methods
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthenticationMethod {
    /// No authentication
    None,
    /// Token-based authentication
    Token,
    /// JWT authentication
    Jwt,
    /// OAuth2 authentication
    OAuth2,
    /// SAML authentication
    Saml,
    /// OpenID Connect
    OpenIdConnect,
    /// LDAP/Active Directory
    Ldap,
    /// Custom authentication
    Custom {
        /// Authentication method name
        name: String,
        /// Configuration parameters
        config: HashMap<String, String>,
    },
}

/// Token authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TokenAuthConfig {
    /// Secret key for token validation
    #[validate(length(min = 32, max = 512))]
    pub secret_key: String,

    /// Token expiration time in seconds
    #[validate(range(min = 300, max = 86400))] // 5 minutes to 1 day
    pub expiration_seconds: u64,

    /// Token refresh threshold in seconds
    #[validate(range(min = 60, max = 43200))] // 1 minute to 12 hours
    pub refresh_threshold_seconds: u64,
}

impl Default for TokenAuthConfig {
    fn default() -> Self {
        Self {
            secret_key: String::new(),
            expiration_seconds: 3600,       // 1 hour
            refresh_threshold_seconds: 300, // 5 minutes
        }
    }
}

/// JWT authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct JwtAuthConfig {
    /// Secret key or public key for JWT validation
    #[validate(length(min = 32, max = 2048))]
    pub secret_or_key: String,

    /// JWT algorithm to use
    pub algorithm: JwtAlgorithm,

    /// JWT issuer claim
    #[validate(length(min = 1, max = 255))]
    pub issuer: String,

    /// JWT audience claim
    #[validate(length(min = 1, max = 255))]
    pub audience: String,

    /// Token expiration time in seconds
    #[validate(range(min = 300, max = 86400))] // 5 minutes to 1 day
    pub expiration_seconds: u64,

    /// Clock skew tolerance in seconds
    #[validate(range(min = 0, max = 300))] // 0 to 5 minutes
    pub clock_skew_seconds: u64,
}

impl Default for JwtAuthConfig {
    fn default() -> Self {
        Self {
            secret_or_key: String::new(),
            algorithm: JwtAlgorithm::HS256,
            issuer: "kaelix".to_string(),
            audience: "kaelix-users".to_string(),
            expiration_seconds: 3600, // 1 hour
            clock_skew_seconds: 60,   // 1 minute
        }
    }
}

/// JWT algorithms
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JwtAlgorithm {
    /// HMAC using SHA-256
    HS256,
    /// HMAC using SHA-384
    HS384,
    /// HMAC using SHA-512
    HS512,
    /// RSA using SHA-256
    RS256,
    /// RSA using SHA-384
    RS384,
    /// RSA using SHA-512
    RS512,
    /// ECDSA using P-256 and SHA-256
    ES256,
    /// ECDSA using P-384 and SHA-384
    ES384,
    /// ECDSA using P-521 and SHA-512
    ES512,
}

impl Default for JwtAlgorithm {
    fn default() -> Self {
        Self::HS256
    }
}

/// OAuth configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct OAuthConfig {
    /// OAuth provider URL
    #[validate(url)]
    pub provider_url: String,

    /// Client ID
    #[validate(length(min = 1, max = 255))]
    pub client_id: String,

    /// Client secret
    #[validate(length(min = 1, max = 255))]
    pub client_secret: String,

    /// Redirect URI
    #[validate(url)]
    pub redirect_uri: String,

    /// OAuth scopes
    pub scopes: Vec<String>,
}

/// LDAP configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct LdapConfig {
    /// LDAP server URL
    #[validate(url)]
    pub server_url: String,

    /// Bind DN for authentication
    #[validate(length(min = 1, max = 512))]
    pub bind_dn: String,

    /// Bind password
    #[validate(length(min = 1, max = 255))]
    pub bind_password: String,

    /// User search base DN
    #[validate(length(min = 1, max = 512))]
    pub user_base_dn: String,

    /// User search filter
    #[validate(length(min = 1, max = 255))]
    pub user_filter: String,

    /// Group search base DN
    #[validate(length(max = 512))]
    pub group_base_dn: Option<String>,

    /// Connection timeout in seconds
    #[validate(range(min = 1, max = 60))]
    pub timeout_seconds: u64,
}

impl Default for LdapConfig {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            bind_dn: String::new(),
            bind_password: String::new(),
            user_base_dn: String::new(),
            user_filter: "(&(objectClass=person)(uid={}))".to_string(),
            group_base_dn: None,
            timeout_seconds: 10,
        }
    }
}

/// Authorization configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AuthorizationConfig {
    /// Whether authorization is enabled
    pub enabled: bool,

    /// Authorization model
    pub model: AuthorizationModel,

    /// Role-based access control settings
    #[validate(nested)]
    pub rbac: Option<RbacConfig>,

    /// Attribute-based access control settings
    #[validate(nested)]
    pub abac: Option<AbacConfig>,

    /// Access control lists
    pub acls: Vec<AccessControlEntry>,
}

impl Default for AuthorizationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model: AuthorizationModel::None,
            rbac: None,
            abac: None,
            acls: Vec::new(),
        }
    }
}

/// Authorization models
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthorizationModel {
    /// No authorization
    None,
    /// Role-Based Access Control
    RBAC,
    /// Attribute-Based Access Control
    ABAC,
    /// Access Control Lists
    ACL,
}

impl Default for AuthorizationModel {
    fn default() -> Self {
        Self::None
    }
}

/// Role-Based Access Control configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RbacConfig {
    /// Default role for authenticated users
    #[validate(length(min = 1, max = 50))]
    pub default_role: String,

    /// Role definitions
    pub roles: HashMap<String, Role>,

    /// User-role assignments
    pub user_roles: HashMap<String, Vec<String>>,
}

impl Default for RbacConfig {
    fn default() -> Self {
        Self { default_role: "user".to_string(), roles: HashMap::new(), user_roles: HashMap::new() }
    }
}

/// Role definition
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Role {
    /// Role name
    #[validate(length(min = 1, max = 50))]
    pub name: String,

    /// Role description
    #[validate(length(max = 255))]
    pub description: Option<String>,

    /// Permissions granted by this role
    pub permissions: Vec<Permission>,

    /// Whether this role inherits from other roles
    pub inherits_from: Vec<String>,
}

/// Permission definition
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Permission {
    /// Resource this permission applies to
    #[validate(length(min = 1, max = 100))]
    pub resource: String,

    /// Actions allowed on the resource
    pub actions: Vec<String>,

    /// Conditions for permission
    pub conditions: Option<HashMap<String, String>>,
}

/// Attribute-Based Access Control configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct AbacConfig {
    /// Policy definitions
    pub policies: Vec<AbacPolicy>,

    /// Attribute sources
    pub attribute_sources: HashMap<String, AttributeSource>,
}

/// ABAC policy definition
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AbacPolicy {
    /// Policy name
    #[validate(length(min = 1, max = 100))]
    pub name: String,

    /// Policy description
    #[validate(length(max = 255))]
    pub description: Option<String>,

    /// Policy rule expression
    #[validate(length(min = 1, max = 1000))]
    pub rule: String,

    /// Effect (allow/deny)
    pub effect: PolicyEffect,

    /// Priority (higher = more important)
    pub priority: u32,
}

/// Policy effect
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyEffect {
    /// Allow access
    Allow,
    /// Deny access
    Deny,
}

impl Default for PolicyEffect {
    fn default() -> Self {
        Self::Deny
    }
}

/// Attribute source configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AttributeSource {
    /// Source type
    pub source_type: AttributeSourceType,

    /// Source configuration
    pub config: HashMap<String, String>,

    /// Cache TTL in seconds
    #[validate(range(min = 0, max = 86400))] // 0 (no cache) to 1 day
    pub cache_ttl_seconds: u64,
}

/// Attribute source types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttributeSourceType {
    /// Static attributes
    Static,
    /// Database lookup
    Database,
    /// External service
    Service,
    /// LDAP lookup
    Ldap,
}

impl Default for AttributeSourceType {
    fn default() -> Self {
        Self::Static
    }
}

/// Access Control Entry
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AccessControlEntry {
    /// Principal (user, group, or role)
    #[validate(length(min = 1, max = 100))]
    pub principal: String,

    /// Principal type
    pub principal_type: PrincipalType,

    /// Resource pattern
    #[validate(length(min = 1, max = 255))]
    pub resource: String,

    /// Actions allowed
    pub actions: Vec<String>,

    /// Effect (allow/deny)
    pub effect: PolicyEffect,

    /// Conditions
    pub conditions: Option<HashMap<String, String>>,
}

/// Principal types for ACL
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrincipalType {
    /// Individual user
    User,
    /// User group
    Group,
    /// Role
    Role,
    /// Service account
    Service,
}

impl Default for PrincipalType {
    fn default() -> Self {
        Self::User
    }
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RateLimitingConfig {
    /// Whether rate limiting is enabled
    pub enabled: bool,

    /// Requests per second limit
    #[validate(range(min = 1, max = 1000000))]
    pub requests_per_second: u32,

    /// Burst size (maximum requests in burst)
    #[validate(range(min = 1, max = 10000))]
    pub burst_size: u32,

    /// Time window for rate limiting (seconds)
    #[validate(range(min = 1, max = 3600))] // 1 second to 1 hour
    pub window_seconds: u64,

    /// Rate limiting strategy
    pub strategy: RateLimitStrategy,

    /// Per-user rate limits
    pub per_user_limits: HashMap<String, UserRateLimit>,
}

impl Default for RateLimitingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            requests_per_second: 1000,
            burst_size: 100,
            window_seconds: 60,
            strategy: RateLimitStrategy::TokenBucket,
            per_user_limits: HashMap::new(),
        }
    }
}

/// Rate limiting strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RateLimitStrategy {
    /// Token bucket algorithm
    TokenBucket,
    /// Fixed window
    FixedWindow,
    /// Sliding window
    SlidingWindow,
    /// Leaky bucket
    LeakyBucket,
}

impl Default for RateLimitStrategy {
    fn default() -> Self {
        Self::TokenBucket
    }
}

/// Per-user rate limit
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UserRateLimit {
    /// User identifier
    #[validate(length(min = 1, max = 255))]
    pub user_id: String,

    /// Requests per second for this user
    #[validate(range(min = 1, max = 1000000))]
    pub requests_per_second: u32,

    /// Burst size for this user
    #[validate(range(min = 1, max = 10000))]
    pub burst_size: u32,
}

/// IP filtering configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct IpFilteringConfig {
    /// Whether IP filtering is enabled
    pub enabled: bool,

    /// Default policy (allow/deny)
    pub default_policy: IpFilterPolicy,

    /// Allowed IP ranges
    pub allowed_ips: Vec<String>,

    /// Denied IP ranges
    pub denied_ips: Vec<String>,

    /// Trusted proxy IPs
    pub trusted_proxies: Vec<String>,

    /// Whether to use X-Forwarded-For header
    pub use_forwarded_header: bool,
}

impl Default for IpFilteringConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_policy: IpFilterPolicy::Allow,
            allowed_ips: Vec::new(),
            denied_ips: Vec::new(),
            trusted_proxies: Vec::new(),
            use_forwarded_header: false,
        }
    }
}

/// IP filter policies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IpFilterPolicy {
    /// Allow by default
    Allow,
    /// Deny by default
    Deny,
}

impl Default for IpFilterPolicy {
    fn default() -> Self {
        Self::Allow
    }
}

/// Encryption configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct EncryptionConfig {
    /// Whether encryption is enabled
    pub enabled: bool,

    /// Encryption algorithm
    pub algorithm: EncryptionAlgorithm,

    /// Key derivation function
    pub key_derivation: KeyDerivationFunction,

    /// Key size in bits
    #[validate(range(min = 128, max = 512))]
    pub key_size_bits: u16,

    /// Key rotation interval in hours
    #[validate(range(min = 1, max = 8760))] // 1 hour to 1 year
    pub key_rotation_hours: u64,

    /// Encryption key management
    #[validate(nested)]
    pub key_management: KeyManagementConfig,
}

impl Default for EncryptionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            algorithm: EncryptionAlgorithm::Aes256Gcm,
            key_derivation: KeyDerivationFunction::PBKDF2,
            key_size_bits: 256,
            key_rotation_hours: 168, // 1 week
            key_management: KeyManagementConfig::default(),
        }
    }
}

/// Encryption algorithms
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncryptionAlgorithm {
    /// AES-128 in GCM mode
    Aes128Gcm,
    /// AES-256 in GCM mode
    Aes256Gcm,
    /// ChaCha20-Poly1305
    ChaCha20Poly1305,
    /// XChaCha20-Poly1305
    XChaCha20Poly1305,
}

impl Default for EncryptionAlgorithm {
    fn default() -> Self {
        Self::Aes256Gcm
    }
}

/// Key derivation functions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyDerivationFunction {
    /// PBKDF2 with SHA-256
    PBKDF2,
    /// scrypt
    Scrypt,
    /// Argon2
    Argon2,
}

impl Default for KeyDerivationFunction {
    fn default() -> Self {
        Self::PBKDF2
    }
}

/// Key management configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct KeyManagementConfig {
    /// Key storage type
    pub storage_type: KeyStorageType,

    /// Key storage configuration
    pub storage_config: HashMap<String, String>,

    /// Whether to use hardware security module
    pub use_hsm: bool,

    /// HSM configuration
    #[validate(nested)]
    pub hsm_config: Option<HsmConfig>,
}

impl Default for KeyManagementConfig {
    fn default() -> Self {
        Self {
            storage_type: KeyStorageType::File,
            storage_config: HashMap::new(),
            use_hsm: false,
            hsm_config: None,
        }
    }
}

/// Key storage types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyStorageType {
    /// File-based storage
    File,
    /// Environment variables
    Environment,
    /// External key management service
    External,
    /// Hardware security module
    HSM,
}

impl Default for KeyStorageType {
    fn default() -> Self {
        Self::File
    }
}

/// Hardware Security Module configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct HsmConfig {
    /// HSM provider
    #[validate(length(min = 1, max = 50))]
    pub provider: String,

    /// HSM slot ID
    pub slot_id: u32,

    /// HSM PIN or password
    #[validate(length(min = 4, max = 255))]
    pub pin: String,

    /// Key label prefix
    #[validate(length(min = 1, max = 50))]
    pub key_label_prefix: String,
}

impl Default for HsmConfig {
    fn default() -> Self {
        Self {
            provider: String::new(),
            slot_id: 0,
            pin: String::new(),
            key_label_prefix: "kaelix".to_string(),
        }
    }
}

/// Telemetry configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct TelemetryConfig {
    /// Logging configuration
    #[validate(nested)]
    pub logging: LoggingConfig,

    /// Metrics configuration
    #[validate(nested)]
    pub metrics: MetricsConfig,

    /// Distributed tracing configuration
    #[validate(nested)]
    pub tracing: TracingConfig,

    /// Performance monitoring configuration
    #[validate(nested)]
    pub performance: PerformanceMonitoringConfig,

    /// Health check configuration
    #[validate(nested)]
    pub health: HealthCheckConfig,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct LoggingConfig {
    /// Whether logging is enabled
    pub enabled: bool,

    /// Log level
    pub level: TracingLevel,

    /// Log format
    pub format: LogFormat,

    /// Log output targets
    pub targets: Vec<LogTarget>,

    /// Maximum log file size in MB
    #[validate(range(min = 1, max = 1024))] // 1MB to 1GB
    pub max_file_size_mb: u64,

    /// Maximum number of log files to keep
    #[validate(range(min = 1, max = 100))]
    pub max_files: u32,

    /// Log rotation strategy
    pub rotation: LogRotation,

    /// Whether to compress old log files
    pub compress_rotated: bool,

    /// Log filtering configuration
    #[validate(nested)]
    pub filtering: LogFilteringConfig,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            level: TracingLevel::Info,
            format: LogFormat::Json,
            targets: vec![LogTarget::Console, LogTarget::File],
            max_file_size_mb: 100,
            max_files: 10,
            rotation: LogRotation::Daily,
            compress_rotated: true,
            filtering: LogFilteringConfig::default(),
        }
    }
}

/// Tracing/logging levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TracingLevel {
    /// Trace level
    Trace,
    /// Debug level
    Debug,
    /// Info level
    Info,
    /// Warn level
    Warn,
    /// Error level
    Error,
}

impl Default for TracingLevel {
    fn default() -> Self {
        Self::Info
    }
}

impl std::fmt::Display for TracingLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Trace => write!(f, "trace"),
            Self::Debug => write!(f, "debug"),
            Self::Info => write!(f, "info"),
            Self::Warn => write!(f, "warn"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// Log output formats
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogFormat {
    /// Plain text format
    Text,
    /// JSON format
    Json,
    /// Compact format
    Compact,
    /// Pretty format (for development)
    Pretty,
}

impl Default for LogFormat {
    fn default() -> Self {
        Self::Json
    }
}

/// Log output targets
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogTarget {
    /// Console output
    Console,
    /// File output
    File,
    /// Syslog
    Syslog,
    /// Custom target
    Custom {
        /// Target name
        name: String,
        /// Target configuration
        config: HashMap<String, String>,
    },
}

/// Log rotation strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogRotation {
    /// No rotation
    None,
    /// Rotate daily
    Daily,
    /// Rotate hourly
    Hourly,
    /// Rotate when size limit reached
    Size,
    /// Rotate weekly
    Weekly,
    /// Rotate monthly
    Monthly,
}

impl Default for LogRotation {
    fn default() -> Self {
        Self::Daily
    }
}

/// Log filtering configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct LogFilteringConfig {
    /// Module-specific log levels
    pub module_levels: HashMap<String, TracingLevel>,

    /// Whether to enable sampling
    pub sampling_enabled: bool,

    /// Sampling rate (0.0 to 1.0)
    #[validate(range(min = 0.0, max = 1.0))]
    pub sampling_rate: f64,

    /// Rate limiting for log messages
    #[validate(nested)]
    pub rate_limiting: LogRateLimitConfig,
}

impl Default for LogFilteringConfig {
    fn default() -> Self {
        Self {
            module_levels: HashMap::new(),
            sampling_enabled: false,
            sampling_rate: 1.0,
            rate_limiting: LogRateLimitConfig::default(),
        }
    }
}

/// Log rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct LogRateLimitConfig {
    /// Whether rate limiting is enabled
    pub enabled: bool,

    /// Maximum log messages per second
    #[validate(range(min = 1, max = 10000))]
    pub max_messages_per_second: u32,

    /// Burst size
    #[validate(range(min = 1, max = 1000))]
    pub burst_size: u32,
}

impl Default for LogRateLimitConfig {
    fn default() -> Self {
        Self { enabled: false, max_messages_per_second: 100, burst_size: 10 }
    }
}

/// Metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MetricsConfig {
    /// Whether metrics collection is enabled
    pub enabled: bool,

    /// Metrics collection interval in seconds
    #[validate(range(min = 1, max = 3600))] // 1 second to 1 hour
    pub collection_interval_seconds: u64,

    /// Prometheus metrics configuration
    #[validate(nested)]
    pub prometheus: PrometheusConfig,

    /// StatsD metrics configuration
    #[validate(nested)]
    pub statsd: Option<StatsDConfig>,

    /// Custom metrics exporters
    pub custom_exporters: Vec<MetricsExporter>,

    /// Metrics retention configuration
    #[validate(nested)]
    pub retention: MetricsRetentionConfig,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            collection_interval_seconds: 15,
            prometheus: PrometheusConfig::default(),
            statsd: None,
            custom_exporters: Vec::new(),
            retention: MetricsRetentionConfig::default(),
        }
    }
}

/// Prometheus metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PrometheusConfig {
    /// Whether Prometheus metrics are enabled
    pub enabled: bool,

    /// Port for Prometheus metrics endpoint
    #[validate(range(min = 1, max = 65535))]
    pub port: u16,

    /// Path for metrics endpoint
    #[validate(length(min = 1, max = 100))]
    pub path: String,

    /// Metric labels to include
    pub labels: HashMap<String, String>,

    /// Metric prefixes
    #[validate(length(max = 50))]
    pub namespace: Option<String>,
}

impl Default for PrometheusConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            port: 9090,
            path: "/metrics".to_string(),
            labels: HashMap::new(),
            namespace: Some("kaelix".to_string()),
        }
    }
}

/// StatsD configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct StatsDConfig {
    /// StatsD server host
    #[validate(length(min = 1, max = 255))]
    pub host: String,

    /// StatsD server port
    #[validate(range(min = 1, max = 65535))]
    pub port: u16,

    /// Metric prefix
    #[validate(length(max = 50))]
    pub prefix: Option<String>,

    /// Sample rate
    #[validate(range(min = 0.0, max = 1.0))]
    pub sample_rate: f64,
}

impl Default for StatsDConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 8125,
            prefix: Some("kaelix".to_string()),
            sample_rate: 1.0,
        }
    }
}

/// Custom metrics exporter
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MetricsExporter {
    /// Exporter name
    #[validate(length(min = 1, max = 50))]
    pub name: String,

    /// Exporter type
    pub exporter_type: MetricsExporterType,

    /// Exporter configuration
    pub config: HashMap<String, String>,

    /// Export interval in seconds
    #[validate(range(min = 1, max = 3600))]
    pub interval_seconds: u64,
}

/// Metrics exporter types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetricsExporterType {
    /// InfluxDB exporter
    InfluxDB,
    /// Graphite exporter
    Graphite,
    /// Custom exporter
    Custom,
}

/// Metrics retention configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MetricsRetentionConfig {
    /// Default retention period in hours
    #[validate(range(min = 1, max = 8760))] // 1 hour to 1 year
    pub default_retention_hours: u64,

    /// Per-metric retention settings
    pub per_metric_retention: HashMap<String, u64>,

    /// Aggregation settings
    #[validate(nested)]
    pub aggregation: MetricsAggregationConfig,
}

impl Default for MetricsRetentionConfig {
    fn default() -> Self {
        Self {
            default_retention_hours: 168, // 1 week
            per_metric_retention: HashMap::new(),
            aggregation: MetricsAggregationConfig::default(),
        }
    }
}

/// Metrics aggregation configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MetricsAggregationConfig {
    /// Whether aggregation is enabled
    pub enabled: bool,

    /// Aggregation intervals
    pub intervals: Vec<AggregationInterval>,

    /// Aggregation functions
    pub functions: Vec<AggregationFunction>,
}

impl Default for MetricsAggregationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            intervals: vec![
                AggregationInterval::Minute,
                AggregationInterval::Hour,
                AggregationInterval::Day,
            ],
            functions: vec![
                AggregationFunction::Average,
                AggregationFunction::Sum,
                AggregationFunction::Max,
                AggregationFunction::Min,
            ],
        }
    }
}

/// Aggregation intervals
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AggregationInterval {
    /// Per minute
    Minute,
    /// Per hour
    Hour,
    /// Per day
    Day,
    /// Per week
    Week,
}

/// Aggregation functions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AggregationFunction {
    /// Average value
    Average,
    /// Sum of values
    Sum,
    /// Maximum value
    Max,
    /// Minimum value
    Min,
    /// Count of values
    Count,
    /// 95th percentile
    P95,
    /// 99th percentile
    P99,
}

/// Distributed tracing configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TracingConfig {
    /// Whether distributed tracing is enabled
    pub enabled: bool,

    /// Tracing service name
    #[validate(length(min = 1, max = 100))]
    pub service_name: String,

    /// Tracing service version
    #[validate(length(min = 1, max = 50))]
    pub service_version: String,

    /// Sampling configuration
    #[validate(nested)]
    pub sampling: TracingSamplingConfig,

    /// Jaeger configuration
    #[validate(nested)]
    pub jaeger: JaegerConfig,

    /// Zipkin configuration
    #[validate(nested)]
    pub zipkin: Option<ZipkinConfig>,

    /// OpenTelemetry configuration
    #[validate(nested)]
    pub opentelemetry: OpenTelemetryConfig,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            service_name: "kaelix".to_string(),
            service_version: "1.0.0".to_string(),
            sampling: TracingSamplingConfig::default(),
            jaeger: JaegerConfig::default(),
            zipkin: None,
            opentelemetry: OpenTelemetryConfig::default(),
        }
    }
}

/// Tracing sampling configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TracingSamplingConfig {
    /// Sampling strategy
    pub strategy: SamplingStrategy,

    /// Sampling rate (0.0 to 1.0)
    #[validate(range(min = 0.0, max = 1.0))]
    pub rate: f64,

    /// Maximum traces per second
    #[validate(range(min = 1, max = 10000))]
    pub max_traces_per_second: u32,
}

impl Default for TracingSamplingConfig {
    fn default() -> Self {
        Self {
            strategy: SamplingStrategy::Probabilistic,
            rate: 0.1, // 10%
            max_traces_per_second: 100,
        }
    }
}

/// Sampling strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SamplingStrategy {
    /// Always sample
    Always,
    /// Never sample
    Never,
    /// Probabilistic sampling
    Probabilistic,
    /// Rate limiting sampling
    RateLimiting,
    /// Adaptive sampling
    Adaptive,
}

impl Default for SamplingStrategy {
    fn default() -> Self {
        Self::Probabilistic
    }
}

/// Jaeger configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct JaegerConfig {
    /// Whether Jaeger is enabled
    pub enabled: bool,

    /// Jaeger agent endpoint
    #[validate(length(min = 1, max = 255))]
    pub endpoint: String,

    /// Authentication token
    #[validate(length(max = 255))]
    pub auth_token: Option<String>,

    /// Username for basic auth
    #[validate(length(max = 100))]
    pub username: Option<String>,

    /// Password for basic auth
    #[validate(length(max = 100))]
    pub password: Option<String>,
}

impl Default for JaegerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: "http://localhost:14268/api/traces".to_string(),
            auth_token: None,
            username: None,
            password: None,
        }
    }
}

/// Zipkin configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ZipkinConfig {
    /// Zipkin endpoint
    #[validate(url)]
    pub endpoint: String,

    /// Timeout for sending spans
    #[validate(range(min = 1, max = 60))]
    pub timeout_seconds: u64,
}

impl Default for ZipkinConfig {
    fn default() -> Self {
        Self { endpoint: "http://localhost:9411/api/v2/spans".to_string(), timeout_seconds: 10 }
    }
}

/// OpenTelemetry configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct OpenTelemetryConfig {
    /// Whether OpenTelemetry is enabled
    pub enabled: bool,

    /// OTLP endpoint
    #[validate(length(min = 1, max = 255))]
    pub endpoint: String,

    /// Protocol to use
    pub protocol: OtlpProtocol,

    /// Headers to include
    pub headers: HashMap<String, String>,

    /// Timeout in seconds
    #[validate(range(min = 1, max = 60))]
    pub timeout_seconds: u64,
}

impl Default for OpenTelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: "http://localhost:4317".to_string(),
            protocol: OtlpProtocol::Grpc,
            headers: HashMap::new(),
            timeout_seconds: 10,
        }
    }
}

/// OTLP protocols
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OtlpProtocol {
    /// gRPC protocol
    Grpc,
    /// HTTP protocol
    Http,
}

impl Default for OtlpProtocol {
    fn default() -> Self {
        Self::Grpc
    }
}

/// Performance monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PerformanceMonitoringConfig {
    /// Whether performance monitoring is enabled
    pub enabled: bool,

    /// Number of worker threads (None = auto-detect)
    #[validate(range(min = 1, max = 64))]
    pub worker_threads: Option<usize>,

    /// Buffer size for performance data
    #[validate(range(min = 1024, max = 16777216))] // 1KB to 16MB
    pub buffer_size: usize,

    /// Batch size for processing
    #[validate(range(min = 1, max = 1000))]
    pub batch_size: u32,

    /// Collection interval in milliseconds
    #[validate(range(min = 100, max = 60000))] // 100ms to 1 minute
    pub collection_interval_ms: u64,

    /// CPU profiling settings
    #[validate(nested)]
    pub cpu_profiling: CpuProfilingConfig,

    /// Memory profiling settings
    #[validate(nested)]
    pub memory_profiling: MemoryProfilingConfig,

    /// I/O profiling settings
    #[validate(nested)]
    pub io_profiling: IoProfilingConfig,
}

impl Default for PerformanceMonitoringConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            worker_threads: None, // Auto-detect based on CPU cores
            buffer_size: 1048576, // 1MB
            batch_size: 100,
            collection_interval_ms: 1000, // 1 second
            cpu_profiling: CpuProfilingConfig::default(),
            memory_profiling: MemoryProfilingConfig::default(),
            io_profiling: IoProfilingConfig::default(),
        }
    }
}

/// CPU profiling configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CpuProfilingConfig {
    /// Whether CPU profiling is enabled
    pub enabled: bool,

    /// Profiling frequency in Hz
    #[validate(range(min = 1, max = 1000))]
    pub frequency_hz: u32,

    /// Stack trace depth
    #[validate(range(min = 1, max = 100))]
    pub stack_depth: u32,
}

impl Default for CpuProfilingConfig {
    fn default() -> Self {
        Self { enabled: false, frequency_hz: 100, stack_depth: 64 }
    }
}

/// Memory profiling configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MemoryProfilingConfig {
    /// Whether memory profiling is enabled
    pub enabled: bool,

    /// Allocation tracking threshold in bytes
    #[validate(range(min = 1, max = 1048576))]
    pub allocation_threshold_bytes: usize,

    /// Stack trace for allocations
    pub track_stack_traces: bool,
}

impl Default for MemoryProfilingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            allocation_threshold_bytes: 1024, // 1KB
            track_stack_traces: false,
        }
    }
}

/// I/O profiling configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct IoProfilingConfig {
    /// Whether I/O profiling is enabled
    pub enabled: bool,

    /// Track file operations
    pub track_file_operations: bool,

    /// Track network operations
    pub track_network_operations: bool,

    /// Minimum operation duration to track (microseconds)
    #[validate(range(min = 1, max = 1000000))]
    pub min_duration_micros: u64,
}

impl Default for IoProfilingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            track_file_operations: true,
            track_network_operations: true,
            min_duration_micros: 100, // 100 microseconds
        }
    }
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct HealthCheckConfig {
    /// Whether health checks are enabled
    pub enabled: bool,

    /// Health check port
    #[validate(range(min = 1, max = 65535))]
    pub port: u16,

    /// Health check path
    #[validate(length(min = 1, max = 100))]
    pub path: String,

    /// Check interval in seconds
    #[validate(range(min = 1, max = 3600))]
    pub interval_seconds: u64,

    /// Timeout for individual checks
    #[validate(range(min = 1, max = 60))]
    pub timeout_seconds: u64,

    /// Health check probes
    pub probes: Vec<HealthProbe>,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            port: 8081,
            path: "/health".to_string(),
            interval_seconds: 30,
            timeout_seconds: 5,
            probes: vec![HealthProbe::Liveness, HealthProbe::Readiness],
        }
    }
}

/// Health check probes
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthProbe {
    /// Liveness probe
    Liveness,
    /// Readiness probe
    Readiness,
    /// Startup probe
    Startup,
    /// Custom probe
    Custom {
        /// Probe name
        name: String,
        /// Probe configuration
        config: HashMap<String, String>,
    },
}

/// Runtime configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct RuntimeConfig {
    /// Storage configuration
    #[validate(nested)]
    pub storage: StorageConfig,

    /// Memory pool configuration
    #[validate(nested)]
    pub memory_pool: MemoryPoolConfig,

    /// Thread pool configuration
    #[validate(nested)]
    pub thread_pool: ThreadPoolConfig,

    /// Resource limits
    #[validate(nested)]
    pub resource_limits: ResourceLimitsConfig,

    /// Garbage collection settings
    #[validate(nested)]
    pub gc: GcConfig,

    /// Maintenance settings
    #[validate(nested)]
    pub maintenance: MaintenanceConfig,
}

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct StorageConfig {
    /// Whether persistent storage is enabled
    pub enabled: bool,

    /// Storage engine to use
    pub engine: StorageEngine,

    /// Data directory path
    #[validate(length(min = 1, max = 512))]
    pub data_dir: String,

    /// Write-ahead log settings
    #[validate(nested)]
    pub wal: WalConfig,

    /// Compression settings
    #[validate(nested)]
    pub compression: StorageCompressionConfig,

    /// Backup settings
    #[validate(nested)]
    pub backup: BackupConfig,

    /// Data retention settings
    #[validate(range(min = 1, max = 8760))] // 1 hour to 1 year
    pub retention_hours: u64,

    /// Cleanup settings
    #[validate(nested)]
    pub cleanup: StorageCleanupConfig,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            engine: StorageEngine::RocksDB,
            data_dir: "./data".to_string(),
            wal: WalConfig::default(),
            compression: StorageCompressionConfig::default(),
            backup: BackupConfig::default(),
            retention_hours: 168, // 1 week
            cleanup: StorageCleanupConfig::default(),
        }
    }
}

/// Storage engines
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StorageEngine {
    /// In-memory storage
    Memory,
    /// RocksDB (LSM-tree)
    RocksDB,
    /// LMDB (B-tree)
    LMDB,
    /// Custom storage engine
    Custom {
        /// Engine name
        name: String,
        /// Engine configuration
        config: HashMap<String, String>,
    },
}

impl Default for StorageEngine {
    fn default() -> Self {
        Self::RocksDB
    }
}

/// Write-ahead log configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct WalConfig {
    /// Whether WAL is enabled
    pub enabled: bool,

    /// WAL directory (separate from data directory for performance)
    #[validate(length(min = 1, max = 512))]
    pub wal_dir: String,

    /// WAL file size limit in MB
    #[validate(range(min = 1, max = 1024))]
    pub file_size_mb: u64,

    /// Maximum number of WAL files
    #[validate(range(min = 1, max = 100))]
    pub max_files: u32,

    /// Sync mode
    pub sync_mode: WalSyncMode,

    /// Compression for WAL files
    pub compress: bool,
}

impl Default for WalConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            wal_dir: "./wal".to_string(),
            file_size_mb: 64,
            max_files: 10,
            sync_mode: WalSyncMode::Periodic,
            compress: false, // WAL writes are latency-sensitive
        }
    }
}

/// WAL synchronization modes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WalSyncMode {
    /// No synchronization (fastest, least safe)
    None,
    /// Periodic synchronization
    Periodic,
    /// Synchronize on every write (slowest, safest)
    Always,
}

impl Default for WalSyncMode {
    fn default() -> Self {
        Self::Periodic
    }
}

/// Storage compression configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct StorageCompressionConfig {
    /// Whether compression is enabled
    pub enabled: bool,

    /// Compression algorithm
    pub algorithm: CompressionAlgorithm,

    /// Compression level
    #[validate(range(min = 0, max = 9))]
    pub level: u8,

    /// Block size for compression in KB
    #[validate(range(min = 4, max = 1024))]
    pub block_size_kb: u32,
}

impl Default for StorageCompressionConfig {
    fn default() -> Self {
        Self { enabled: true, algorithm: CompressionAlgorithm::Lz4, level: 3, block_size_kb: 64 }
    }
}

/// Backup configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct BackupConfig {
    /// Whether automatic backups are enabled
    pub enabled: bool,

    /// Backup schedule (cron expression)
    #[validate(length(min = 9, max = 100))]
    pub schedule: String,

    /// Backup retention policy
    #[validate(nested)]
    pub retention: BackupRetentionPolicy,

    /// Backup destinations
    pub destinations: Vec<BackupDestination>,

    /// Compression for backups
    #[validate(nested)]
    pub compression: BackupCompressionConfig,

    /// Encryption for backups
    #[validate(nested)]
    pub encryption: BackupEncryptionConfig,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            schedule: "0 2 * * *".to_string(), // Daily at 2 AM
            retention: BackupRetentionPolicy::default(),
            destinations: vec![BackupDestination::Local { path: "./backups".to_string() }],
            compression: BackupCompressionConfig::default(),
            encryption: BackupEncryptionConfig::default(),
        }
    }
}

/// Backup retention policy
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct BackupRetentionPolicy {
    /// Number of daily backups to keep
    #[validate(range(min = 0, max = 365))]
    pub daily_backups: u32,

    /// Number of weekly backups to keep
    #[validate(range(min = 0, max = 52))]
    pub weekly_backups: u32,

    /// Number of monthly backups to keep
    #[validate(range(min = 0, max = 12))]
    pub monthly_backups: u32,

    /// Number of yearly backups to keep
    #[validate(range(min = 0, max = 10))]
    pub yearly_backups: u32,
}

impl Default for BackupRetentionPolicy {
    fn default() -> Self {
        Self { daily_backups: 7, weekly_backups: 4, monthly_backups: 6, yearly_backups: 2 }
    }
}

/// Backup destinations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackupDestination {
    /// Local filesystem
    Local {
        /// Local path
        path: String,
    },
    /// Amazon S3
    S3 {
        /// S3 bucket name
        bucket: String,
        /// AWS region
        region: String,
        /// Access key
        access_key: String,
        /// Secret key
        secret_key: String,
    },
    /// Google Cloud Storage
    Gcs {
        /// GCS bucket name
        bucket: String,
        /// Credentials file path
        credentials_path: String,
    },
    /// Azure Blob Storage
    Azure {
        /// Storage account name
        account_name: String,
        /// Account key
        account_key: String,
        /// Container name
        container: String,
    },
}

/// Backup compression configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct BackupCompressionConfig {
    /// Whether backup compression is enabled
    pub enabled: bool,

    /// Compression algorithm
    pub algorithm: CompressionAlgorithm,

    /// Compression level
    #[validate(range(min = 0, max = 9))]
    pub level: u8,
}

impl Default for BackupCompressionConfig {
    fn default() -> Self {
        Self { enabled: true, algorithm: CompressionAlgorithm::Zstd, level: 3 }
    }
}

/// Backup encryption configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct BackupEncryptionConfig {
    /// Whether backup encryption is enabled
    pub enabled: bool,

    /// Encryption key (or path to key file)
    #[validate(length(min = 32, max = 512))]
    pub key: String,

    /// Encryption algorithm
    pub algorithm: EncryptionAlgorithm,
}

impl Default for BackupEncryptionConfig {
    fn default() -> Self {
        Self { enabled: false, key: String::new(), algorithm: EncryptionAlgorithm::Aes256Gcm }
    }
}

/// Storage cleanup configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct StorageCleanupConfig {
    /// Whether automatic cleanup is enabled
    pub enabled: bool,

    /// Cleanup interval in hours
    #[validate(range(min = 1, max = 168))] // 1 hour to 1 week
    pub interval_hours: u64,

    /// Cleanup strategies
    pub strategies: Vec<CleanupStrategy>,
}

impl Default for StorageCleanupConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_hours: 24, // Daily
            strategies: vec![CleanupStrategy::ExpiredData, CleanupStrategy::CompactLogs],
        }
    }
}

/// Cleanup strategies
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CleanupStrategy {
    /// Remove expired data
    ExpiredData,
    /// Compact log files
    CompactLogs,
    /// Remove temporary files
    TempFiles,
    /// Vacuum storage
    Vacuum,
}

/// Memory pool configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MemoryPoolConfig {
    /// Whether memory pooling is enabled
    pub enabled: bool,

    /// Initial pool size in bytes
    #[validate(range(min = 1048576, max = 1073741824))] // 1MB to 1GB
    pub size: usize,

    /// Pool growth factor
    #[validate(range(min = 1.1, max = 3.0))]
    pub growth_factor: f64,

    /// Maximum pool size in bytes
    #[validate(range(min = 1048576, max = 1073741824))] // 1MB to 1GB
    pub max_size: usize,

    /// Pool allocation strategy
    pub allocation_strategy: AllocationStrategy,

    /// Memory alignment in bytes
    #[validate(range(min = 8, max = 4096))]
    pub alignment: usize,
}

impl Default for MemoryPoolConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            size: 67108864, // 64MB
            growth_factor: 1.5,
            max_size: 1073741824, // 1GB
            allocation_strategy: AllocationStrategy::FirstFit,
            alignment: 64, // Cache line aligned
        }
    }
}

/// Memory allocation strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AllocationStrategy {
    /// First fit algorithm
    FirstFit,
    /// Best fit algorithm
    BestFit,
    /// Worst fit algorithm
    WorstFit,
    /// Buddy allocation
    Buddy,
}

impl Default for AllocationStrategy {
    fn default() -> Self {
        Self::FirstFit
    }
}

/// Thread pool configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ThreadPoolConfig {
    /// Core number of threads
    #[validate(range(min = 1, max = 128))]
    pub core_threads: usize,

    /// Maximum number of threads
    #[validate(range(min = 1, max = 1024))]
    pub max_threads: usize,

    /// Thread keep-alive time in seconds
    #[validate(range(min = 1, max = 3600))]
    pub keep_alive_seconds: u64,

    /// Queue size for pending tasks
    #[validate(range(min = 1, max = 1000000))]
    pub queue_size: usize,

    /// Thread priority
    pub thread_priority: ThreadPriority,

    /// Thread naming pattern
    #[validate(length(min = 1, max = 50))]
    pub thread_name_pattern: String,
}

impl Default for ThreadPoolConfig {
    fn default() -> Self {
        Self {
            core_threads: num_cpus::get(),
            max_threads: num_cpus::get() * 4,
            keep_alive_seconds: 60,
            queue_size: 10000,
            thread_priority: ThreadPriority::Normal,
            thread_name_pattern: "kaelix-worker-{}".to_string(),
        }
    }
}

/// Thread priorities
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThreadPriority {
    /// Low priority
    Low,
    /// Normal priority
    Normal,
    /// High priority
    High,
}

impl Default for ThreadPriority {
    fn default() -> Self {
        Self::Normal
    }
}

/// Resource limits configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ResourceLimitsConfig {
    /// Maximum memory usage in bytes (0 = unlimited)
    pub max_memory_bytes: u64,

    /// Maximum CPU usage as percentage (0-100, 0 = unlimited)
    #[validate(range(min = 0, max = 100))]
    pub max_cpu_percent: u8,

    /// Maximum file descriptors
    #[validate(range(min = 100, max = 1000000))]
    pub max_file_descriptors: u32,

    /// Maximum network connections
    #[validate(range(min = 1, max = 1000000))]
    pub max_network_connections: u32,

    /// Memory pressure thresholds
    #[validate(nested)]
    pub memory_pressure: MemoryPressureConfig,
}

impl Default for ResourceLimitsConfig {
    fn default() -> Self {
        Self {
            max_memory_bytes: 0, // Unlimited
            max_cpu_percent: 0,  // Unlimited
            max_file_descriptors: 10000,
            max_network_connections: 10000,
            memory_pressure: MemoryPressureConfig::default(),
        }
    }
}

/// Memory pressure configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MemoryPressureConfig {
    /// Low pressure threshold (percentage)
    #[validate(range(min = 50, max = 80))]
    pub low_threshold_percent: u8,

    /// High pressure threshold (percentage)
    #[validate(range(min = 80, max = 95))]
    pub high_threshold_percent: u8,

    /// Critical pressure threshold (percentage)
    #[validate(range(min = 90, max = 99))]
    pub critical_threshold_percent: u8,

    /// Actions to take under memory pressure
    pub pressure_actions: Vec<PressureAction>,
}

impl Default for MemoryPressureConfig {
    fn default() -> Self {
        Self {
            low_threshold_percent: 70,
            high_threshold_percent: 85,
            critical_threshold_percent: 95,
            pressure_actions: vec![
                PressureAction::GarbageCollect,
                PressureAction::FlushCaches,
                PressureAction::CompactMemory,
            ],
        }
    }
}

/// Actions to take under memory pressure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PressureAction {
    /// Trigger garbage collection
    GarbageCollect,
    /// Flush memory caches
    FlushCaches,
    /// Compact memory pools
    CompactMemory,
    /// Reduce buffer sizes
    ReduceBuffers,
    /// Drop non-essential data
    DropNonEssential,
}

/// Garbage collection configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct GcConfig {
    /// Whether GC is enabled
    pub enabled: bool,

    /// GC strategy
    pub strategy: GcStrategy,

    /// GC interval in seconds
    #[validate(range(min = 1, max = 3600))]
    pub interval_seconds: u64,

    /// Memory threshold for triggering GC (percentage)
    #[validate(range(min = 50, max = 95))]
    pub memory_threshold_percent: u8,

    /// GC pause target in milliseconds
    #[validate(range(min = 1, max = 1000))]
    pub pause_target_ms: u64,
}

impl Default for GcConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            strategy: GcStrategy::Generational,
            interval_seconds: 60,
            memory_threshold_percent: 80,
            pause_target_ms: 10,
        }
    }
}

/// Garbage collection strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GcStrategy {
    /// No garbage collection
    None,
    /// Simple mark-and-sweep
    MarkSweep,
    /// Generational GC
    Generational,
    /// Incremental GC
    Incremental,
    /// Concurrent GC
    Concurrent,
}

impl Default for GcStrategy {
    fn default() -> Self {
        Self::Generational
    }
}

/// Maintenance configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
pub struct MaintenanceConfig {
    /// Whether maintenance is enabled
    pub enabled: bool,

    /// Maintenance window configuration
    #[validate(nested)]
    pub window: MaintenanceWindow,

    /// Automatic cleanup settings
    #[validate(nested)]
    pub cleanup: CleanupConfig,

    /// Compaction settings
    #[validate(nested)]
    pub compaction: CompactionConfig,
}

/// Maintenance window configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MaintenanceWindow {
    /// Maintenance schedule (cron expression)
    pub schedule: String,

    /// Maximum duration of maintenance in minutes
    #[validate(range(min = 1, max = 480))] // 1 minute to 8 hours
    pub max_duration_minutes: u64,

    /// Whether to allow maintenance during high load
    pub allow_during_high_load: bool,

    /// Load threshold for preventing maintenance
    #[validate(range(min = 0.0, max = 1.0))]
    pub load_threshold: f64,
}

impl Default for MaintenanceWindow {
    fn default() -> Self {
        Self {
            schedule: "0 3 * * *".to_string(), // Daily at 3 AM
            max_duration_minutes: 30,
            allow_during_high_load: false,
            load_threshold: 0.7, // 70%
        }
    }
}

/// Cleanup configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CleanupConfig {
    /// Whether automatic cleanup is enabled
    pub enabled: bool,

    /// Types of cleanup to perform
    pub cleanup_types: Vec<CleanupType>,

    /// Cleanup thresholds
    #[validate(nested)]
    pub thresholds: CleanupThresholds,
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cleanup_types: vec![
                CleanupType::ExpiredMessages,
                CleanupType::OldMetrics,
                CleanupType::TempFiles,
            ],
            thresholds: CleanupThresholds::default(),
        }
    }
}

/// Types of cleanup operations
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CleanupType {
    /// Clean up expired messages
    ExpiredMessages,
    /// Clean up old metrics
    OldMetrics,
    /// Clean up temporary files
    TempFiles,
    /// Clean up log files
    LogFiles,
    /// Clean up backup files
    BackupFiles,
}

/// Cleanup thresholds
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CleanupThresholds {
    /// Disk usage threshold for cleanup (percentage)
    #[validate(range(min = 50, max = 95))]
    pub disk_usage_percent: u8,

    /// Age threshold for cleanup (hours)
    #[validate(range(min = 1, max = 8760))]
    pub age_threshold_hours: u64,

    /// Size threshold for cleanup (MB)
    #[validate(range(min = 1, max = 102400))]
    pub size_threshold_mb: u64,
}

impl Default for CleanupThresholds {
    fn default() -> Self {
        Self {
            disk_usage_percent: 80,
            age_threshold_hours: 168, // 1 week
            size_threshold_mb: 1024,  // 1GB
        }
    }
}

/// Compaction configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CompactionConfig {
    /// Whether compaction is enabled
    pub enabled: bool,

    /// Compaction strategy
    pub strategy: CompactionStrategy,

    /// Compaction triggers
    pub triggers: Vec<CompactionTrigger>,

    /// Compaction settings
    #[validate(nested)]
    pub settings: CompactionSettings,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            strategy: CompactionStrategy::LeveledCompaction,
            triggers: vec![CompactionTrigger::SizeThreshold, CompactionTrigger::FragmentationRatio],
            settings: CompactionSettings::default(),
        }
    }
}

/// Compaction strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompactionStrategy {
    /// No compaction
    None,
    /// Size-tiered compaction
    SizeTiered,
    /// Leveled compaction
    LeveledCompaction,
    /// Universal compaction
    Universal,
}

impl Default for CompactionStrategy {
    fn default() -> Self {
        Self::LeveledCompaction
    }
}

/// Compaction triggers
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompactionTrigger {
    /// Trigger based on size threshold
    SizeThreshold,
    /// Trigger based on fragmentation ratio
    FragmentationRatio,
    /// Trigger based on time interval
    TimeInterval,
    /// Manual trigger
    Manual,
}

/// Compaction settings
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CompactionSettings {
    /// Maximum compaction parallelism
    #[validate(range(min = 1, max = 16))]
    pub max_parallel_compactions: u32,

    /// Size threshold for compaction (MB)
    #[validate(range(min = 1, max = 10240))]
    pub size_threshold_mb: u64,

    /// Fragmentation ratio threshold
    #[validate(range(min = 0.1, max = 0.9))]
    pub fragmentation_threshold: f64,

    /// Compaction interval (hours)
    #[validate(range(min = 1, max = 168))]
    pub interval_hours: u64,
}

impl Default for CompactionSettings {
    fn default() -> Self {
        Self {
            max_parallel_compactions: 2,
            size_threshold_mb: 100,
            fragmentation_threshold: 0.3, // 30%
            interval_hours: 24,           // Daily
        }
    }
}

/// Validation context for system-specific validation
#[derive(Debug, Clone)]
pub struct ValidationContext {
    /// Number of CPU cores on the system
    pub cpu_cores: usize,

    /// Available memory in MB
    pub available_memory_mb: u64,

    /// Available disk space in MB
    pub available_disk_space_mb: u64,

    /// Operating system
    pub os: String,

    /// System architecture
    pub arch: String,
}

impl ValidationContext {
    /// Detect system configuration automatically
    pub fn detect() -> Self {
        Self {
            cpu_cores: num_cpus::get(),
            available_memory_mb: Self::detect_memory_mb(),
            available_disk_space_mb: Self::detect_disk_space_mb(),
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
        }
    }

    /// Detect available memory in MB
    fn detect_memory_mb() -> u64 {
        // This is a simplified implementation
        // In a real implementation, you would use platform-specific APIs
        8192 // Default to 8GB
    }

    /// Detect available disk space in MB
    fn detect_disk_space_mb() -> u64 {
        // This is a simplified implementation
        // In a real implementation, you would check the actual filesystem
        102400 // Default to 100GB
    }
}

impl Default for ValidationContext {
    fn default() -> Self {
        Self::detect()
    }
}

/// Performance configuration (alias for backward compatibility)
pub type PerformanceConfig = PerformanceMonitoringConfig;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_creation() {
        let config = MemoryStreamerConfig::default();
        assert_eq!(config.network.host, "127.0.0.1");
        assert_eq!(config.network.port, 8080);
        assert!(config.telemetry.logging.enabled);
        assert_eq!(config.telemetry.logging.level, TracingLevel::Info);
    }

    #[test]
    fn test_config_serialization() {
        let config = MemoryStreamerConfig::default();
        let serialized = toml::to_string(&config).unwrap();
        assert!(!serialized.is_empty());

        let deserialized: MemoryStreamerConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.network.host, config.network.host);
    }

    #[test]
    fn test_validation_context() {
        let context = ValidationContext::detect();
        assert!(context.cpu_cores > 0);
        assert!(context.available_memory_mb > 0);
        assert!(!context.os.is_empty());
    }

    #[test]
    fn test_tracing_level_ordering() {
        assert!(TracingLevel::Trace < TracingLevel::Debug);
        assert!(TracingLevel::Debug < TracingLevel::Info);
        assert!(TracingLevel::Info < TracingLevel::Warn);
        assert!(TracingLevel::Warn < TracingLevel::Error);
    }

    #[test]
    fn test_compression_algorithm_default() {
        let alg = CompressionAlgorithm::default();
        assert_eq!(alg, CompressionAlgorithm::Lz4);
    }

    #[test]
    fn test_memory_pool_config_validation() {
        let config = MemoryPoolConfig::default();
        assert!(config.enabled);
        assert!(config.size >= 1048576); // At least 1MB
        assert!(config.max_size >= config.size);
        assert!(config.growth_factor >= 1.1);
    }

    #[test]
    fn test_thread_pool_config() {
        let config = ThreadPoolConfig::default();
        assert!(config.core_threads > 0);
        assert!(config.max_threads >= config.core_threads);
        assert_eq!(config.thread_priority, ThreadPriority::Normal);
    }
}
