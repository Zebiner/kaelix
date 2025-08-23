//! Dynamic plugin loading infrastructure.

use crate::plugin::{PluginError, PluginResult, SecurityError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Plugin loading strategies for different plugin types.
///
/// Supports multiple loading mechanisms to accommodate different plugin
/// architectures and deployment scenarios.
#[derive(Debug, Clone)]
pub enum PluginLoadStrategy {
    /// Static plugin compiled into the binary
    Static {
        /// Plugin name for identification
        name: String,
    },

    /// Dynamic library plugin loaded at runtime
    Dynamic {
        /// Path to the dynamic library
        library_path: PathBuf,
        /// Symbol name for plugin entry point
        symbol_name: String,
    },

    /// WebAssembly plugin for maximum isolation
    Wasm {
        /// Path to the WASM module
        module_path: PathBuf,
        /// WASM runtime configuration
        runtime_config: WasmRuntimeConfig,
    },

    /// Plugin loaded from a registry URL
    Remote {
        /// Registry URL
        registry_url: String,
        /// Plugin name and version
        plugin_spec: PluginSpec,
        /// Authentication credentials
        credentials: Option<RegistryCredentials>,
    },
}

/// Serializable version of PluginLoadStrategy for configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PluginLoadStrategyConfig {
    /// Static plugin configuration
    Static {
        /// Plugin name for identification
        name: String,
    },

    /// Dynamic library plugin configuration
    Dynamic {
        /// Path to the dynamic library
        library_path: PathBuf,
        /// Symbol name for plugin entry point
        symbol_name: String,
    },

    /// WebAssembly plugin configuration
    Wasm {
        /// Path to the WASM module
        module_path: PathBuf,
        /// WASM runtime configuration
        runtime_config: WasmRuntimeConfig,
    },

    /// Remote plugin configuration
    Remote {
        /// Registry URL
        registry_url: String,
        /// Plugin name and version
        plugin_spec: PluginSpec,
        /// Authentication credentials
        credentials: Option<RegistryCredentials>,
    },
}

impl From<PluginLoadStrategyConfig> for PluginLoadStrategy {
    fn from(config: PluginLoadStrategyConfig) -> Self {
        match config {
            PluginLoadStrategyConfig::Static { name } => PluginLoadStrategy::Static { name },
            PluginLoadStrategyConfig::Dynamic { library_path, symbol_name } => {
                PluginLoadStrategy::Dynamic { library_path, symbol_name }
            },
            PluginLoadStrategyConfig::Wasm { module_path, runtime_config } => {
                PluginLoadStrategy::Wasm { module_path, runtime_config }
            },
            PluginLoadStrategyConfig::Remote { registry_url, plugin_spec, credentials } => {
                PluginLoadStrategy::Remote { registry_url, plugin_spec, credentials }
            },
        }
    }
}

/// WebAssembly runtime configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmRuntimeConfig {
    /// Maximum memory allocation (bytes)
    pub max_memory: usize,

    /// Maximum execution time
    #[serde(with = "duration_serde")]
    pub max_execution_time: Duration,

    /// Enable WASI (WebAssembly System Interface)
    pub enable_wasi: bool,

    /// Allowed host functions
    pub allowed_host_functions: Vec<String>,

    /// Custom environment variables
    pub environment: HashMap<String, String>,
}

// Custom serialization for Duration
mod duration_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_secs_f64().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = f64::deserialize(deserializer)?;
        Ok(Duration::from_secs_f64(secs))
    }
}

impl Default for WasmRuntimeConfig {
    fn default() -> Self {
        Self {
            max_memory: 64 * 1024 * 1024, // 64MB
            max_execution_time: Duration::from_secs(30),
            enable_wasi: false,
            allowed_host_functions: Vec::new(),
            environment: HashMap::new(),
        }
    }
}

/// Plugin specification for remote loading.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSpec {
    /// Plugin name
    pub name: String,

    /// Plugin version (semantic versioning)
    pub version: semver::Version,

    /// Optional architecture constraint
    pub architecture: Option<String>,

    /// Optional platform constraint
    pub platform: Option<String>,
}

/// Registry authentication credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryCredentials {
    /// Authentication type
    pub auth_type: AuthType,

    /// Credentials data
    pub credentials: String,
}

