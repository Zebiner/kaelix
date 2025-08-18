//! Core plugin trait definitions and interfaces.

use crate::message::Message;
use crate::plugin::{PluginCapabilities, PluginError, ProcessingContext};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;
use chrono::{DateTime, Utc};

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
///     fn name(&self) -> &str { "my-plugin" }
///     fn version(&self) -> semver::Version { semver::Version::new(1, 0, 0) }
///     fn description(&self) -> &str { "Example plugin" }
///     
///     fn capabilities(&self) -> PluginCapabilities {
///         PluginCapabilities::default()
///     }
///     
///     fn validate_config(&self, config: &Self::Config) -> Result<(), Self::Error> {
///         if config.name.is_empty() {
///             return Err(MyPluginError::ProcessingFailed {
///                 reason: "Name cannot be empty".to_string(),
///             });
///         }
///         Ok(())
///     }
///
///     async fn initialize(&self, config: Self::Config) -> Result<Self::State, Self::Error> {
///         Ok(MyPluginState { counter: 0 })
///     }
///
///     async fn process_message(
///         &self,
///         state: &mut Self::State,
///         message: &Message,
///         context: &ProcessingContext,
///     ) -> Result<MessageProcessingResult, Self::Error> {
///         state.counter += 1;
///         Ok(MessageProcessingResult::Continue)
///     }
///     
///     async fn shutdown(&self, state: Self::State) -> Result<(), Self::Error> {
///         Ok(())
///     }
/// }
/// ```
#[async_trait]
pub trait Plugin: Send + Sync + 'static + Clone {
    /// Plugin-specific configuration type
    type Config: Send + Sync + 'static;

    /// Plugin-specific state type  
    type State: Send + Sync + 'static;

    /// Plugin-specific error type
    type Error: std::error::Error + Send + Sync + 'static;

    /// Get the plugin name (unique identifier).
    fn name(&self) -> &str;

    /// Get the plugin version.
    fn version(&self) -> semver::Version;

    /// Get the plugin description.
    fn description(&self) -> &str;

    /// Get the plugin capabilities and permissions.
    fn capabilities(&self) -> PluginCapabilities;

    /// Validate plugin configuration before initialization.
    ///
    /// This method is called before `initialize` to ensure the configuration
    /// is valid and the plugin can be started successfully.
    ///
    /// # Parameters
    /// - `config`: The configuration to validate
    ///
    /// # Returns
    /// - `Ok(())`: Configuration is valid
    /// - `Err(Error)`: Configuration is invalid with description
    fn validate_config(&self, config: &Self::Config) -> Result<(), Self::Error>;

    /// Initialize the plugin with the given configuration.
    ///
    /// This method is called once when the plugin is first loaded.
    /// It should set up any necessary resources and return the initial state.
    ///
    /// # Parameters
    /// - `config`: Plugin configuration
    ///
    /// # Returns
    /// - `Ok(State)`: Plugin initialized successfully with initial state
    /// - `Err(Error)`: Initialization failed
    async fn initialize(&self, config: Self::Config) -> Result<Self::State, Self::Error>;

    /// Process a message through this plugin.
    ///
    /// This is the main entry point for message processing. The plugin
    /// can modify the message, update its state, and decide how the
    /// processing pipeline should continue.
    ///
    /// # Parameters
    /// - `state`: Mutable reference to plugin state
    /// - `message`: The message to process
    /// - `context`: Processing context with metadata and utilities
    ///
    /// # Returns
    /// - `Ok(MessageProcessingResult)`: Processing result
    /// - `Err(Error)`: Processing failed
    async fn process_message(
        &self,
        state: &mut Self::State,
        message: &Message,
        context: &ProcessingContext,
    ) -> Result<MessageProcessingResult, Self::Error>;

    /// Shutdown the plugin gracefully.
    ///
    /// This method is called when the plugin is being unloaded.
    /// It should clean up any resources and perform final operations.
    ///
    /// # Parameters
    /// - `state`: Final plugin state
    ///
    /// # Returns
    /// - `Ok(())`: Shutdown completed successfully
    /// - `Err(Error)`: Shutdown failed
    async fn shutdown(&self, state: Self::State) -> Result<(), Self::Error>;

    /// Perform a health check on the plugin.
    ///
    /// This method is called periodically to check if the plugin
    /// is functioning correctly.
    ///
    /// # Parameters
    /// - `state`: Current plugin state
    ///
    /// # Returns
    /// - `PluginHealth`: Current health status
    async fn health_check(&self, _state: &Self::State) -> PluginHealth {
        PluginHealth {
            status: HealthStatus::Healthy,
            message: "Plugin is healthy".to_string(),
            timestamp: Utc::now(),
            details: HashMap::new(),
        }
    }

    /// Get plugin metadata for monitoring and management.
    ///
    /// # Returns
    /// - `PluginMetadata`: Static plugin information
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: self.name().to_string(),
            version: self.version(),
            description: self.description().to_string(),
            capabilities: self.capabilities(),
            dependencies: Vec::new(),
            tags: Vec::new(),
        }
    }
}

