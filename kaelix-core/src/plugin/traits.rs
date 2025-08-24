//! Core plugin trait definitions and interfaces.

use crate::message::Message;
use crate::plugin::{PluginCapabilities, PluginError, ProcessingContext};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;

/// Core plugin trait for all plugin implementations.
///
/// This trait defines the essential interface that all plugins must implement.
/// It provides zero-cost abstractions for high-performance message processing
/// while maintaining type safety and flexibility.
///
/// # Type Parameters
///
/// - `Config`: Plugin-specific configuration type
/// - `State`: Plugin-specific state type
/// - `Error`: Plugin-specific error type
///
/// # Examples
///
/// ```rust
/// use kaelix_core::plugin::*;
/// use async_trait::async_trait;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Debug, Clone, Deserialize)]
/// pub struct MyPluginConfig {
///     pub name: String,
///     pub timeout: std::time::Duration,
/// }
///
/// #[derive(Debug, Clone)]
/// pub struct MyPluginState {
///     pub counter: u64,
/// }
///
/// #[derive(Debug, thiserror::Error)]
/// pub enum MyPluginError {
///     #[error("Processing failed: {reason}")]
///     ProcessingFailed { reason: String },
/// }
///
/// pub struct MyPlugin;
///
/// #[async_trait]
/// impl Plugin for MyPlugin {
///     type Config = MyPluginConfig;
///     type State = MyPluginState;
///     type Error = MyPluginError;
///
///     async fn initialize(&self, config: Self::Config) -> Result<Self::State, Self::Error> {
///         Ok(MyPluginState { counter: 0 })
///     }
///
///     async fn process_message(
///         &self,
///         state: &mut Self::State,
///         message: Message,
///         _context: &ProcessingContext,
///     ) -> Result<Option<Message>, Self::Error> {
///         state.counter += 1;
///         Ok(Some(message))
///     }
///
///     fn metadata(&self) -> PluginMetadata {
///         PluginMetadata {
///             name: "MyPlugin".to_string(),
///             version: "1.0.0".to_string(),
///             description: "A simple example plugin".to_string(),
///             author: "Kaelix Team".to_string(),
///             capabilities: PluginCapabilities::default(),
///         }
///     }
/// }
/// ```
#[async_trait]
pub trait Plugin: Send + Sync + Clone + 'static {
    /// Plugin-specific configuration type
    type Config: Send + Sync + 'static;

    /// Plugin-specific state type
    type State: Send + Sync + 'static;

    /// Plugin-specific error type
    type Error: std::error::Error + Send + Sync + 'static + Into<PluginError>;

    /// Initialize the plugin with the provided configuration.
    ///
    /// This method is called once when the plugin is loaded and should
    /// perform any necessary setup operations.
    ///
    /// # Parameters
    /// - `config`: Plugin configuration
    ///
    /// # Returns
    /// - `Ok(state)`: Plugin initialized successfully with initial state
    /// - `Err(error)`: Plugin initialization failed
    async fn initialize(&self, config: Self::Config) -> Result<Self::State, Self::Error>;

    /// Process a message with the plugin's state.
    ///
    /// This is the core message processing method that implements the
    /// plugin's business logic.
    ///
    /// # Parameters
    /// - `state`: Mutable reference to plugin state
    /// - `message`: The message to process
    /// - `context`: Processing context with metadata and utilities
    ///
    /// # Returns
    /// - `Ok(Some(message))`: Message processed, return transformed message
    /// - `Ok(None)`: Message consumed, do not forward
    /// - `Err(error)`: Processing failed
    async fn process_message(
        &self,
        state: &mut Self::State,
        message: Message,
        context: &ProcessingContext,
    ) -> Result<Option<Message>, Self::Error>;

    /// Shutdown the plugin gracefully.
    ///
    /// This method is called when the plugin is being unloaded and should
    /// perform any necessary cleanup operations.
    ///
    /// # Parameters
    /// - `state`: Plugin state to cleanup
    ///
    /// # Returns
    /// - `Ok(())`: Plugin shutdown successfully
    /// - `Err(error)`: Plugin shutdown failed
    async fn shutdown(&self, state: Self::State) -> Result<(), Self::Error> {
        // Default implementation - plugins can override if needed
        drop(state);
        Ok(())
    }

    /// Perform a health check on the plugin.
    ///
    /// This method should return the current health status of the plugin
    /// based on its internal state and any external dependencies.
    ///
    /// # Parameters
    /// - `state`: Reference to plugin state
    ///
    /// # Returns
    /// Plugin health information
    async fn health_check(&self, _state: &Self::State) -> PluginHealth {
        PluginHealth {
            status: HealthStatus::Healthy,
            message: "Plugin is healthy".to_string(),
            timestamp: Utc::now(),
            details: HashMap::new(),
        }
    }

    /// Get plugin metadata.
    ///
    /// This method returns static information about the plugin including
    /// name, version, description, and capabilities.
    ///
    /// # Returns
    /// Plugin metadata
    fn metadata(&self) -> PluginMetadata;

    /// Start the plugin (lifecycle hook).
    ///
    /// Called when the plugin transitions from loaded to running state.
    async fn start(&self, _state: &mut Self::State) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Stop the plugin (lifecycle hook).
    ///
    /// Called when the plugin transitions from running to stopped state.
    async fn stop(&self, _state: &mut Self::State) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Plugin metadata containing information about the plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    /// Plugin name
    pub name: String,

    /// Plugin version
    pub version: String,

    /// Plugin description
    pub description: String,

    /// Plugin author
    pub author: String,

    /// Plugin capabilities
    pub capabilities: PluginCapabilities,
}