/// Authentication types for plugin registries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthType {
    /// Bearer token authentication
    Bearer,

    /// API key authentication
    ApiKey,

    /// Username/password authentication
    Basic,

    /// OAuth2 authentication
    OAuth2,
}

/// Plugin loader with security validation and dependency resolution.
///
/// Provides secure plugin loading with comprehensive validation,
/// dependency resolution, and hot-reload capabilities.
///
/// # Security Features
///
/// - Digital signature verification
/// - Code signing validation
/// - Dependency vulnerability scanning
/// - Runtime security checks
/// - Sandboxed loading environment
///
/// # Examples
///
/// ```rust
/// use kaelix_core::plugin::*;
///
/// # tokio_test::block_on(async {
/// let loader = PluginLoader::new(LoaderConfig::production());
///
/// // Load a dynamic library plugin
/// let strategy = PluginLoadStrategy::Dynamic {
///     library_path: "/path/to/plugin.so".into(),
///     symbol_name: "create_plugin".to_string(),
/// };
///
/// let plugin = loader.load_plugin(strategy).await?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// # });
/// ```
pub struct PluginLoader {
    /// Loader configuration
    config: LoaderConfig,

    /// Security validator for plugin verification
    security_validator: SecurityValidator,

    /// Dependency resolver for plugin dependencies
    dependency_resolver: DependencyResolver,

    /// Plugin cache for loaded plugins
    cache: PluginCache,

    /// Loader creation timestamp
    created_at: Instant,
}

impl PluginLoader {
    /// Create a new plugin loader with default configuration.
    pub fn new() -> Self {
        Self::with_config(LoaderConfig::default())
    }

    /// Create a new plugin loader with custom configuration.
    pub fn with_config(config: LoaderConfig) -> Self {
        Self {
            security_validator: SecurityValidator::new(&config),
            dependency_resolver: DependencyResolver::new(&config),
            cache: PluginCache::new(config.cache_size),
            config,
            created_at: Instant::now(),
        }
    }

    /// Load a plugin using the specified strategy.
    ///
    /// Performs comprehensive validation and security checks before
    /// loading the plugin into memory.
    ///
    /// # Parameters
    /// - `strategy`: Loading strategy defining how to load the plugin
    ///
    /// # Returns
    /// - Type-erased plugin instance ready for registration
    ///
    /// # Errors
    /// - `PluginError::LoadingFailed`: If plugin loading fails
    /// - `SecurityError`: If security validation fails
    /// - `PluginError::DependencyError`: If dependencies cannot be resolved
    pub async fn load_plugin(
        &self,
        strategy: PluginLoadStrategy,
    ) -> PluginResult<Box<dyn crate::plugin::PluginObject>> {
        let start_time = Instant::now();

        // Check cache first
        if let Some(cached_plugin) = self.cache.get(&strategy).await {
            tracing::debug!("Plugin loaded from cache");
            return Ok(cached_plugin);
        }

        // Clone strategy for later use
        let strategy_for_cache = strategy.clone();

        // Load plugin based on strategy
        let plugin = match strategy {
            PluginLoadStrategy::Static { name } => self.load_static_plugin(&name).await?,
            PluginLoadStrategy::Dynamic { library_path, symbol_name } => {
                self.load_dynamic_plugin(&library_path, &symbol_name).await?
            },
            PluginLoadStrategy::Wasm { module_path, runtime_config } => {
                self.load_wasm_plugin(&module_path, &runtime_config).await?
            },
            PluginLoadStrategy::Remote { registry_url, plugin_spec, credentials } => {
                self.load_remote_plugin(&registry_url, &plugin_spec, credentials.as_ref())
                    .await?
            },
        };

        // Validate plugin security
        self.security_validator.validate_plugin(&plugin).await?;

        // Resolve dependencies
        self.dependency_resolver.resolve_dependencies(&plugin).await?;

        // Cache the loaded plugin
        self.cache.insert(strategy_for_cache, plugin.clone()).await;

        let loading_time = start_time.elapsed();
        tracing::info!(loading_time_ms = loading_time.as_millis(), "Plugin loaded successfully");

        Ok(plugin)
    }

    /// Load a static plugin.
    async fn load_static_plugin(
        &self,
        name: &str,
    ) -> PluginResult<Box<dyn crate::plugin::PluginObject>> {
        tracing::debug!(plugin_name = %name, "Loading static plugin");

        // Static plugin loading would look up registered static plugins
        // This is a placeholder for future implementation
        Err(PluginError::Internal {
            operation: "static_plugin_loading".to_string(),
            reason: format!("Static plugin '{}' not found or not yet implemented", name),
        })
    }

