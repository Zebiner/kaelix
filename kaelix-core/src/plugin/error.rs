//! Error types for the plugin system.

use thiserror::Error;

/// Plugin system error types.
#[derive(Error, Debug, Clone)]
pub enum PluginError {
    /// Plugin registration errors
    #[error("Plugin registration failed: {message}")]
    Registration {
        /// Error message describing the registration failure
        message: String,
    },

    /// Plugin loading errors
    #[error("Plugin loading failed: {plugin_id}, reason: {reason}")]
    LoadingFailed {
        /// Plugin identifier
        plugin_id: String,
        /// Reason for loading failure
        reason: String,
    },

    /// Plugin initialization errors
    #[error("Plugin initialization failed: {plugin_id}, reason: {reason}")]
    InitializationFailed {
        /// Plugin identifier
        plugin_id: String,
        /// Reason for initialization failure
        reason: String,
    },

    /// Plugin execution errors
    #[error("Plugin execution error: {plugin_id}, operation: {operation}, reason: {reason}")]
    ExecutionError {
        /// Plugin identifier
        plugin_id: String,
        /// Operation that failed
        operation: String,
        /// Reason for execution failure
        reason: String,
    },

    /// Plugin lifecycle errors
    #[error("Plugin lifecycle error: {plugin_id}, state: {state}, reason: {reason}")]
    LifecycleError {
        /// Plugin identifier
        plugin_id: String,
        /// Current plugin state
        state: String,
        /// Reason for lifecycle error
        reason: String,
    },

    /// Invalid plugin state transition
    #[error("Invalid state transition: plugin {plugin_id}, from {from} to {to}")]
    InvalidStateTransition {
        /// Plugin identifier
        plugin_id: String,
        /// Source state
        from: String,
        /// Target state
        to: String,
    },

    /// Plugin configuration errors
    #[error("Plugin configuration error: {plugin_id}, config_key: {config_key}, reason: {reason}")]
    ConfigurationError {
        /// Plugin identifier
        plugin_id: String,
        /// Configuration key that caused the error
        config_key: String,
        /// Reason for configuration error
        reason: String,
    },

    /// Plugin dependency errors
    #[error("Plugin dependency error: {plugin_id}, dependency: {dependency}, reason: {reason}")]
    DependencyError {
        /// Plugin identifier
        plugin_id: String,
        /// Missing or incompatible dependency
        dependency: String,
        /// Reason for dependency error
        reason: String,
    },

    /// Plugin compatibility errors
    #[error("Plugin compatibility error: {plugin_id}, required_version: {required_version}, actual_version: {actual_version}")]
    CompatibilityError {
        /// Plugin identifier
        plugin_id: String,
        /// Required API version
        required_version: String,
        /// Actual API version
        actual_version: String,
    },

    /// Plugin security violations
    #[error("Plugin security violation: {plugin_id}, violation: {violation}")]
    SecurityViolation {
        /// Plugin identifier
        plugin_id: String,
        /// Security violation description
        violation: String,
    },

    /// Plugin resource limit exceeded
    #[error("Plugin resource limit exceeded: {plugin_id}, resource: {resource}, limit: {limit}, actual: {actual}")]
    ResourceLimitExceeded {
        /// Plugin identifier
        plugin_id: String,
        /// Resource type (memory, CPU, etc.)
        resource: String,
        /// Resource limit
        limit: String,
        /// Actual resource usage
        actual: String,
    },

    /// Plugin not found
    #[error("Plugin not found: {plugin_id}")]
    NotFound {
        /// Plugin identifier
        plugin_id: String,
    },

    /// Plugin already exists
    #[error("Plugin already exists: {plugin_id}")]
    AlreadyExists {
        /// Plugin identifier
        plugin_id: String,
    },

    /// Plugin communication error
    #[error("Plugin communication error: {plugin_id}, reason: {reason}")]
    CommunicationError {
        /// Plugin identifier
        plugin_id: String,
        /// Communication error reason
        reason: String,
    },

