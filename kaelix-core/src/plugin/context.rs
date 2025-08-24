use crate::{Error as MemoryStreamerError, Message};
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU64, AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Processing context for plugins with comprehensive state management.
///
/// Provides isolated context for plugin execution with:
/// - Security enforcement and sandboxing
/// - Performance monitoring and limits
/// - Metadata management
/// - Child context creation for nested processing
/// - Comprehensive metrics collection
#[derive(Debug)]
pub struct ProcessingContext {
    /// Unique identifier for this processing context
    pub id: Uuid,

    /// Current message being processed
    pub message: Message,

    /// Processing start timestamp for latency tracking
    pub start_time: Instant,

    /// Execution limits and constraints
    pub limits: ProcessingLimits,

    /// Security context for isolation
    pub security: Option<SecurityContext>,

    /// Thread-safe metadata storage
    pub metadata: Arc<RwLock<HashMap<String, String>>>,

    /// Child contexts for nested processing
    pub children: Arc<Mutex<Vec<ProcessingContext>>>,

    /// Plugin processing chain state
    pub plugin_chain: Arc<Mutex<PluginChain>>,

    /// Performance and usage metrics
    pub metrics: ContextMetrics,
}

/// Resource limits for processing operations.
#[derive(Debug, Clone)]
pub struct ProcessingLimits {
    /// Maximum memory allocation (bytes)
    pub max_memory: usize,

    /// Maximum execution time
    pub max_duration: Duration,

    /// Maximum number of child contexts
    pub max_children: usize,

    /// Maximum CPU time (nanoseconds)
    pub max_cpu_time: u64,

    /// Maximum number of system calls
    pub max_syscalls: u64,
}

/// Security context for plugin isolation.
#[derive(Debug, Clone)]
pub struct SecurityContext {
    /// Security level (0-100, higher = more restrictive)
    pub security_level: u8,

    /// Allowed capabilities
    pub capabilities: Vec<String>,

    /// Resource access permissions
    pub permissions: SecurityPermissions,

    /// Cryptographic context for secure operations
    pub crypto: Option<CryptoContext>,
}

/// Security permissions for resource access.
#[derive(Debug, Clone)]
pub struct SecurityPermissions {
    /// File system access permissions
    pub filesystem: FileSystemPermissions,

    /// Network access permissions
    pub network: NetworkPermissions,

    /// System resource permissions
    pub system: SystemPermissions,
}

/// File system access permissions.
#[derive(Debug, Clone)]
pub struct FileSystemPermissions {
    /// Allowed read paths
    pub read_paths: Vec<String>,

    /// Allowed write paths
    pub write_paths: Vec<String>,

    /// Maximum file size for operations
    pub max_file_size: usize,
}

/// Network access permissions.
#[derive(Debug, Clone)]
pub struct NetworkPermissions {
    /// Allowed outbound connections
    pub allowed_hosts: Vec<String>,

    /// Allowed ports
    pub allowed_ports: Vec<u16>,

    /// Maximum bandwidth (bytes/second)
    pub max_bandwidth: usize,
}

/// System resource permissions.
#[derive(Debug, Clone)]
pub struct SystemPermissions {
    /// Can spawn processes
    pub can_spawn_processes: bool,

    /// Can access environment variables
    pub can_access_env: bool,

    /// Can access system information
    pub can_access_sysinfo: bool,
}

/// Cryptographic context for secure operations.
#[derive(Debug, Clone)]
pub struct CryptoContext {
    /// Encryption key for secure storage
    pub encryption_key: Vec<u8>,

    /// Signing key for message authentication
    pub signing_key: Vec<u8>,

    /// Key derivation function parameters
    pub kdf_params: KdfParameters,
}

/// Key derivation function parameters.
#[derive(Debug, Clone)]
pub struct KdfParameters {
    /// Algorithm identifier
    pub algorithm: String,

    /// Salt for key derivation
    pub salt: Vec<u8>,

