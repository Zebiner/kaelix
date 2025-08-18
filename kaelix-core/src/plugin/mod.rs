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
pub mod traits;

// Core plugin traits and infrastructure
pub use traits::{
    HealthStatus, MessageFilter, MessageProcessor, MessageProcessingResult, MessageTransformer,
    Plugin, PluginHealth, PluginMetadata, PluginObject,
};

// Error handling
pub use error::{PluginError, PluginResult, RegistryError, SecurityError, SandboxError, HookError};

// Processing context
pub use context::{ProcessingContext};

// Capabilities and permissions
pub use capabilities::{PluginCapabilities, CapabilityLevel};

// Security and isolation
pub use sandbox::{
    ByteSize, CapabilitySet, CpuQuota, IsolationLevel, NetworkAccess, PluginSandbox,
    ResourceLimits, SandboxHealthReport, SandboxStatsSnapshot,
};

// Hook system
pub use hooks::{HookManager, HookPoint, HookPriority, HookRegistration, HookResult};

// Lifecycle management
pub use lifecycle::{PluginLifecycle, PluginState, RestartPolicy};

// Plugin loading
pub use loader::{PluginLoader, PluginLoadStrategy};

// Metrics and monitoring
pub use metrics::{PluginMetrics, PluginMonitor};

// Registry and management
pub use registry::{PluginHandle, PluginId, PluginRegistry, RegistryMetrics};