    /// Plugin timeout error
    #[error("Plugin timeout: {plugin_id}, operation: {operation}, timeout: {timeout}ms")]
    TimeoutError {
        /// Plugin identifier
        plugin_id: String,
        /// Operation that timed out
        operation: String,
        /// Timeout duration in milliseconds
        timeout: u64,
    },

    /// Plugin validation error
    #[error("Plugin validation error: {plugin_id}, field: {field}, reason: {reason}")]
    ValidationError {
        /// Plugin identifier
        plugin_id: String,
        /// Field that failed validation
        field: String,
        /// Validation failure reason
        reason: String,
    },

    /// Plugin serialization/deserialization error
    #[error("Plugin serialization error: {plugin_id}, operation: {operation}, reason: {reason}")]
    SerializationError {
        /// Plugin identifier
        plugin_id: String,
        /// Serialization operation (serialize/deserialize)
        operation: String,
        /// Serialization error reason
        reason: String,
    },

    /// Plugin I/O error
    #[error("Plugin I/O error: {plugin_id}, operation: {operation}, reason: {reason}")]
    IoError {
        /// Plugin identifier
        plugin_id: String,
        /// I/O operation that failed
        operation: String,
        /// I/O error reason
        reason: String,
    },

    /// Internal plugin system error
    #[error("Internal plugin system error: operation: {operation}, reason: {reason}")]
    Internal {
        /// Operation that failed
        operation: String,
        /// Internal error reason
        reason: String,
    },
}

/// Type alias for plugin system results.
pub type PluginResult<T> = Result<T, PluginError>;

/// Registry-specific error types.
#[derive(Error, Debug, Clone)]
pub enum RegistryError {
    /// Plugin already registered
    #[error("Plugin already registered: {plugin_id}")]
    AlreadyRegistered {
        /// Plugin identifier
        plugin_id: String,
    },

    /// Plugin not found in registry
    #[error("Plugin not found in registry: {plugin_id}")]
    NotFound {
        /// Plugin identifier
        plugin_id: String,
    },

    /// Registry capacity exceeded
    #[error("Registry capacity exceeded: current {current}, maximum {maximum}")]
    CapacityExceeded {
        /// Current number of plugins
        current: usize,
        /// Maximum capacity
        maximum: usize,
    },

    /// Registry operation failed
    #[error("Registry operation failed: {operation}, reason: {reason}")]
    OperationFailed {
        /// Operation that failed
        operation: String,
        /// Failure reason
        reason: String,
    },

    /// Registry corruption detected
    #[error("Registry corruption detected: {description}")]
    CorruptionDetected {
        /// Corruption description
        description: String,
    },

    /// Registry access denied
    #[error("Registry access denied: {plugin_id}, reason: {reason}")]
    AccessDenied {
        /// Plugin identifier
        plugin_id: String,
        /// Access denial reason
        reason: String,
    },
}

/// Type alias for registry results.
pub type RegistryResult<T> = Result<T, RegistryError>;

/// Lifecycle management error types.
#[derive(Error, Debug, Clone)]
pub enum LifecycleError {
    /// Invalid state transition
    #[error("Invalid state transition: from {from} to {to}")]
    InvalidTransition {
        /// Source state
        from: String,
        /// Target state
        to: String,
    },

    /// State transition timeout
    #[error("State transition timeout: from {from} to {to}, timeout: {timeout}ms")]
    TransitionTimeout {
        /// Source state
        from: String,
        /// Target state
        to: String,
        /// Timeout duration in milliseconds
        timeout: u64,
    },

    /// Lifecycle operation failed
    #[error("Lifecycle operation failed: {operation}, current_state: {current_state}, reason: {reason}")]
    OperationFailed {
        /// Operation that failed
        operation: String,
        /// Current plugin state
        current_state: String,
        /// Failure reason
        reason: String,
    },

    /// Restart policy violation
    #[error("Restart policy violation: {plugin_id}, attempts: {attempts}, max_attempts: {max_attempts}")]
    RestartPolicyViolation {
        /// Plugin identifier
        plugin_id: String,
        /// Current restart attempts
        attempts: u32,
        /// Maximum allowed attempts
        max_attempts: u32,
    },