/// Message processing result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageProcessingResult {
    /// Message processed successfully, continue with transformed message
    Processed(Message),

    /// Message consumed, do not forward
    Consumed,

    /// Message forwarded without modification
    Forwarded(Message),

    /// Message processing deferred
    Deferred { retry_after: std::time::Duration },
}

/// Message filter trait for plugins that filter messages based on content.
#[async_trait]
pub trait MessageFilter: Send + Sync + 'static {
    /// Filter error type
    type Error: std::error::Error + Send + Sync + 'static;

    /// Filter a message.
    ///
    /// # Parameters
    /// - `message`: The message to filter
    /// - `context`: Processing context
    ///
    /// # Returns
    /// - `Ok(true)`: Message passes filter
    /// - `Ok(false)`: Message filtered out
    /// - `Err(Error)`: Filter error
    async fn filter_message(
        &self,
        message: &Message,
        context: &ProcessingContext,
    ) -> Result<bool, Self::Error>;
}

/// Message transformer trait for plugins that transform messages.
#[async_trait]
pub trait MessageTransformer: Send + Sync + 'static {
    /// Transformer error type
    type Error: std::error::Error + Send + Sync + 'static;

    /// Transform a message.
    ///
    /// # Parameters
    /// - `message`: The message to transform
    /// - `context`: Processing context
    ///
    /// # Returns
    /// - `Ok(Some(message))`: Message transformed
    /// - `Ok(None)`: Message consumed
    /// - `Err(Error)`: Transform error
    async fn transform_message(
        &self,
        message: Message,
        context: &ProcessingContext,
    ) -> Result<Option<Message>, Self::Error>;
}

/// Message processing trait for plugins that process messages without state.
///
/// This is a specialized trait for stateless message processors.
#[async_trait]
pub trait MessageProcessor: Send + Sync + 'static {
    /// Processor error type
    type Error: std::error::Error + Send + Sync + 'static;

    /// Process a message.
    ///
    /// # Parameters
    /// - `message`: The message to process
    /// - `context`: Processing context
    ///
    /// # Returns
    /// - `Ok(result)`: Processing result
    /// - `Err(Error)`: Processing failed
    async fn process_message(
        &self,
        message: &Message,
        context: &ProcessingContext,
    ) -> Result<MessageProcessingResult, Self::Error>;
}