/// Type-erased plugin object for dynamic plugin management.
///
/// This trait allows plugins with different type parameters to be
/// stored and managed uniformly.
#[async_trait]
pub trait PluginObject: Send + Sync + 'static {
    /// Get the plugin name.
    fn name(&self) -> &str;

    /// Get the plugin version.
    fn version(&self) -> semver::Version;

    /// Get the plugin description.
    fn description(&self) -> &str;

    /// Get the plugin capabilities.
    fn capabilities(&self) -> PluginCapabilities;

    /// Initialize the plugin with raw configuration data.
    async fn initialize_raw(&self, config_data: &[u8]) -> Result<Box<dyn Any + Send + Sync>, PluginError>;

    /// Process a message with type-erased state.
    async fn process_message_erased(
        &self,
        state: &mut Box<dyn Any + Send + Sync>,
        message: &Message,
        context: &ProcessingContext,
    ) -> Result<MessageProcessingResult, PluginError>;

    /// Shutdown the plugin with type-erased state.
    async fn shutdown_erased(&self, state: Box<dyn Any + Send + Sync>) -> Result<(), PluginError>;

    /// Check plugin health with type-erased state.
    fn health_check_erased(&self, state: &Box<dyn Any + Send + Sync>) -> Result<HealthStatus, PluginError>;

    /// Get plugin metrics with type-erased state.
    fn metrics_erased(&self, state: &Box<dyn Any + Send + Sync>) -> Result<crate::plugin::PluginMetrics, PluginError>;

    /// Get plugin metadata.
    fn metadata(&self) -> PluginMetadata;

    /// Clone the plugin object into a new box.
    fn clone_box(&self) -> Box<dyn PluginObject>;

    /// Convert to Any for downcasting.
    fn as_any(&self) -> &dyn Any;
}

/// Implement Clone for Box<dyn PluginObject> using the clone_box method
impl Clone for Box<dyn PluginObject> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// Message processing result enumeration.
///
/// Indicates how the message processing pipeline should proceed
/// after a plugin has processed a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageProcessingResult {
    /// Continue processing with the next plugin in the chain
    Continue,

    /// Stop processing and accept the message
    Accept,

    /// Stop processing and reject the message
    Reject {
        /// Reason for rejection
        reason: String,
    },

    /// Transform the message and continue processing
    Transform {
        /// New message to process
        message: Message,
    },

    /// Split the message into multiple messages
    Split {
        /// New messages to process
        messages: Vec<Message>,
    },

    /// Route the message to a specific topic/partition
    Route {
        /// Target topic
        topic: String,
        /// Optional target partition
        partition: Option<u32>,
    },
}

/// Message filtering trait for plugins that only filter messages.
///
/// This is a specialized trait for plugins that only need to decide
/// whether messages should be accepted or rejected without transformation.
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
    /// - `Ok(true)`: Message should be accepted
    /// - `Ok(false)`: Message should be rejected
    /// - `Err(Error)`: Filtering failed
    async fn filter_message(
        &self,
        message: &Message,
        context: &ProcessingContext,
    ) -> Result<bool, Self::Error>;
}