    /// Health check failed
    #[error("Health check failed: {plugin_id}, reason: {reason}")]
    HealthCheckFailed {
        /// Plugin identifier
        plugin_id: String,
        /// Health check failure reason
        reason: String,
    },
}

/// Type alias for lifecycle results.
pub type LifecycleResult<T> = Result<T, LifecycleError>;

/// Plugin loader error types.
#[derive(Error, Debug, Clone)]
pub enum LoaderError {
    /// Plugin file not found
    #[error("Plugin file not found: {path}")]
    FileNotFound {
        /// Plugin file path
        path: String,
    },

    /// Plugin file format invalid
    #[error("Invalid plugin file format: {path}, reason: {reason}")]
    InvalidFormat {
        /// Plugin file path
        path: String,
        /// Format validation failure reason
        reason: String,
    },

    /// Plugin signature verification failed
    #[error("Plugin signature verification failed: {path}, reason: {reason}")]
    SignatureVerificationFailed {
        /// Plugin file path
        path: String,
        /// Signature verification failure reason
        reason: String,
    },

    /// Plugin dependency resolution failed
    #[error("Plugin dependency resolution failed: {plugin_id}, missing_dependencies: {missing_dependencies:?}")]
    DependencyResolutionFailed {
        /// Plugin identifier
        plugin_id: String,
        /// List of missing dependencies
        missing_dependencies: Vec<String>,
    },

    /// Dynamic loading failed
    #[error("Dynamic loading failed: {path}, reason: {reason}")]
    DynamicLibraryError {
        /// Plugin file path
        path: String,
        /// Dynamic loading failure reason
        reason: String,
    },

    /// Symbol resolution failed
    #[error("Symbol resolution failed: {symbol}, reason: {reason}")]
    SymbolResolutionFailed {
        /// Symbol name
        symbol: String,
        /// Symbol resolution failure reason
        reason: String,
    },

    /// Plugin metadata invalid
    #[error("Plugin metadata invalid: {path}, field: {field}, reason: {reason}")]
    InvalidMetadata {
        /// Plugin file path
        path: String,
        /// Invalid metadata field
        field: String,
        /// Validation failure reason
        reason: String,
    },

    /// Plugin version incompatible
    #[error("Plugin version incompatible: {plugin_id}, plugin_version: {plugin_version}, required_version: {required_version}")]
    VersionIncompatible {
        /// Plugin identifier
        plugin_id: String,
        /// Plugin version
        plugin_version: String,
        /// Required version
        required_version: String,
    },

    /// Loading permission denied
    #[error("Loading permission denied: {path}, reason: {reason}")]
    PermissionDenied {
        /// Plugin file path
        path: String,
        /// Permission denial reason
        reason: String,
    },

    /// Loading operation failed
    #[error("Loading failed: {path}, reason: {reason}")]
    LoadingFailed {
        /// Plugin file path
        path: String,
        /// Loading failure reason
        reason: String,
    },
}

/// Type alias for loader results.
pub type LoaderResult<T> = Result<T, LoaderError>;

/// Sandbox and security error types.
#[derive(Error, Debug, Clone)]
pub enum SandboxError {
    /// Sandbox creation failed
    #[error("Sandbox creation failed: {plugin_id}, reason: {reason}")]
    CreationFailed {
        /// Plugin identifier
        plugin_id: String,
        /// Creation failure reason
        reason: String,
    },

    /// Sandbox enforcement failed
    #[error("Sandbox enforcement failed: {plugin_id}, violation: {violation}")]
    EnforcementFailed {
        /// Plugin identifier
        plugin_id: String,
        /// Security violation description
        violation: String,
    },

    /// Resource limit exceeded
    #[error("Resource limit exceeded: {plugin_id}, resource: {resource}, limit: {limit}, usage: {usage}")]
    ResourceLimitExceeded {
        /// Plugin identifier
        plugin_id: String,
        /// Resource type
        resource: String,
        /// Resource limit
        limit: u64,
        /// Current usage
        usage: u64,
    },