/// Plugin health status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginHealth {
    /// Health status
    pub status: HealthStatus,

    /// Health message
    pub message: String,

    /// Health check timestamp
    pub timestamp: DateTime<Utc>,

    /// Additional health details
    pub details: HashMap<String, String>,
}

/// Health status enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// Plugin is healthy and functioning normally
    Healthy,

    /// Plugin is degraded but still functional
    Degraded,

    /// Plugin is unhealthy and may not function properly
    Unhealthy,

    /// Plugin status is unknown
    Unknown,
}

/// Type-erased plugin object for dynamic dispatch.
///
/// This trait allows plugins to be stored and managed uniformly
/// regardless of their specific type parameters.
pub trait PluginObject: Send + Sync + 'static {
    /// Initialize the plugin with configuration
    async fn initialize_erased(
        &self,
        config: Box<dyn Any + Send + Sync>,
    ) -> Result<Box<dyn Any + Send + Sync>, PluginError>;

    /// Start the plugin
    async fn start_erased(&self, state: &mut Box<dyn Any + Send + Sync>)
        -> Result<(), PluginError>;

    /// Stop the plugin
    async fn stop_erased(&self, state: &mut Box<dyn Any + Send + Sync>) -> Result<(), PluginError>;

    /// Process a message
    async fn process_message_erased(
        &self,
        state: &mut Box<dyn Any + Send + Sync>,
        message: Message,
        context: &ProcessingContext,
    ) -> Result<Option<Message>, PluginError>;

    /// Shutdown the plugin
    async fn shutdown_erased(&self, state: Box<dyn Any + Send + Sync>) -> Result<(), PluginError>;

    /// Health check
    fn health_check_erased(
        &self,
        state: &Box<dyn Any + Send + Sync>,
    ) -> Result<HealthStatus, PluginError>;

    /// Get plugin metrics
    fn metrics_erased(
        &self,
        state: &Box<dyn Any + Send + Sync>,
    ) -> Result<crate::plugin::PluginMetrics, PluginError>;

    /// Get plugin metadata
    fn metadata(&self) -> PluginMetadata;

    /// Clone the plugin object
    fn clone_box(&self) -> Box<dyn PluginObject>;

    /// Get the plugin as Any for downcasting
    fn as_any(&self) -> &dyn Any;
}