/// Message transformation trait for plugins that transform messages.
///
/// This is a specialized trait for plugins that transform messages
/// without complex state management.
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
    /// - `Ok(Some(message))`: Message was transformed
    /// - `Ok(None)`: Message should be filtered out
    /// - `Err(Error)`: Transformation failed
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

    /// Plugin has minor issues but is still functional
    Warning,

    /// Plugin has critical issues and may not be functioning
    Critical,

    /// Plugin is in an unknown state
    Unknown,
}

/// Plugin metadata for registration and management.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    /// Plugin name (unique identifier)
    pub name: String,

    /// Plugin version
    pub version: semver::Version,

    /// Plugin description
    pub description: String,

    /// Plugin capabilities
    pub capabilities: PluginCapabilities,

    /// Plugin dependencies
    pub dependencies: Vec<String>,

    /// Plugin tags for categorization
    pub tags: Vec<String>,
}

impl PluginMetadata {
    /// Validate the plugin metadata.
    pub fn validate(&self) -> Result<(), PluginError> {
        if self.name.is_empty() {
            return Err(PluginError::ValidationError {
                plugin_id: "unknown".to_string(),
                field: "name".to_string(),
                reason: "Plugin name cannot be empty".to_string(),
            });
        }

        if self.description.is_empty() {
            return Err(PluginError::ValidationError {
                plugin_id: self.name.clone(),
                field: "description".to_string(),
                reason: "Plugin description cannot be empty".to_string(),
            });
        }

        Ok(())
    }
}

/// Automatic implementation of PluginObject for types that implement Plugin.
#[async_trait]
impl<P> PluginObject for P
where
    P: Plugin + Clone,
    P::Config: for<'de> Deserialize<'de>,
    P::State: 'static,
    P::Error: Into<PluginError>,
{
    fn name(&self) -> &str {
        Plugin::name(self)
    }

    fn version(&self) -> semver::Version {
        Plugin::version(self)
    }

    fn description(&self) -> &str {
        Plugin::description(self)
    }

    fn capabilities(&self) -> PluginCapabilities {
        Plugin::capabilities(self)
    }

    async fn initialize_raw(&self, config_data: &[u8]) -> Result<Box<dyn Any + Send + Sync>, PluginError> {
        let config: P::Config = serde_json::from_slice(config_data)
            .map_err(|e| PluginError::SerializationError {
                plugin_id: self.name().to_string(),
                operation: "deserialize_config".to_string(),
                reason: e.to_string(),
            })?;

        let state = self.initialize(config).await.map_err(|e| e.into())?;
        Ok(Box::new(state))
    }

    async fn process_message_erased(
        &self,
        state: &mut Box<dyn Any + Send + Sync>,
        message: &Message,
        context: &ProcessingContext,
    ) -> Result<MessageProcessingResult, PluginError> {
        let state = state
            .downcast_mut::<P::State>()
            .ok_or_else(|| PluginError::Internal {
                operation: "downcast_state".to_string(),
                reason: "Failed to downcast plugin state".to_string(),
            })?;

        self.process_message(state, message, context)
            .await
            .map_err(|e| e.into())
    }

    async fn shutdown_erased(&self, state: Box<dyn Any + Send + Sync>) -> Result<(), PluginError> {
        let state = *state
            .downcast::<P::State>()
            .map_err(|_| PluginError::Internal {
                operation: "downcast_state".to_string(),
                reason: "Failed to downcast plugin state for shutdown".to_string(),
            })?;

        self.shutdown(state).await.map_err(|e| e.into())
    }

    fn health_check_erased(&self, state: &Box<dyn Any + Send + Sync>) -> Result<HealthStatus, PluginError> {
        let state = state
            .downcast_ref::<P::State>()
            .ok_or_else(|| PluginError::Internal {
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

    fn metrics_erased(&self, _state: &Box<dyn Any + Send + Sync>) -> Result<crate::plugin::PluginMetrics, PluginError> {
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