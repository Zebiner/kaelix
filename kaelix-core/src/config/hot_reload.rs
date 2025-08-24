//! Hot reload manager for configuration changes
//!
//! Provides infrastructure for monitoring and reloading configurations dynamically

use crate::{
    config::{loader::ConfigLoader, schema::MemoryStreamerConfig},
    Error, Result,
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
};
use tokio::sync::{broadcast, Mutex, RwLock};
use tracing::info;

/// Configuration change notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigChange {
    /// The new configuration
    pub new_config: MemoryStreamerConfig,
    /// Timestamp of the change
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Change sequence number
    pub sequence: u64,
    /// Optional description of what changed
    pub description: Option<String>,
}

/// Statistics for hot reload operations
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HotReloadStats {
    /// Total number of reload attempts
    pub total_reloads: u64,
    /// Number of successful reloads
    pub successful_reloads: u64,
    /// Number of failed reloads
    pub failed_reloads: u64,
    /// Last successful reload timestamp
    pub last_successful_reload: Option<chrono::DateTime<chrono::Utc>>,
    /// Last failed reload timestamp
    pub last_failed_reload: Option<chrono::DateTime<chrono::Utc>>,
    /// Average reload time in milliseconds
    pub average_reload_time_ms: f64,
}

/// Hot reload settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotReloadSettings {
    /// Debounce delay in milliseconds to avoid rapid-fire reloads
    pub debounce_delay_ms: u64,
    /// Whether to validate configuration before applying
    pub validate_before_apply: bool,
    /// Whether to backup old configuration before applying new one
    pub backup_on_change: bool,
    /// Maximum number of backup files to keep
    pub max_backups: u32,
}

impl Default for HotReloadSettings {
    fn default() -> Self {
        Self {
            debounce_delay_ms: 1000, // 1 second debounce
            validate_before_apply: true,
            backup_on_change: true,
            max_backups: 10,
        }
    }
}

/// Hot reload manager for configuration changes
pub struct HotReloadManager {
    /// Current configuration
    current_config: Arc<RwLock<MemoryStreamerConfig>>,
    /// Configuration file path being monitored
    config_path: PathBuf,
    /// Change notification sender
    change_tx: broadcast::Sender<ConfigChange>,
    /// Configuration loader
    loader: ConfigLoader,
    /// Hot reload statistics
    stats: Arc<Mutex<HotReloadStats>>,
    /// Hot reload settings
    settings: HotReloadSettings,
    /// Sequence counter for changes
    sequence: AtomicU64,
    /// Whether the manager is active
    active: AtomicBool,
}

impl HotReloadManager {
    /// Create a new hot reload manager
    pub async fn new<P: AsRef<Path>>(
        config_path: P,
        initial_config: MemoryStreamerConfig,
        settings: Option<HotReloadSettings>,
    ) -> Result<Self> {
        let config_path = config_path.as_ref().to_path_buf();
        let settings = settings.unwrap_or_default();

        // Validate the config file exists
        if !config_path.exists() {
            return Err(Error::Configuration(format!(
                "Configuration file does not exist: {}",
                config_path.display()
            )));
        }

        // Create change notification channel
        let (change_tx, _) = broadcast::channel(100);

        // Initialize shared state
        let current_config = Arc::new(RwLock::new(initial_config));
        let stats = Arc::new(Mutex::new(HotReloadStats::default()));
        let sequence = AtomicU64::new(0);
        let active = AtomicBool::new(true);

        let loader = ConfigLoader::new();

        Ok(Self {
            current_config,
            config_path,
            change_tx,
            loader,
            stats,
            settings,
            sequence,
            active,
        })
    }

    /// Subscribe to configuration change notifications
    pub fn subscribe(&self) -> broadcast::Receiver<ConfigChange> {
        self.change_tx.subscribe()
    }

    /// Get the current configuration
    pub async fn current_config(&self) -> MemoryStreamerConfig {
        self.current_config.read().await.clone()
    }

    /// Stop the hot reload manager
    pub fn stop(&self) {
        self.active.store(false, Ordering::Relaxed);
        info!("Hot reload manager stopped");
    }
}

impl Drop for HotReloadManager {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Configuration watcher convenience type
pub type ConfigWatcher = HotReloadManager;