    /// Capability violation
    #[error("Capability violation: {plugin_id}, required_capability: {required_capability}")]
    CapabilityViolation {
        /// Plugin identifier
        plugin_id: String,
        /// Required capability
        required_capability: String,
    },

    /// Isolation breach
    #[error("Isolation breach detected: {plugin_id}, breach_type: {breach_type}")]
    IsolationBreach {
        /// Plugin identifier
        plugin_id: String,
        /// Type of isolation breach
        breach_type: String,
    },
}

/// Type alias for sandbox results.
pub type SandboxResult<T> = Result<T, SandboxError>;

/// Hook system error types.
#[derive(Error, Debug, Clone)]
pub enum HookError {
    /// Hook registration failed
    #[error("Hook registration failed: {plugin_id}, hook_point: {hook_point}, reason: {reason}")]
    RegistrationFailed {
        /// Plugin identifier
        plugin_id: String,
        /// Hook point
        hook_point: String,
        /// Registration failure reason
        reason: String,
    },

    /// Hook execution failed
    #[error("Hook execution failed: {plugin_id}, hook_point: {hook_point}, reason: {reason}")]
    ExecutionFailed {
        /// Plugin identifier
        plugin_id: String,
        /// Hook point
        hook_point: String,
        /// Execution failure reason
        reason: String,
    },

    /// Hook priority conflict
    #[error("Hook priority conflict: {plugin_id}, hook_point: {hook_point}, conflicting_plugin: {conflicting_plugin}")]
    PriorityConflict {
        /// Plugin identifier
        plugin_id: String,
        /// Hook point
        hook_point: String,
        /// Conflicting plugin identifier
        conflicting_plugin: String,
    },

    /// Hook chain broken
    #[error("Hook chain broken: {hook_point}, failed_plugin: {failed_plugin}, reason: {reason}")]
    ChainBroken {
        /// Hook point
        hook_point: String,
        /// Plugin that broke the chain
        failed_plugin: String,
        /// Chain breakage reason
        reason: String,
    },

    /// Hook not found
    #[error("Hook not found: {plugin_id}, hook_point: {hook_point}")]
    NotFound {
        /// Plugin identifier
        plugin_id: String,
        /// Hook point
        hook_point: String,
    },
}

/// Type alias for hook results.
pub type HookResult<T> = Result<T, HookError>;

/// Plugin reload error types.
#[derive(Error, Debug, Clone)]
pub enum ReloadError {
    /// Reload operation failed
    #[error("Reload operation failed: {plugin_id}, phase: {phase}, reason: {reason}")]
    OperationFailed {
        /// Plugin identifier
        plugin_id: String,
        /// Reload phase that failed
        phase: String,
        /// Failure reason
        reason: String,
    },

    /// Hot reload not supported
    #[error("Hot reload not supported: {plugin_id}, reason: {reason}")]
    NotSupported {
        /// Plugin identifier
        plugin_id: String,
        /// Reason why hot reload is not supported
        reason: String,
    },

    /// State preservation failed
    #[error("State preservation failed: {plugin_id}, reason: {reason}")]
    StatePreservationFailed {
        /// Plugin identifier
        plugin_id: String,
        /// State preservation failure reason
        reason: String,
    },

    /// Version mismatch during reload
    #[error("Version mismatch during reload: {plugin_id}, old_version: {old_version}, new_version: {new_version}")]
    VersionMismatch {
        /// Plugin identifier
        plugin_id: String,
        /// Old plugin version
        old_version: String,
        /// New plugin version
        new_version: String,
    },

    /// Rollback failed
    #[error("Reload rollback failed: {plugin_id}, reason: {reason}")]
    RollbackFailed {
        /// Plugin identifier
        plugin_id: String,
        /// Rollback failure reason
        reason: String,
    },
}

/// Type alias for reload results.
pub type ReloadResult<T> = Result<T, ReloadError>;

