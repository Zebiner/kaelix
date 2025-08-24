//! Plugin registry system for managing and coordinating plugins in Kaelix.
//!
//! The registry provides centralized plugin management, dependency resolution,
//! lifecycle coordination, and security policy enforcement.

use crate::{
    error::{Error, Result},
    plugin::{
        capabilities::PluginCapabilities,
        context::ProcessingContext,
        loader::{LoaderConfig, PluginLoader},
        security::{SecurityContext, SecurityPolicy, ThreatLevel},
        traits::{Plugin, PluginMetadata},
    },
};

use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{
    sync::{mpsc, RwLock as AsyncRwLock},
    time::timeout,
};
use tracing::{error, info, warn};
use uuid::Uuid;

/// Unique identifier for plugins
pub type PluginId = Uuid;

/// Registry event types for monitoring and auditing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RegistryEvent {
    /// Plugin was registered
    PluginRegistered { plugin_id: PluginId, metadata: PluginMetadata, timestamp: Instant },
    /// Plugin was unregistered
    PluginUnregistered { plugin_id: PluginId, timestamp: Instant },
    /// Plugin state changed
    PluginStateChanged {
        plugin_id: PluginId,
        old_state: PluginState,
        new_state: PluginState,
        timestamp: Instant,
    },
    /// Plugin error occurred
    PluginError { plugin_id: PluginId, error: String, timestamp: Instant },
    /// Security violation detected
    SecurityViolation {
        plugin_id: PluginId,
        violation: String,
        threat_level: ThreatLevel,
        timestamp: Instant,
    },
}

/// Plugin runtime state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginState {
    /// Plugin is registered but not initialized
    Registered,
    /// Plugin is initializing
    Initializing,
    /// Plugin is active and processing
    Active,
    /// Plugin is paused
    Paused,
    /// Plugin is stopping
    Stopping,
    /// Plugin has stopped
    Stopped,
    /// Plugin is in error state
    Error,
}

impl Default for PluginState {
    fn default() -> Self {
        Self::Registered
    }
}

/// Plugin registry entry containing all plugin information
#[derive(Debug)]
pub struct PluginEntry {
    /// Plugin ID
    pub id: PluginId,
    /// Plugin instance
    pub plugin: Arc<dyn Plugin>,
    /// Plugin metadata
    pub metadata: PluginMetadata,
    /// Current state
    pub state: Arc<RwLock<PluginState>>,
    /// Plugin capabilities
    pub capabilities: PluginCapabilities,
    /// Security context
    pub security_context: SecurityContext,
    /// Registration timestamp
    pub registered_at: Instant,
    /// Last activity timestamp
    pub last_activity: Arc<RwLock<Instant>>,
    /// Error count
    pub error_count: Arc<AtomicU64>,
    /// Active processing contexts
    pub active_contexts: Arc<AsyncRwLock<HashMap<Uuid, ProcessingContext>>>,
}

impl PluginEntry {
    /// Create a new plugin entry
    pub fn new(
        plugin: Arc<dyn Plugin>,
        metadata: PluginMetadata,
        capabilities: PluginCapabilities,
        security_context: SecurityContext,
    ) -> Self {
        let now = Instant::now();
        Self {
            id: Uuid::new_v4(),
            plugin,
            metadata,
            state: Arc::new(RwLock::new(PluginState::Registered)),
            capabilities,
            security_context,
            registered_at: now,
            last_activity: Arc::new(RwLock::new(now)),
            error_count: Arc::new(AtomicU64::new(0)),
            active_contexts: Arc::new(AsyncRwLock::new(HashMap::new())),
        }
    }

    /// Update the plugin state
    pub fn set_state(&self, new_state: PluginState) -> PluginState {
        let old_state = {
            let mut state = self.state.write();
            let old = *state;
            *state = new_state;
            old
        };
        self.update_activity();
        old_state
    }

    /// Get current plugin state
    pub fn get_state(&self) -> PluginState {
        *self.state.read()
    }

    /// Update last activity timestamp
    pub fn update_activity(&self) {
        let mut last_activity = self.last_activity.write();
        *last_activity = Instant::now();
    }