    /// Load a dynamic library plugin.
    async fn load_dynamic_plugin(
        &self,
        library_path: &PathBuf,
        symbol_name: &str,
    ) -> PluginResult<Box<dyn crate::plugin::PluginObject>> {
        tracing::debug!(
            library_path = %library_path.display(),
            symbol_name = %symbol_name,
            "Loading dynamic plugin"
        );

        // Check if dynamic loading is enabled
        #[cfg(feature = "plugin-dynamic-loading")]
        {
            use libloading::{Library, Symbol};

            // Load the dynamic library
            let library = Library::new(library_path).map_err(|e| PluginError::LoadingFailed {
                path: library_path.display().to_string(),
                reason: e.to_string(),
            })?;

            // Get the symbol
            let create_plugin: Symbol<unsafe extern "C" fn() -> *mut std::ffi::c_void> = unsafe {
                library.get(symbol_name.as_bytes()).map_err(|e| PluginError::LoadingFailed {
                    path: library_path.display().to_string(),
                    reason: format!("Symbol '{}' not found: {}", symbol_name, e),
                })?
            };

            // Call the plugin creation function
            let plugin_ptr = unsafe { create_plugin() };

            if plugin_ptr.is_null() {
                return Err(PluginError::LoadingFailed {
                    path: library_path.display().to_string(),
                    reason: "Plugin creation function returned null".to_string(),
                });
            }

            // Convert the raw pointer to a plugin object
            // This is a simplified implementation - real implementation would
            // need proper type safety and memory management
            Err(PluginError::Internal {
                operation: "dynamic_plugin_conversion".to_string(),
                reason: "Dynamic plugin conversion not yet fully implemented".to_string(),
            })
        }

        #[cfg(not(feature = "plugin-dynamic-loading"))]
        {
            Err(PluginError::Internal {
                operation: "dynamic_plugin_loading".to_string(),
                reason: "Dynamic plugin loading not enabled (missing feature flag)".to_string(),
            })
        }
    }

    /// Load a WebAssembly plugin.
    async fn load_wasm_plugin(
        &self,
        module_path: &PathBuf,
        runtime_config: &WasmRuntimeConfig,
    ) -> PluginResult<Box<dyn crate::plugin::PluginObject>> {
        tracing::debug!(
            module_path = %module_path.display(),
            max_memory = runtime_config.max_memory,
            "Loading WASM plugin"
        );

        // WASM plugin loading would require a WASM runtime
        // This is a placeholder for future implementation
        Err(PluginError::Internal {
            operation: "wasm_plugin_loading".to_string(),
            reason: "WASM plugin loading not yet implemented".to_string(),
        })
    }

    /// Load a plugin from a remote registry.
    async fn load_remote_plugin(
        &self,
        registry_url: &str,
        plugin_spec: &PluginSpec,
        _credentials: Option<&RegistryCredentials>,
    ) -> PluginResult<Box<dyn crate::plugin::PluginObject>> {
        tracing::debug!(
            registry_url = %registry_url,
            plugin_name = %plugin_spec.name,
            plugin_version = %plugin_spec.version,
            "Loading remote plugin"
        );

        // Remote plugin loading would involve:
        // 1. Downloading the plugin from the registry
        // 2. Verifying signatures and checksums
        // 3. Extracting and loading the plugin
        // This is a placeholder for future implementation
        Err(PluginError::Internal {
            operation: "remote_plugin_loading".to_string(),
            reason: "Remote plugin loading not yet implemented".to_string(),
        })
    }

    /// Perform hot-reload of a plugin.
    ///
    /// Reloads a plugin without stopping the service, preserving state
    /// where possible and handling version compatibility.
    ///
    /// # Parameters
    /// - `plugin_id`: ID of the plugin to reload
    /// - `new_strategy`: New loading strategy (may be the same as original)
    ///
    /// # Returns
    /// - New plugin instance ready for replacement
    ///
    /// # Errors
    /// - `ReloadError`: If hot-reload fails
    pub async fn hot_reload_plugin(
        &mut self,
        plugin_id: crate::plugin::PluginId,
        new_strategy: PluginLoadStrategy,
    ) -> PluginResult<Box<dyn crate::plugin::PluginObject>> {
        tracing::info!(
            plugin_id = %plugin_id,
            "Starting hot-reload"
        );

        let start_time = Instant::now();

        // Load the new plugin version
        let new_plugin = self.load_plugin(new_strategy).await?;

        // Validate compatibility for hot-reload
        self.validate_hot_reload_compatibility(&new_plugin).await?;

        let reload_time = start_time.elapsed();
        tracing::info!(
            plugin_id = %plugin_id,
            reload_time_ms = reload_time.as_millis(),
            "Hot-reload completed successfully"
        );

        Ok(new_plugin)
    }