/// Blanket implementation of PluginObject for all Plugin types
impl<P> PluginObject for P
where
    P: Plugin + Clone + 'static,
    P::Config: 'static,
    P::State: 'static,
{
    async fn initialize_erased(
        &self,
        config: Box<dyn Any + Send + Sync>,
    ) -> Result<Box<dyn Any + Send + Sync>, PluginError> {
        let config = *config.downcast::<P::Config>().map_err(|_| PluginError::Internal {
            operation: "downcast_config".to_string(),
            reason: "Failed to downcast plugin config".to_string(),
        })?;

        let state = self.initialize(config).await.map_err(|e| e.into())?;
        Ok(Box::new(state))
    }

    async fn start_erased(
        &self,
        state: &mut Box<dyn Any + Send + Sync>,
    ) -> Result<(), PluginError> {
        let state = state.downcast_mut::<P::State>().ok_or_else(|| PluginError::Internal {
            operation: "downcast_state".to_string(),
            reason: "Failed to downcast plugin state".to_string(),
        })?;

        self.start(state).await.map_err(|e| e.into())
    }

    async fn stop_erased(&self, state: &mut Box<dyn Any + Send + Sync>) -> Result<(), PluginError> {
        let state = state.downcast_mut::<P::State>().ok_or_else(|| PluginError::Internal {
            operation: "downcast_state".to_string(),
            reason: "Failed to downcast plugin state".to_string(),
        })?;

        self.stop(state).await.map_err(|e| e.into())
    }

    async fn process_message_erased(
        &self,
        state: &mut Box<dyn Any + Send + Sync>,
        message: Message,
        context: &ProcessingContext,
    ) -> Result<Option<Message>, PluginError> {
        let state = state.downcast_mut::<P::State>().ok_or_else(|| PluginError::Internal {
            operation: "downcast_state".to_string(),
            reason: "Failed to downcast plugin state".to_string(),
        })?;

        self.process_message(state, message, context).await.map_err(|e| e.into())
    }

    async fn shutdown_erased(&self, state: Box<dyn Any + Send + Sync>) -> Result<(), PluginError> {
        let state = *state.downcast::<P::State>().map_err(|_| PluginError::Internal {
            operation: "downcast_state".to_string(),
            reason: "Failed to downcast plugin state for shutdown".to_string(),
        })?;

        self.shutdown(state).await.map_err(|e| e.into())
    }

    fn health_check_erased(
        &self,
        state: &Box<dyn Any + Send + Sync>,
    ) -> Result<HealthStatus, PluginError> {
        let state = state.downcast_ref::<P::State>().ok_or_else(|| PluginError::Internal {
            operation: "downcast_state".to_string(),
            reason: "Failed to downcast plugin state for health check".to_string(),
        })?;

        // Create a future for the health check and block on it
        // Note: This is a simplified approach - in a real implementation,
        // you might want to handle async health checks differently
        let health = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.health_check(state))
        });

        Ok(health.status)
    }

    fn metrics_erased(
        &self,
        _state: &Box<dyn Any + Send + Sync>,
    ) -> Result<crate::plugin::PluginMetrics, PluginError> {
        // Return default metrics - would be implemented based on plugin type
        Ok(crate::plugin::PluginMetrics::default())
    }

    fn metadata(&self) -> PluginMetadata {
        Plugin::metadata(self)
    }

    fn clone_box(&self) -> Box<dyn PluginObject> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Hook event types that plugins can listen for
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HookEvent {
    /// Plugin lifecycle events
    PluginStarted(String),
    PluginStopped(String),
    PluginFailed(String),

    /// Message processing events
    MessageReceived(String),
    MessageProcessed(String),
    MessageFiltered(String),

    /// System events
    SystemStartup,
    SystemShutdown,
    HealthCheckStarted,
    HealthCheckCompleted,

    /// Custom events
    Custom(String, HashMap<String, String>),
}

impl HookEvent {
    /// Get the event name as a string
    pub fn name(&self) -> &str {
        match self {
            HookEvent::PluginStarted(_) => "plugin_started",
            HookEvent::PluginStopped(_) => "plugin_stopped",
            HookEvent::PluginFailed(_) => "plugin_failed",
            HookEvent::MessageReceived(_) => "message_received",
            HookEvent::MessageProcessed(_) => "message_processed",
            HookEvent::MessageFiltered(_) => "message_filtered",
            HookEvent::SystemStartup => "system_startup",
            HookEvent::SystemShutdown => "system_shutdown",
            HookEvent::HealthCheckStarted => "health_check_started",
            HookEvent::HealthCheckCompleted => "health_check_completed",
            HookEvent::Custom(name, _) => name,
        }
    }

    /// Check if this event matches a pattern
    pub fn matches(&self, pattern: &str) -> bool {
        self.name() == pattern || pattern == "*"
    }
}

/// Plugin hook trait for event-driven plugin interactions
#[async_trait]
pub trait PluginHook: Send + Sync + 'static {
    /// Hook error type
    type Error: std::error::Error + Send + Sync + 'static;

    /// Handle a hook event
    ///
    /// # Parameters
    /// - `event`: The event that occurred
    /// - `context`: Processing context
    ///
    /// # Returns
    /// - `Ok(())`: Hook handled successfully
    /// - `Err(Error)`: Hook handling failed
    async fn handle_event(
        &self,
        event: HookEvent,
        context: &ProcessingContext,
    ) -> Result<(), Self::Error>;

    /// Get the events this hook is interested in
    fn interested_events(&self) -> Vec<String>;

    /// Get hook priority (higher priority hooks run first)
    fn priority(&self) -> i32 {
        0
    }

    /// Get hook name for debugging
    fn name(&self) -> &str;
}