    /// Iteration count
    pub iterations: u32,
}

/// Plugin processing chain state.
#[derive(Debug)]
pub struct PluginChain {
    /// Plugins in the processing chain
    pub plugins: Vec<String>,

    /// Current plugin index
    pub current_index: AtomicUsize,

    /// Chain execution state
    pub state: ChainState,

    /// Per-plugin execution metrics
    pub plugin_metrics: HashMap<String, PluginMetrics>,
}

/// Chain execution state.
#[derive(Debug, Clone, PartialEq)]
pub enum ChainState {
    /// Chain is ready to execute
    Ready,

    /// Chain is currently executing
    Executing,

    /// Chain has completed successfully
    Completed,

    /// Chain execution failed
    Failed(String),

    /// Chain execution was cancelled
    Cancelled,
}

/// Per-plugin execution metrics.
#[derive(Debug, Default)]
pub struct PluginMetrics {
    /// Number of times this plugin was executed
    pub execution_count: AtomicU64,

    /// Total execution time in nanoseconds
    pub total_execution_time: AtomicU64,

    /// Number of successful executions
    pub success_count: AtomicU64,

    /// Number of failed executions
    pub failure_count: AtomicU64,

    /// Average memory usage during execution
    pub avg_memory_usage: AtomicU64,
}

/// Context performance and usage metrics.
#[derive(Debug, Default)]
pub struct ContextMetrics {
    /// Number of child contexts created
    pub children_created: AtomicU64,

    /// Total memory allocated
    pub memory_allocated: AtomicU64,

    /// Peak memory usage
    pub peak_memory_usage: AtomicU64,

    /// Total CPU time consumed
    pub cpu_time_used: AtomicU64,

    /// Number of security violations detected
    pub security_violations: AtomicU64,

    /// Number of metadata operations
    pub metadata_operations: AtomicU64,
}

impl Default for ProcessingLimits {
    fn default() -> Self {
        Self {
            max_memory: 64 * 1024 * 1024, // 64MB
            max_duration: Duration::from_secs(30),
            max_children: 100,
            max_cpu_time: 10_000_000_000, // 10 seconds in nanoseconds
            max_syscalls: 10000,
        }
    }
}

impl Default for SecurityContext {
    fn default() -> Self {
        Self {
            security_level: 50,
            capabilities: vec!["read_config".to_string(), "write_temp".to_string()],
            permissions: SecurityPermissions::default(),
            crypto: None,
        }
    }
}

impl Default for SecurityPermissions {
    fn default() -> Self {
        Self {
            filesystem: FileSystemPermissions::default(),
            network: NetworkPermissions::default(),
            system: SystemPermissions::default(),
        }
    }
}

impl Default for FileSystemPermissions {
    fn default() -> Self {
        Self {
            read_paths: vec!["/tmp".to_string()],
            write_paths: vec!["/tmp".to_string()],
            max_file_size: 10 * 1024 * 1024, // 10MB
        }
    }
}

impl Default for NetworkPermissions {
    fn default() -> Self {
        Self {
            allowed_hosts: vec!["localhost".to_string()],
            allowed_ports: vec![8080, 9090],
            max_bandwidth: 1024 * 1024, // 1MB/s
        }
    }
}

impl Default for SystemPermissions {
    fn default() -> Self {
        Self { can_spawn_processes: false, can_access_env: false, can_access_sysinfo: false }
    }
}

impl PluginChain {
    /// Create a new plugin chain.
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
            current_index: AtomicUsize::new(0),
            state: ChainState::Ready,
            plugin_metrics: HashMap::new(),
        }
    }

    /// Add a plugin to the chain.
    pub fn add_plugin(&mut self, plugin_name: String) {
        self.plugins.push(plugin_name.clone());
        self.plugin_metrics.insert(plugin_name, PluginMetrics::default());
    }

    /// Get completion percentage.
    pub fn completion_percentage(&self) -> f64 {
        if self.plugins.is_empty() {
            return 100.0;
        }

        let current = self.current_index.load(Ordering::Relaxed);
        (current as f64 / self.plugins.len() as f64) * 100.0
    }

    /// Advance to next plugin in the chain.
    pub fn advance(&self) -> bool {
        let current = self.current_index.load(Ordering::Relaxed);
        if current < self.plugins.len() {
            self.current_index.store(current + 1, Ordering::Relaxed);
            true
        } else {
            false
        }
    }
}