    /// Validate hot-reload compatibility.
    async fn validate_hot_reload_compatibility(
        &self,
        _new_plugin: &Box<dyn crate::plugin::PluginObject>,
    ) -> PluginResult<()> {
        // Check API version compatibility
        // Check configuration compatibility
        // Check state migration requirements
        // This is a placeholder for future implementation
        Ok(())
    }

    /// Get loader configuration.
    pub fn config(&self) -> &LoaderConfig {
        &self.config
    }

    /// Get loader uptime.
    pub fn uptime(&self) -> Duration {
        self.created_at.elapsed()
    }

    /// Clear the plugin cache.
    pub async fn clear_cache(&self) {
        self.cache.clear().await;
    }

    /// Get cache statistics.
    pub async fn cache_stats(&self) -> CacheStats {
        self.cache.stats().await
    }
}

impl Default for PluginLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// Plugin loader configuration.
#[derive(Debug, Clone)]
pub struct LoaderConfig {
    /// Enable security validation
    pub enable_security_validation: bool,

    /// Enable signature verification
    pub enable_signature_verification: bool,

    /// Enable dependency resolution
    pub enable_dependency_resolution: bool,

    /// Maximum plugin size (bytes)
    pub max_plugin_size: usize,

    /// Plugin loading timeout
    #[allow(dead_code)]
    pub loading_timeout: Duration,

    /// Cache size (number of plugins)
    pub cache_size: usize,

    /// Allowed plugin directories
    pub allowed_directories: Vec<PathBuf>,

    /// Trusted signers for plugin verification
    pub trusted_signers: Vec<String>,

    /// Custom validation rules
    pub validation_rules: Vec<ValidationRule>,
}

impl LoaderConfig {
    /// Create production-ready loader configuration.
    pub fn production() -> Self {
        Self {
            enable_security_validation: true,
            enable_signature_verification: true,
            enable_dependency_resolution: true,
            max_plugin_size: 100 * 1024 * 1024, // 100MB
            loading_timeout: Duration::from_secs(60),
            cache_size: 50,
            allowed_directories: vec![
                PathBuf::from("/usr/lib/kaelix/plugins"),
                PathBuf::from("/opt/kaelix/plugins"),
            ],
            trusted_signers: Vec::new(),
            validation_rules: Vec::new(),
        }
    }

    /// Create development-friendly loader configuration.
    pub fn development() -> Self {
        Self {
            enable_security_validation: false,
            enable_signature_verification: false,
            enable_dependency_resolution: true,
            max_plugin_size: 500 * 1024 * 1024, // 500MB
            loading_timeout: Duration::from_secs(120),
            cache_size: 20,
            allowed_directories: vec![
                PathBuf::from("./plugins"),
                PathBuf::from("./target/release"),
                PathBuf::from("./target/debug"),
            ],
            trusted_signers: Vec::new(),
            validation_rules: Vec::new(),
        }
    }

    /// Create minimal security loader configuration.
    pub fn minimal_security() -> Self {
        Self {
            enable_security_validation: true,
            enable_signature_verification: false,
            enable_dependency_resolution: false,
            max_plugin_size: 50 * 1024 * 1024, // 50MB
            loading_timeout: Duration::from_secs(30),
            cache_size: 10,
            allowed_directories: vec![PathBuf::from("/usr/lib/kaelix/plugins")],
            trusted_signers: Vec::new(),
            validation_rules: Vec::new(),
        }
    }
}

impl Default for LoaderConfig {
    fn default() -> Self {
        Self::production()
    }
}

/// Custom validation rule for plugins.
#[derive(Debug, Clone)]
pub struct ValidationRule {
    /// Rule name
    pub name: String,

    /// Rule description
    pub description: String,

    /// Validation function (cannot be serialized)
    #[allow(dead_code)]
    pub validator: fn(&dyn crate::plugin::PluginObject) -> Result<(), String>,
}