/// Security-specific error types.
#[derive(Error, Debug, Clone)]
pub enum SecurityError {
    /// Authentication failed
    #[error("Authentication failed: {plugin_id}, reason: {reason}")]
    AuthenticationFailed {
        /// Plugin identifier
        plugin_id: String,
        /// Authentication failure reason
        reason: String,
    },

    /// Authorization failed
    #[error("Authorization failed: {plugin_id}, operation: {operation}, reason: {reason}")]
    AuthorizationFailed {
        /// Plugin identifier
        plugin_id: String,
        /// Attempted operation
        operation: String,
        /// Authorization failure reason
        reason: String,
    },

    /// Certificate validation failed
    #[error("Certificate validation failed: {plugin_id}, reason: {reason}")]
    CertificateValidationFailed {
        /// Plugin identifier
        plugin_id: String,
        /// Certificate validation failure reason
        reason: String,
    },

    /// Cryptographic operation failed
    #[error("Cryptographic operation failed: {operation}, reason: {reason}")]
    CryptographicOperationFailed {
        /// Cryptographic operation
        operation: String,
        /// Operation failure reason
        reason: String,
    },

    /// Trust verification failed
    #[error("Trust verification failed: {plugin_id}, reason: {reason}")]
    TrustVerificationFailed {
        /// Plugin identifier
        plugin_id: String,
        /// Trust verification failure reason
        reason: String,
    },
}

/// Type alias for security results.
pub type SecurityResult<T> = Result<T, SecurityError>;

/// Enforcement-specific error types.
#[derive(Error, Debug, Clone)]
pub enum EnforcementError {
    /// Policy violation
    #[error("Policy violation: {plugin_id}, policy: {policy}, violation: {violation}")]
    PolicyViolation {
        /// Plugin identifier
        plugin_id: String,
        /// Violated policy
        policy: String,
        /// Violation description
        violation: String,
    },

    /// Quota exceeded
    #[error("Quota exceeded: {plugin_id}, quota_type: {quota_type}, limit: {limit}, usage: {usage}")]
    QuotaExceeded {
        /// Plugin identifier
        plugin_id: String,
        /// Type of quota
        quota_type: String,
        /// Quota limit
        limit: u64,
        /// Current usage
        usage: u64,
    },

    /// Rate limit exceeded
    #[error("Rate limit exceeded: {plugin_id}, operation: {operation}, limit: {limit}, window: {window}ms")]
    RateLimitExceeded {
        /// Plugin identifier
        plugin_id: String,
        /// Rate-limited operation
        operation: String,
        /// Rate limit
        limit: u64,
        /// Time window in milliseconds
        window: u64,
    },

    /// Enforcement mechanism failed
    #[error("Enforcement mechanism failed: {mechanism}, reason: {reason}")]
    MechanismFailed {
        /// Enforcement mechanism
        mechanism: String,
        /// Failure reason
        reason: String,
    },

    /// Compliance check failed
    #[error("Compliance check failed: {plugin_id}, requirement: {requirement}, reason: {reason}")]
    ComplianceCheckFailed {
        /// Plugin identifier
        plugin_id: String,
        /// Compliance requirement
        requirement: String,
        /// Check failure reason
        reason: String,
    },
}

/// Type alias for enforcement results.
pub type EnforcementResult<T> = Result<T, EnforcementError>;

/// Helper functions for error creation and conversion.
impl PluginError {
    /// Create a registration error.
    pub fn registration(message: impl Into<String>) -> Self {
        Self::Registration {
            message: message.into(),
        }
    }

    /// Create a loading failed error.
    pub fn loading_failed(plugin_id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::LoadingFailed {
            plugin_id: plugin_id.into(),
            reason: reason.into(),
        }
    }

    /// Create an initialization failed error.
    pub fn initialization_failed(plugin_id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::InitializationFailed {
            plugin_id: plugin_id.into(),
            reason: reason.into(),
        }
    }

    /// Create an execution error.
    pub fn execution_error(
        plugin_id: impl Into<String>,
        operation: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self::ExecutionError {
            plugin_id: plugin_id.into(),
            operation: operation.into(),
            reason: reason.into(),
        }
    }