    /// Increment error count
    pub fn increment_error_count(&self) -> u64 {
        self.error_count.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Get current error count
    pub fn get_error_count(&self) -> u64 {
        self.error_count.load(Ordering::Relaxed)
    }
}

/// Plugin registry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryConfig {
    /// Maximum number of plugins
    pub max_plugins: usize,
    /// Plugin initialization timeout
    pub init_timeout: Duration,
    /// Plugin shutdown timeout
    pub shutdown_timeout: Duration,
    /// Event buffer size
    pub event_buffer_size: usize,
    /// Security policy enforcement
    pub enforce_security: bool,
    /// Plugin hot-reload support
    pub enable_hot_reload: bool,
    /// Health check interval
    pub health_check_interval: Duration,
    /// Maximum error threshold per plugin
    pub max_plugin_errors: u64,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            max_plugins: 1000,
            init_timeout: Duration::from_secs(30),
            shutdown_timeout: Duration::from_secs(10),
            event_buffer_size: 10000,
            enforce_security: true,
            enable_hot_reload: false,
            health_check_interval: Duration::from_secs(60),
            max_plugin_errors: 10,
        }
    }
}

/// Plugin dependency graph for resolution
#[derive(Debug, Default)]
pub struct DependencyGraph {
    /// Dependencies map (plugin_id -> list of dependencies)
    dependencies: HashMap<PluginId, Vec<PluginId>>,
    /// Reverse dependencies map (plugin_id -> list of dependents)
    dependents: HashMap<PluginId, Vec<PluginId>>,
}

impl DependencyGraph {
    /// Add a dependency relationship
    pub fn add_dependency(&mut self, plugin_id: PluginId, dependency_id: PluginId) {
        self.dependencies.entry(plugin_id).or_insert_with(Vec::new).push(dependency_id);

        self.dependents.entry(dependency_id).or_insert_with(Vec::new).push(plugin_id);
    }

    /// Get dependencies for a plugin
    pub fn get_dependencies(&self, plugin_id: &PluginId) -> Vec<PluginId> {
        self.dependencies.get(plugin_id).cloned().unwrap_or_default()
    }

    /// Get dependents for a plugin
    pub fn get_dependents(&self, plugin_id: &PluginId) -> Vec<PluginId> {
        self.dependents.get(plugin_id).cloned().unwrap_or_default()
    }

    /// Check for circular dependencies
    pub fn has_cycles(&self) -> bool {
        // Simplified cycle detection - in production would use proper DFS
        false
    }

    /// Get startup order based on dependencies
    pub fn get_startup_order(&self) -> Vec<PluginId> {
        // Simplified topological sort - in production would use proper algorithm
        self.dependencies.keys().copied().collect()
    }
}

/// Plugin registry statistics
#[derive(Debug, Clone, Default)]
pub struct RegistryStats {
    /// Total registered plugins
    pub total_plugins: usize,
    /// Active plugins
    pub active_plugins: usize,
    /// Failed plugins
    pub failed_plugins: usize,
    /// Total events processed
    pub total_events: u64,
    /// Security violations detected
    pub security_violations: u64,
    /// Average plugin initialization time
    pub avg_init_time: Duration,
}

/// Main plugin registry
pub struct PluginRegistry {
    /// Configuration
    config: RegistryConfig,
    /// Plugin entries
    plugins: DashMap<PluginId, Arc<PluginEntry>>,
    /// Plugin name to ID mapping
    name_to_id: DashMap<String, PluginId>,
    /// Plugin loader
    loader: Arc<PluginLoader>,
    /// Security policy
    security_policy: Arc<SecurityPolicy>,
    /// Dependency graph
    dependency_graph: Arc<RwLock<DependencyGraph>>,
    /// Event channel
    event_tx: mpsc::UnboundedSender<RegistryEvent>,
    event_rx: Arc<AsyncRwLock<Option<mpsc::UnboundedReceiver<RegistryEvent>>>>,
    /// Registry statistics
    stats: Arc<RwLock<RegistryStats>>,
    /// Running state
    is_running: Arc<AtomicBool>,
}