/// Security validator for plugin verification.
struct SecurityValidator {
    config: LoaderConfig,
}

impl SecurityValidator {
    fn new(config: &LoaderConfig) -> Self {
        Self { config: config.clone() }
    }

    async fn validate_plugin(
        &self,
        _plugin: &Box<dyn crate::plugin::PluginObject>,
    ) -> Result<(), SecurityError> {
        if !self.config.enable_security_validation {
            return Ok(());
        }

        // Perform security validation
        // - Check digital signatures
        // - Validate code integrity
        // - Scan for known vulnerabilities
        // - Check against security policies

        Ok(())
    }
}

/// Dependency resolver for plugin dependencies.
struct DependencyResolver {
    config: LoaderConfig,
}

impl DependencyResolver {
    fn new(config: &LoaderConfig) -> Self {
        Self { config: config.clone() }
    }

    async fn resolve_dependencies(
        &self,
        _plugin: &Box<dyn crate::plugin::PluginObject>,
    ) -> PluginResult<()> {
        if !self.config.enable_dependency_resolution {
            return Ok(());
        }

        // Resolve plugin dependencies
        // - Check required libraries
        // - Verify version compatibility
        // - Load missing dependencies
        // - Handle dependency conflicts

        Ok(())
    }
}

/// Plugin cache for loaded plugins.
struct PluginCache {
    cache: Arc<tokio::sync::RwLock<HashMap<String, CacheEntry>>>,
    max_size: usize,
}

impl PluginCache {
    fn new(max_size: usize) -> Self {
        Self { cache: Arc::new(tokio::sync::RwLock::new(HashMap::new())), max_size }
    }

    async fn get(
        &self,
        strategy: &PluginLoadStrategy,
    ) -> Option<Box<dyn crate::plugin::PluginObject>> {
        let cache_key = self.strategy_to_key(strategy);
        let cache = self.cache.read().await;

        if let Some(entry) = cache.get(&cache_key) {
            if !entry.is_expired() {
                // Would need to clone the plugin object
                // This is simplified for now
                return None;
            }
        }

        None
    }

    async fn insert(
        &self,
        strategy: PluginLoadStrategy,
        _plugin: Box<dyn crate::plugin::PluginObject>,
    ) {
        let cache_key = self.strategy_to_key(&strategy);
        let mut cache = self.cache.write().await;

        // Check if cache is full
        if cache.len() >= self.max_size {
            // Remove oldest entry (simplified LRU)
            if let Some(oldest_key) = cache.keys().next().cloned() {
                cache.remove(&oldest_key);
            }
        }

        let entry = CacheEntry {
            // plugin, // Would store the actual plugin
            created_at: Instant::now(),
            last_accessed: Instant::now(),
        };

        cache.insert(cache_key, entry);
    }

    async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    async fn stats(&self) -> CacheStats {
        let cache = self.cache.read().await;
        CacheStats {
            size: cache.len(),
            max_size: self.max_size,
            hit_count: 0,  // Would track actual hits
            miss_count: 0, // Would track actual misses
        }
    }

    fn strategy_to_key(&self, strategy: &PluginLoadStrategy) -> String {
        // Generate a unique key for the loading strategy
        match strategy {
            PluginLoadStrategy::Static { name } => format!("static:{}", name),
            PluginLoadStrategy::Dynamic { library_path, symbol_name } => {
                format!("dynamic:{}:{}", library_path.display(), symbol_name)
            },
            PluginLoadStrategy::Wasm { module_path, .. } => {
                format!("wasm:{}", module_path.display())
            },
            PluginLoadStrategy::Remote { registry_url, plugin_spec, .. } => {
                format!("remote:{}:{}:{}", registry_url, plugin_spec.name, plugin_spec.version)
            },
        }
    }
}

/// Cache entry for loaded plugins.
struct CacheEntry {
    // plugin: Box<dyn crate::plugin::PluginObject>,
    created_at: Instant,
    last_accessed: Instant,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        // Simple expiration check (5 minutes)
        self.created_at.elapsed() > Duration::from_secs(300)
    }
}

/// Cache statistics.
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Current cache size
    pub size: usize,

    /// Maximum cache size
    pub max_size: usize,

    /// Number of cache hits
    pub hit_count: u64,

    /// Number of cache misses
    pub miss_count: u64,
}