    /// Create a timeout error.
    pub fn timeout(
        plugin_id: impl Into<String>,
        operation: impl Into<String>,
        timeout: u64,
    ) -> Self {
        Self::TimeoutError {
            plugin_id: plugin_id.into(),
            operation: operation.into(),
            timeout,
        }
    }

    /// Create a not found error.
    pub fn not_found(plugin_id: impl Into<String>) -> Self {
        Self::NotFound {
            plugin_id: plugin_id.into(),
        }
    }

    /// Create an already exists error.
    pub fn already_exists(plugin_id: impl Into<String>) -> Self {
        Self::AlreadyExists {
            plugin_id: plugin_id.into(),
        }
    }

    /// Create an internal error.
    pub fn internal(operation: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Internal {
            operation: operation.into(),
            reason: reason.into(),
        }
    }
}

/// Convert various error types into PluginError.
impl From<std::io::Error> for PluginError {
    fn from(error: std::io::Error) -> Self {
        Self::IoError {
            plugin_id: "unknown".to_string(),
            operation: "io_operation".to_string(),
            reason: error.to_string(),
        }
    }
}

impl From<serde_json::Error> for PluginError {
    fn from(error: serde_json::Error) -> Self {
        Self::SerializationError {
            plugin_id: "unknown".to_string(),
            operation: "json_serialization".to_string(),
            reason: error.to_string(),
        }
    }
}

impl From<RegistryError> for PluginError {
    fn from(error: RegistryError) -> Self {
        match error {
            RegistryError::AlreadyRegistered { plugin_id } => Self::AlreadyExists { plugin_id },
            RegistryError::NotFound { plugin_id } => Self::NotFound { plugin_id },
            RegistryError::CapacityExceeded { current, maximum } => Self::Internal {
                operation: "registry_operation".to_string(),
                reason: format!("Registry capacity exceeded: {}/{}", current, maximum),
            },
            RegistryError::OperationFailed { operation, reason } => Self::Internal {
                operation,
                reason: format!("Registry operation failed: {}", reason),
            },
            RegistryError::CorruptionDetected { description } => Self::Internal {
                operation: "registry_operation".to_string(),
                reason: format!("Registry corruption: {}", description),
            },
            RegistryError::AccessDenied { plugin_id, reason } => Self::SecurityViolation {
                plugin_id,
                violation: format!("Registry access denied: {}", reason),
            },
        }
    }
}

impl From<LifecycleError> for PluginError {
    fn from(error: LifecycleError) -> Self {
        match error {
            LifecycleError::InvalidTransition { from, to } => Self::LifecycleError {
                plugin_id: "unknown".to_string(),
                state: from,
                reason: format!("Invalid transition to {}", to),
            },
            LifecycleError::TransitionTimeout { from, to, timeout } => Self::TimeoutError {
                plugin_id: "unknown".to_string(),
                operation: format!("transition_{}_{}", from, to),
                timeout,
            },
            LifecycleError::OperationFailed {
                operation,
                current_state,
                reason,
            } => Self::LifecycleError {
                plugin_id: "unknown".to_string(),
                state: current_state,
                reason: format!("Operation '{}' failed: {}", operation, reason),
            },
            LifecycleError::RestartPolicyViolation {
                plugin_id,
                attempts,
                max_attempts,
            } => Self::LifecycleError {
                plugin_id,
                state: "failed".to_string(),
                reason: format!("Restart attempts {}/{} exceeded", attempts, max_attempts),
            },
            LifecycleError::HealthCheckFailed { plugin_id, reason } => Self::LifecycleError {
                plugin_id,
                state: "unhealthy".to_string(),
                reason: format!("Health check failed: {}", reason),
            },
        }
    }
}

