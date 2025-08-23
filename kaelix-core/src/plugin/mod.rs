//! Plugin system with comprehensive lifecycle management.

pub mod capabilities;
pub mod context;
pub mod error;
pub mod hooks;
pub mod lifecycle;
pub mod loader;
pub mod metrics;
pub mod registry;
pub mod sandbox;
pub mod security;
pub mod traits;

// Core plugin traits and infrastructure
pub use traits::{
    HealthStatus, MessageFilter, MessageProcessingResult, MessageProcessor, MessageTransformer,
    Plugin, PluginHealth, PluginMetadata, PluginObject,
};

// Error handling
pub use error::{HookError, PluginError, PluginResult, RegistryError, SandboxError, SecurityError};

// Processing context
pub use context::ProcessingContext;

// Capabilities and permissions
pub use capabilities::{CapabilityLevel, PluginCapabilities};

// Security and isolation
pub use sandbox::{
    ByteSize, CpuQuota, IsolationLevel, NetworkAccess, PluginSandbox, ResourceLimits,
    SandboxHealthReport, SandboxStatsSnapshot,
};

// Security system
pub use security::{SecurityContext, SecurityPolicy, ThreatLevel};

// Hook system
pub use hooks::{HookManager, HookPoint, HookPriority, HookRegistration, HookResult};

// Lifecycle management
pub use lifecycle::{PluginLifecycle, PluginState, RestartPolicy};

// Plugin loading
pub use loader::{PluginLoadStrategy, PluginLoader};

// Metrics and monitoring
pub use metrics::{PluginMetrics, PluginMonitor};

// Registry and management
pub use registry::{PluginId, PluginRegistry, RegistryStats};