impl ProcessingContext {
    /// Create a new processing context.
    pub fn new(message: &Message) -> Self {
        Self {
            id: Uuid::new_v4(),
            message: message.clone(),
            start_time: Instant::now(),
            limits: ProcessingLimits::default(),
            security: None,
            metadata: Arc::new(RwLock::new(HashMap::new())),
            children: Arc::new(Mutex::new(Vec::new())),
            plugin_chain: Arc::new(Mutex::new(PluginChain::new())),
            metrics: ContextMetrics::default(),
        }
    }

    /// Create context with custom limits.
    pub fn with_limits(mut self, limits: ProcessingLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Create context with security context.
    pub fn with_security(mut self, security: SecurityContext) -> Self {
        self.security = Some(security);
        self
    }

    /// Add metadata key-value pair.
    pub fn add_metadata(&self, key: String, value: String) -> Result<(), MemoryStreamerError> {
        let mut metadata = self.metadata.write();
        metadata.insert(key, value);
        self.metrics.metadata_operations.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Get metadata value by key.
    pub fn get_metadata(&self, key: &str) -> Option<String> {
        self.metadata.read().get(key).cloned()
    }

    /// Get completion percentage of plugin processing chain.
    pub fn completion_percentage(&self) -> f64 {
        self.plugin_chain.lock().completion_percentage()
    }

    /// Create a child context for nested processing.
    pub fn create_child_context(&self, message: &Message) -> ProcessingContext {
        let mut child = ProcessingContext::new(message).with_limits(self.limits.clone());

        // Add security context if present
        if let Some(ref security) = self.security {
            child = child.with_security(security.clone());
        }

        // Copy shared metadata
        let parent_metadata = self.metadata.read();
        for (key, value) in parent_metadata.iter() {
            let _ = child.add_metadata(key.clone(), value.clone());
        }

        self.children.lock().push(child.clone());
        self.metrics.children_created.fetch_add(1, Ordering::Relaxed);

        child
    }

    /// Add a plugin to the processing chain.
    pub fn add_plugin_to_chain(&self, plugin_name: String) {
        self.plugin_chain.lock().add_plugin(plugin_name);
    }

    /// Execute the plugin chain.
    pub async fn execute_chain(&self) -> Result<(), MemoryStreamerError> {
        let mut chain = self.plugin_chain.lock();
        chain.state = ChainState::Executing;

        // Plugin execution would be implemented here
        // This is a placeholder for the actual plugin execution logic

        chain.state = ChainState::Completed;
        Ok(())
    }

    /// Get execution metrics.
    pub fn get_metrics(&self) -> ContextMetricsSnapshot {
        ContextMetricsSnapshot {
            children_created: self.metrics.children_created.load(Ordering::Relaxed),
            memory_allocated: self.metrics.memory_allocated.load(Ordering::Relaxed),
            peak_memory_usage: self.metrics.peak_memory_usage.load(Ordering::Relaxed),
            cpu_time_used: self.metrics.cpu_time_used.load(Ordering::Relaxed),
            security_violations: self.metrics.security_violations.load(Ordering::Relaxed),
            metadata_operations: self.metrics.metadata_operations.load(Ordering::Relaxed),
            elapsed_time: self.start_time.elapsed(),
        }
    }

    /// Check if resource limits are exceeded.
    pub fn check_limits(&self) -> Result<(), MemoryStreamerError> {
        let metrics = self.get_metrics();

        // Check execution time limit
        if metrics.elapsed_time > self.limits.max_duration {
            return Err(MemoryStreamerError::resource_limit(
                "execution_time",
                format!(
                    "Execution time limit exceeded: {:?} > {:?}",
                    metrics.elapsed_time, self.limits.max_duration
                ),
            ));
        }

        // Check memory limit
        if metrics.memory_allocated > self.limits.max_memory as u64 {
            return Err(MemoryStreamerError::resource_limit(
                "memory",
                format!(
                    "Memory limit exceeded: {} > {}",
                    metrics.memory_allocated, self.limits.max_memory
                ),
            ));
        }

        // Check child context limit
        let child_count = self.children.lock().len();
        if child_count > self.limits.max_children {
            return Err(MemoryStreamerError::resource_limit(
                "child_contexts",
                format!(
                    "Child context limit exceeded: {} > {}",
                    child_count, self.limits.max_children
                ),
            ));
        }

        Ok(())
    }

    /// Validate security permissions for an operation.
    pub fn validate_security(
        &self,
        operation: &str,
        resource: &str,
    ) -> Result<(), MemoryStreamerError> {
        if let Some(ref security) = self.security {
            // Basic security validation - would be expanded with actual security logic
            match operation {
                "read_file" => {
                    if !security
                        .permissions
                        .filesystem
                        .read_paths
                        .iter()
                        .any(|path| resource.starts_with(path))
                    {
                        self.metrics.security_violations.fetch_add(1, Ordering::Relaxed);
                        return Err(MemoryStreamerError::security_violation(format!(
                            "File read access denied: {}",
                            resource
                        )));
                    }
                },
                "write_file" => {
                    if !security
                        .permissions
                        .filesystem
                        .write_paths
                        .iter()
                        .any(|path| resource.starts_with(path))
                    {
                        self.metrics.security_violations.fetch_add(1, Ordering::Relaxed);
                        return Err(MemoryStreamerError::security_violation(format!(
                            "File write access denied: {}",
                            resource
                        )));
                    }
                },
                "network_connect" => {
                    if !security
                        .permissions
                        .network
                        .allowed_hosts
                        .iter()
                        .any(|host| resource.contains(host))
                    {
                        self.metrics.security_violations.fetch_add(1, Ordering::Relaxed);
                        return Err(MemoryStreamerError::security_violation(format!(
                            "Network access denied: {}",
                            resource
                        )));
                    }
                },
                _ => {
                    // Unknown operation - deny by default
                    self.metrics.security_violations.fetch_add(1, Ordering::Relaxed);
                    return Err(MemoryStreamerError::security_violation(format!(
                        "Unknown operation denied: {}",
                        operation
                    )));
                },
            }
        }
        Ok(())
    }
}

impl Clone for ProcessingContext {
    fn clone(&self) -> Self {
        Self {
            id: Uuid::new_v4(), // New unique ID for cloned context
            message: self.message.clone(),
            start_time: Instant::now(), // Reset start time
            limits: self.limits.clone(),
            security: self.security.clone(),
            metadata: Arc::new(RwLock::new(self.metadata.read().clone())),
            children: Arc::new(Mutex::new(Vec::new())), // Empty children for clone
            plugin_chain: Arc::new(Mutex::new(PluginChain::new())), // Reset chain
            metrics: ContextMetrics::default(),         // Reset metrics
        }
    }
}

/// Snapshot of context metrics for reporting.
#[derive(Debug, Clone)]
pub struct ContextMetricsSnapshot {
    /// Number of child contexts created
    pub children_created: u64,

    /// Total memory allocated
    pub memory_allocated: u64,

    /// Peak memory usage
    pub peak_memory_usage: u64,

    /// Total CPU time consumed
    pub cpu_time_used: u64,

    /// Number of security violations detected
    pub security_violations: u64,

    /// Number of metadata operations
    pub metadata_operations: u64,

    /// Total elapsed time since context creation
    pub elapsed_time: Duration,
}