impl From<LoaderError> for PluginError {
    fn from(error: LoaderError) -> Self {
        match error {
            LoaderError::FileNotFound { path } => Self::LoadingFailed {
                plugin_id: path.clone(),
                reason: format!("File not found: {}", path),
            },
            LoaderError::InvalidFormat { path, reason } => Self::LoadingFailed {
                plugin_id: path.clone(),
                reason: format!("Invalid format: {}", reason),
            },
            LoaderError::SignatureVerificationFailed { path, reason } => Self::SecurityViolation {
                plugin_id: path,
                violation: format!("Signature verification failed: {}", reason),
            },
            LoaderError::DependencyResolutionFailed {
                plugin_id,
                missing_dependencies,
            } => Self::DependencyError {
                plugin_id,
                dependency: missing_dependencies.join(", "),
                reason: "Missing dependencies".to_string(),
            },
            LoaderError::DynamicLibraryError { path, reason } => Self::LoadingFailed {
                plugin_id: path,
                reason: format!("Dynamic loading failed: {}", reason),
            },
            LoaderError::SymbolResolutionFailed { symbol, reason } => Self::LoadingFailed {
                plugin_id: "unknown".to_string(),
                reason: format!("Symbol '{}' resolution failed: {}", symbol, reason),
            },
            LoaderError::InvalidMetadata { path, field, reason } => Self::ValidationError {
                plugin_id: path,
                field,
                reason: format!("Invalid metadata: {}", reason),
            },
            LoaderError::VersionIncompatible {
                plugin_id,
                plugin_version,
                required_version,
            } => Self::CompatibilityError {
                plugin_id,
                required_version,
                actual_version: plugin_version,
            },
            LoaderError::PermissionDenied { path, reason } => Self::SecurityViolation {
                plugin_id: path,
                violation: format!("Permission denied: {}", reason),
            },
            LoaderError::LoadingFailed { path, reason } => Self::LoadingFailed {
                plugin_id: path,
                reason,
            },
        }
    }
}

impl From<SandboxError> for PluginError {
    fn from(error: SandboxError) -> Self {
        match error {
            SandboxError::CreationFailed { plugin_id, reason } => Self::InitializationFailed {
                plugin_id,
                reason: format!("Sandbox creation failed: {}", reason),
            },
            SandboxError::EnforcementFailed {
                plugin_id,
                violation,
            } => Self::SecurityViolation {
                plugin_id,
                violation,
            },
            SandboxError::ResourceLimitExceeded {
                plugin_id,
                resource,
                limit,
                usage,
            } => Self::ResourceLimitExceeded {
                plugin_id,
                resource,
                limit: limit.to_string(),
                actual: usage.to_string(),
            },
            SandboxError::CapabilityViolation {
                plugin_id,
                required_capability,
            } => Self::SecurityViolation {
                plugin_id,
                violation: format!("Missing capability: {}", required_capability),
            },
            SandboxError::IsolationBreach {
                plugin_id,
                breach_type,
            } => Self::SecurityViolation {
                plugin_id,
                violation: format!("Isolation breach: {}", breach_type),
            },
        }
    }
}

impl From<HookError> for PluginError {
    fn from(error: HookError) -> Self {
        match error {
            HookError::RegistrationFailed {
                plugin_id,
                hook_point,
                reason,
            } => Self::Registration {
                message: format!(
                    "Hook registration failed for plugin {} at {}: {}",
                    plugin_id, hook_point, reason
                ),
            },
            HookError::ExecutionFailed {
                plugin_id,
                hook_point,
                reason,
            } => Self::ExecutionError {
                plugin_id,
                operation: format!("hook_{}", hook_point),
                reason,
            },
            HookError::PriorityConflict {
                plugin_id,
                hook_point,
                conflicting_plugin,
            } => Self::ConfigurationError {
                plugin_id,
                config_key: "hook_priority".to_string(),
                reason: format!(
                    "Priority conflict at {} with plugin {}",
                    hook_point, conflicting_plugin
                ),
            },
            HookError::ChainBroken {
                hook_point,
                failed_plugin,
                reason,
            } => Self::ExecutionError {
                plugin_id: failed_plugin,
                operation: format!("hook_chain_{}", hook_point),
                reason,
            },
            HookError::NotFound {
                plugin_id,
                hook_point,
            } => Self::NotFound {
                plugin_id: format!("{}::{}", plugin_id, hook_point),
            },
        }
    }
}