impl PluginRegistry {
    /// Create a new plugin registry
    pub fn new(config: RegistryConfig) -> Result<Self> {
        let loader_config = LoaderConfig::default();
        let loader = Arc::new(PluginLoader::new(loader_config)?);
        let security_policy = Arc::new(SecurityPolicy::default());

        let (event_tx, event_rx) = mpsc::unbounded_channel();

        Ok(Self {
            config,
            plugins: DashMap::new(),
            name_to_id: DashMap::new(),
            loader,
            security_policy,
            dependency_graph: Arc::new(RwLock::new(DependencyGraph::default())),
            event_tx,
            event_rx: Arc::new(AsyncRwLock::new(Some(event_rx))),
            stats: Arc::new(RwLock::new(RegistryStats::default())),
            is_running: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Start the plugin registry
    pub async fn start(&self) -> Result<()> {
        if self.is_running.load(Ordering::Relaxed) {
            return Err(Error::Configuration("Plugin registry is already running".to_string()));
        }

        info!("Starting plugin registry with {} max plugins", self.config.max_plugins);
        self.is_running.store(true, Ordering::Relaxed);

        // Start event processing
        self.start_event_processing().await?;

        // Start health monitoring
        if self.config.health_check_interval > Duration::ZERO {
            self.start_health_monitoring().await?;
        }

        Ok(())
    }

    /// Stop the plugin registry
    pub async fn stop(&self) -> Result<()> {
        if !self.is_running.load(Ordering::Relaxed) {
            return Ok(());
        }

        info!("Stopping plugin registry");
        self.is_running.store(false, Ordering::Relaxed);

        // Stop all plugins
        self.stop_all_plugins().await?;

        Ok(())
    }

    /// Register a plugin from a file path
    pub async fn register_plugin_from_path(&self, plugin_path: PathBuf) -> Result<PluginId> {
        if self.plugins.len() >= self.config.max_plugins {
            return Err(Error::Configuration("Maximum number of plugins reached".to_string()));
        }

        // Load plugin using the loader
        let (plugin, metadata, capabilities) =
            timeout(self.config.init_timeout, self.loader.load_plugin(plugin_path.clone()))
                .await
                .map_err(|_| Error::Configuration("Plugin load timeout".to_string()))??;

        // Create security context
        let security_context = self.security_policy.create_context_for_plugin(&metadata)?;

        // Create plugin entry
        let entry =
            Arc::new(PluginEntry::new(plugin, metadata.clone(), capabilities, security_context));

        let plugin_id = entry.id;

        // Check for name conflicts
        if self.name_to_id.contains_key(&metadata.name) {
            return Err(Error::Configuration(format!(
                "Plugin with name '{}' is already registered",
                metadata.name
            )));
        }

        // Register plugin
        self.plugins.insert(plugin_id, entry.clone());
        self.name_to_id.insert(metadata.name.clone(), plugin_id);

        // Update statistics
        {
            let mut stats = self.stats.write();
            stats.total_plugins += 1;
        }

        // Send registration event
        let event = RegistryEvent::PluginRegistered {
            plugin_id,
            metadata: metadata.clone(),
            timestamp: Instant::now(),
        };

        if let Err(e) = self.event_tx.send(event) {
            warn!("Failed to send plugin registration event: {}", e);
        }

        info!("Registered plugin '{}' with ID {}", metadata.name, plugin_id);
        Ok(plugin_id)
    }

    /// Unregister a plugin
    pub async fn unregister_plugin(&self, plugin_id: PluginId) -> Result<()> {
        let entry = self
            .plugins
            .get(&plugin_id)
            .ok_or_else(|| Error::Configuration(format!("Plugin {} not found", plugin_id)))?
            .clone();

        // Stop plugin if running
        if entry.get_state() == PluginState::Active {
            self.stop_plugin(plugin_id).await?;
        }

        // Remove from registries
        self.plugins.remove(&plugin_id);
        self.name_to_id.remove(&entry.metadata.name);

        // Update dependency graph
        {
            let mut graph = self.dependency_graph.write();
            // Remove dependencies - simplified for now
        }

        // Update statistics
        {
            let mut stats = self.stats.write();
            stats.total_plugins = stats.total_plugins.saturating_sub(1);
        }

        // Send unregistration event
        let event = RegistryEvent::PluginUnregistered { plugin_id, timestamp: Instant::now() };

        if let Err(e) = self.event_tx.send(event) {
            warn!("Failed to send plugin unregistration event: {}", e);
        }

        info!("Unregistered plugin with ID {}", plugin_id);
        Ok(())
    }

    /// Start a plugin
    pub async fn start_plugin(&self, plugin_id: PluginId) -> Result<()> {
        let entry = self
            .plugins
            .get(&plugin_id)
            .ok_or_else(|| Error::Configuration(format!("Plugin {} not found", plugin_id)))?
            .clone();

        let current_state = entry.get_state();
        if current_state == PluginState::Active {
            return Ok(()); // Already active
        }

        // Set initializing state
        entry.set_state(PluginState::Initializing);

        // Initialize plugin with timeout
        let result = timeout(self.config.init_timeout, entry.plugin.initialize()).await;

        match result {
            Ok(Ok(())) => {
                entry.set_state(PluginState::Active);

                // Update statistics
                {
                    let mut stats = self.stats.write();
                    stats.active_plugins += 1;
                }

                info!("Started plugin '{}'", entry.metadata.name);
                Ok(())
            },
            Ok(Err(e)) => {
                entry.set_state(PluginState::Error);
                entry.increment_error_count();
                Err(e)
            },
            Err(_) => {
                entry.set_state(PluginState::Error);
                entry.increment_error_count();
                Err(Error::Configuration("Plugin initialization timeout".to_string()))
            },
        }
    }

    /// Stop a plugin
    pub async fn stop_plugin(&self, plugin_id: PluginId) -> Result<()> {
        let entry = self
            .plugins
            .get(&plugin_id)
            .ok_or_else(|| Error::Configuration(format!("Plugin {} not found", plugin_id)))?
            .clone();

        let current_state = entry.get_state();
        if current_state == PluginState::Stopped {
            return Ok(()); // Already stopped
        }

        // Set stopping state
        entry.set_state(PluginState::Stopping);

        // Shutdown plugin with timeout
        let result = timeout(self.config.shutdown_timeout, entry.plugin.shutdown()).await;

        match result {
            Ok(Ok(())) => {
                entry.set_state(PluginState::Stopped);

                // Update statistics
                {
                    let mut stats = self.stats.write();
                    stats.active_plugins = stats.active_plugins.saturating_sub(1);
                }

                info!("Stopped plugin '{}'", entry.metadata.name);
                Ok(())
            },
            Ok(Err(e)) => {
                entry.set_state(PluginState::Error);
                entry.increment_error_count();
                Err(e)
            },
            Err(_) => {
                entry.set_state(PluginState::Error);
                entry.increment_error_count();
                Err(Error::Configuration("Plugin shutdown timeout".to_string()))
            },
        }
    }

    /// Get plugin by ID
    pub fn get_plugin(&self, plugin_id: PluginId) -> Option<Arc<PluginEntry>> {
        self.plugins.get(&plugin_id).map(|entry| entry.clone())
    }

    /// Get plugin by name
    pub fn get_plugin_by_name(&self, name: &str) -> Option<Arc<PluginEntry>> {
        self.name_to_id
            .get(name)
            .and_then(|id| self.plugins.get(&id))
            .map(|entry| entry.clone())
    }

    /// List all plugins
    pub fn list_plugins(&self) -> Vec<Arc<PluginEntry>> {
        self.plugins.iter().map(|entry| entry.value().clone()).collect()
    }

    /// Get registry statistics
    pub fn get_stats(&self) -> RegistryStats {
        let stats = self.stats.read().clone();
        RegistryStats {
            total_plugins: self.plugins.len(),
            active_plugins: self
                .plugins
                .iter()
                .filter(|entry| entry.get_state() == PluginState::Active)
                .count(),
            failed_plugins: self
                .plugins
                .iter()
                .filter(|entry| entry.get_state() == PluginState::Error)
                .count(),
            ..stats
        }
    }

    /// Start event processing loop
    async fn start_event_processing(&self) -> Result<()> {
        // Implementation would start background task to process events
        Ok(())
    }

    /// Start health monitoring loop
    async fn start_health_monitoring(&self) -> Result<()> {
        // Implementation would start background health checks
        Ok(())
    }

    /// Stop all plugins
    async fn stop_all_plugins(&self) -> Result<()> {
        let plugin_ids: Vec<PluginId> =
            self.plugins.iter().map(|entry| entry.key().clone()).collect();

        for plugin_id in plugin_ids {
            if let Err(e) = self.stop_plugin(plugin_id).await {
                error!("Failed to stop plugin {}: {}", plugin_id, e);
            }
        }

        Ok(())
    }
}

impl Drop for PluginRegistry {
    fn drop(&mut self) {
        // Ensure proper cleanup
        self.is_running.store(false, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_registry_creation() {
        let config = RegistryConfig::default();
        let registry = PluginRegistry::new(config);
        assert!(registry.is_ok());
    }

    #[tokio::test]
    async fn test_registry_start_stop() {
        let config = RegistryConfig::default();
        let registry = PluginRegistry::new(config).unwrap();

        assert!(registry.start().await.is_ok());
        assert!(registry.is_running.load(Ordering::Relaxed));

        assert!(registry.stop().await.is_ok());
        assert!(!registry.is_running.load(Ordering::Relaxed));
    }

    #[test]
    fn test_dependency_graph() {
        let mut graph = DependencyGraph::default();
        let plugin1 = Uuid::new_v4();
        let plugin2 = Uuid::new_v4();

        graph.add_dependency(plugin1, plugin2);

        assert_eq!(graph.get_dependencies(&plugin1), vec![plugin2]);
        assert_eq!(graph.get_dependents(&plugin2), vec![plugin1]);
    }
}