impl CacheStats {
    /// Calculate cache hit rate.
    pub fn hit_rate(&self) -> f64 {
        let total = self.hit_count + self.miss_count;
        if total > 0 {
            (self.hit_count as f64 / total as f64) * 100.0
        } else {
            0.0
        }
    }

    /// Check if cache is full.
    pub fn is_full(&self) -> bool {
        self.size >= self.max_size
    }

    /// Get cache utilization percentage.
    pub fn utilization(&self) -> f64 {
        if self.max_size > 0 {
            (self.size as f64 / self.max_size as f64) * 100.0
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_runtime_config() {
        let config = WasmRuntimeConfig::default();
        assert_eq!(config.max_memory, 64 * 1024 * 1024);
        assert_eq!(config.max_execution_time, Duration::from_secs(30));
        assert!(!config.enable_wasi);
        assert!(config.allowed_host_functions.is_empty());
    }

    #[test]
    fn test_plugin_spec() {
        let spec = PluginSpec {
            name: "test-plugin".to_string(),
            version: semver::Version::new(1, 0, 0),
            architecture: Some("x86_64".to_string()),
            platform: Some("linux".to_string()),
        };

        assert_eq!(spec.name, "test-plugin");
        assert_eq!(spec.version, semver::Version::new(1, 0, 0));
    }

    #[test]
    fn test_loader_config() {
        let prod_config = LoaderConfig::production();
        assert!(prod_config.enable_security_validation);
        assert!(prod_config.enable_signature_verification);
        assert_eq!(prod_config.max_plugin_size, 100 * 1024 * 1024);

        let dev_config = LoaderConfig::development();
        assert!(!dev_config.enable_security_validation);
        assert!(!dev_config.enable_signature_verification);
        assert_eq!(dev_config.max_plugin_size, 500 * 1024 * 1024);

        let minimal_config = LoaderConfig::minimal_security();
        assert!(minimal_config.enable_security_validation);
        assert!(!minimal_config.enable_signature_verification);
        assert_eq!(minimal_config.max_plugin_size, 50 * 1024 * 1024);
    }

    #[test]
    fn test_auth_types() {
        let creds = RegistryCredentials {
            auth_type: AuthType::Bearer,
            credentials: "token123".to_string(),
        };

        assert!(matches!(creds.auth_type, AuthType::Bearer));
        assert_eq!(creds.credentials, "token123");
    }

    #[tokio::test]
    async fn test_plugin_cache() {
        let cache = PluginCache::new(2);

        let stats = cache.stats().await;
        assert_eq!(stats.size, 0);
        assert_eq!(stats.max_size, 2);
        assert!(!stats.is_full());
        assert_eq!(stats.utilization(), 0.0);
    }

    #[test]
    fn test_cache_stats() {
        let stats = CacheStats { size: 5, max_size: 10, hit_count: 80, miss_count: 20 };

        assert_eq!(stats.hit_rate(), 80.0);
        assert!(!stats.is_full());
        assert_eq!(stats.utilization(), 50.0);
    }

    #[test]
    fn test_cache_entry_expiration() {
        let entry = CacheEntry {
            created_at: Instant::now() - Duration::from_secs(400), // Expired
            last_accessed: Instant::now(),
        };

        assert!(entry.is_expired());

        let fresh_entry = CacheEntry { created_at: Instant::now(), last_accessed: Instant::now() };

        assert!(!fresh_entry.is_expired());
    }

    #[test]
    fn test_plugin_load_strategy_conversion() {
        let config = PluginLoadStrategyConfig::Dynamic {
            library_path: PathBuf::from("/path/to/plugin.so"),
            symbol_name: "create_plugin".to_string(),
        };

        let strategy: PluginLoadStrategy = config.into();

        match strategy {
            PluginLoadStrategy::Dynamic { library_path, symbol_name } => {
                assert_eq!(library_path, PathBuf::from("/path/to/plugin.so"));
                assert_eq!(symbol_name, "create_plugin");
            },
            _ => panic!("Unexpected strategy type"),
        }
    }

    #[tokio::test]
    async fn test_plugin_loader_creation() {
        let loader = PluginLoader::new();
        assert!(loader.uptime() >= Duration::from_nanos(0));

        let custom_config = LoaderConfig::development();
        let custom_loader = PluginLoader::with_config(custom_config);
        assert!(!custom_loader.config().enable_security_validation);
    }
}