impl From<SecurityError> for PluginError {
    fn from(error: SecurityError) -> Self {
        match error {
            SecurityError::AuthenticationFailed { plugin_id, reason } => Self::SecurityViolation {
                plugin_id,
                violation: format!("Authentication failed: {}", reason),
            },
            SecurityError::AuthorizationFailed { plugin_id, operation, reason } => Self::SecurityViolation {
                plugin_id,
                violation: format!("Authorization failed for operation '{}': {}", operation, reason),
            },
            SecurityError::CertificateValidationFailed { plugin_id, reason } => Self::SecurityViolation {
                plugin_id,
                violation: format!("Certificate validation failed: {}", reason),
            },
            SecurityError::CryptographicOperationFailed { operation, reason } => Self::Internal {
                operation,
                reason: format!("Cryptographic operation failed: {}", reason),
            },
            SecurityError::TrustVerificationFailed { plugin_id, reason } => Self::SecurityViolation {
                plugin_id,
                violation: format!("Trust verification failed: {}", reason),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_error_creation() {
        let error = PluginError::registration("Test registration error");
        assert!(error.to_string().contains("Test registration error"));

        let error = PluginError::loading_failed("test-plugin", "File not found");
        assert!(error.to_string().contains("test-plugin"));
        assert!(error.to_string().contains("File not found"));

        let error = PluginError::timeout("test-plugin", "initialize", 5000);
        assert!(error.to_string().contains("test-plugin"));
        assert!(error.to_string().contains("5000"));
    }

    #[test]
    fn test_error_conversions() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
        let plugin_error: PluginError = io_error.into();
        assert!(plugin_error.to_string().contains("File not found"));

        let registry_error = RegistryError::NotFound {
            plugin_id: "test-plugin".to_string(),
        };
        let plugin_error: PluginError = registry_error.into();
        assert!(plugin_error.to_string().contains("test-plugin"));
    }

    #[test]
    fn test_error_types() {
        let lifecycle_error = LifecycleError::InvalidTransition {
            from: "Running".to_string(),
            to: "Unloaded".to_string(),
        };
        assert!(lifecycle_error.to_string().contains("Running"));
        assert!(lifecycle_error.to_string().contains("Unloaded"));

        let sandbox_error = SandboxError::ResourceLimitExceeded {
            plugin_id: "test-plugin".to_string(),
            resource: "memory".to_string(),
            limit: 1000,
            usage: 1500,
        };
        assert!(sandbox_error.to_string().contains("test-plugin"));
        assert!(sandbox_error.to_string().contains("memory"));
        assert!(sandbox_error.to_string().contains("1000"));
        assert!(sandbox_error.to_string().contains("1500"));
    }

    #[test]
    fn test_hook_error() {
        let hook_error = HookError::ExecutionFailed {
            plugin_id: "test-plugin".to_string(),
            hook_point: "pre_process".to_string(),
            reason: "Invalid message format".to_string(),
        };
        assert!(hook_error.to_string().contains("test-plugin"));
        assert!(hook_error.to_string().contains("pre_process"));
        assert!(hook_error.to_string().contains("Invalid message format"));
    }

    #[test]
    fn test_security_error() {
        let security_error = SecurityError::AuthenticationFailed {
            plugin_id: "test-plugin".to_string(),
            reason: "Invalid credentials".to_string(),
        };
        assert!(security_error.to_string().contains("test-plugin"));
        assert!(security_error.to_string().contains("Invalid credentials"));
    }

    #[test]
    fn test_security_error_conversion() {
        let security_error = SecurityError::AuthenticationFailed {
            plugin_id: "test-plugin".to_string(),
            reason: "Invalid credentials".to_string(),
        };
        let plugin_error: PluginError = security_error.into();
        assert!(plugin_error.to_string().contains("test-plugin"));
        assert!(plugin_error.to_string().contains("Authentication failed"));
    }
}