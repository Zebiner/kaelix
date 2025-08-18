//! Hot-reload configuration system with file watching
//!
//! This module provides runtime configuration updates without service restart.
//! It includes:
//! - File system watching for configuration changes
//! - Safe configuration updates with validation
//! - Rollback mechanism for invalid configurations
//! - Event notification system for configuration changes
//! - Thread-safe configuration access

use crate::config::loader::ConfigLoader;
use crate::config::schema::MemoryStreamerConfig;
use crate::config::validator::ConfigValidator;
use crate::error::{Error, Result};
use crossbeam::channel::{Receiver, Sender, TryRecvError};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::RwLock;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

/// Hot-reload configuration manager
///
/// Manages runtime configuration updates with file watching, validation,
/// and safe rollback capabilities. Provides thread-safe access to the
/// current configuration and notifies subscribers of changes.
pub struct HotReloadManager {
    /// Current configuration (thread-safe)
    current_config: Arc<RwLock<MemoryStreamerConfig>>,
    /// Configuration file path being watched
    config_path: PathBuf,
    /// Configuration loader for reloading
    loader: ConfigLoader,
    /// Configuration validator
    validator: ConfigValidator,
    /// File system watcher
    watcher: Option<RecommendedWatcher>,
    /// Channel for file system events
    fs_event_tx: Option<Sender<notify::Result<Event>>>,
    fs_event_rx: Option<Receiver<notify::Result<Event>>>,
    /// Broadcast channel for configuration change notifications
    change_tx: broadcast::Sender<ConfigChangeEvent>,
    /// Last successful configuration (for rollback)
    last_valid_config: Arc<RwLock<MemoryStreamerConfig>>,
    /// Reload statistics
    stats: Arc<RwLock<ReloadStats>>,
    /// Hot-reload settings
    settings: HotReloadSettings,
}

/// Configuration change event
#[derive(Debug, Clone)]
pub struct ConfigChangeEvent {
    /// Timestamp of the change
    pub timestamp: Instant,
    /// Type of change that occurred
    pub change_type: ChangeType,
    /// Path to the configuration file that changed
    pub file_path: PathBuf,
    /// Whether the reload was successful
    pub success: bool,
    /// Error message if reload failed
    pub error: Option<String>,
}

/// Type of configuration change
#[derive(Debug, Clone, PartialEq)]
pub enum ChangeType {
    /// Configuration file was modified
    FileModified,
    /// Configuration file was created
    FileCreated,
    /// Configuration file was deleted
    FileDeleted,
    /// Manual configuration reload triggered
    ManualReload,
    /// Configuration rollback occurred
    Rollback,
}

/// Hot-reload statistics
#[derive(Debug, Clone, Default)]
pub struct ReloadStats {
    /// Total number of reload attempts
    pub total_reloads: u64,
    /// Number of successful reloads
    pub successful_reloads: u64,
    /// Number of failed reloads
    pub failed_reloads: u64,
    /// Number of rollbacks
    pub rollbacks: u64,
    /// Last successful reload timestamp
    pub last_successful_reload: Option<Instant>,
    /// Last failed reload timestamp
    pub last_failed_reload: Option<Instant>,
}

/// Hot-reload configuration settings
#[derive(Debug, Clone)]
pub struct HotReloadSettings {
    /// Debounce delay for file changes (prevents rapid reloads)
    pub debounce_delay: Duration,
    /// Maximum number of consecutive failures before disabling hot-reload
    pub max_consecutive_failures: u32,
    /// Enable automatic rollback on validation failure
    pub enable_rollback: bool,
    /// Enable detailed change logging
    pub verbose_logging: bool,
}

impl HotReloadManager {
    /// Create a new hot-reload manager for a configuration file
    pub fn new<P: AsRef<Path>>(config_path: P) -> Result<Self> {
        let config_path = config_path.as_ref().to_path_buf();
        
        // Load initial configuration
        let loader = ConfigLoader::new();
        let initial_config = if config_path.exists() {
            ConfigLoader::load_from_file(&config_path)?
        } else {
            loader.load()?
        };

        // Validate initial configuration
        let validator = ConfigValidator::new();
        validator.validate(&initial_config)?;

        // Create file system event channel
        let (fs_event_tx, fs_event_rx) = crossbeam::channel::unbounded();

        // Create configuration change broadcast channel
        let (change_tx, _) = broadcast::channel(100);

        Ok(Self {
            current_config: Arc::new(RwLock::new(initial_config.clone())),
            config_path,
            loader,
            validator,
            watcher: None,
            fs_event_tx: Some(fs_event_tx),
            fs_event_rx: Some(fs_event_rx),
            change_tx,
            last_valid_config: Arc::new(RwLock::new(initial_config)),
            stats: Arc::new(RwLock::new(ReloadStats::default())),
            settings: HotReloadSettings::default(),
        })
    }

    /// Create a hot-reload manager with custom settings
    pub fn with_settings<P: AsRef<Path>>(config_path: P, settings: HotReloadSettings) -> Result<Self> {
        let mut manager = Self::new(config_path)?;
        manager.settings = settings;
        Ok(manager)
    }

    /// Start watching for configuration file changes
    pub async fn start_watching(&mut self) -> Result<()> {
        info!("Starting configuration file watching: {}", self.config_path.display());

        // Create file system watcher
        let fs_event_tx = self.fs_event_tx.take()
            .ok_or_else(|| Error::Configuration { message: "File watcher already started".to_string() })?;

        let mut watcher = RecommendedWatcher::new(
            move |res| {
                if let Err(e) = fs_event_tx.send(res) {
                    error!("Failed to send file system event: {}", e);
                }
            },
            Config::default()
                .with_poll_interval(Duration::from_millis(500))
                .with_compare_contents(true),
        ).map_err(|e| Error::Configuration { message: format!("Failed to create file watcher: {}", e) })?;

        // Watch the configuration file and its parent directory
        if let Some(parent_dir) = self.config_path.parent() {
            watcher.watch(parent_dir, RecursiveMode::NonRecursive)
                .map_err(|e| Error::Configuration { message: format!("Failed to watch directory: {}", e) })?;
        }

        self.watcher = Some(watcher);

        // Start background task to handle file events
        let fs_event_rx = self.fs_event_rx.take()
            .ok_or_else(|| Error::Configuration { message: "Event receiver already taken".to_string() })?;

        self.start_reload_task(fs_event_rx).await;

        info!("Configuration file watching started successfully");
        Ok(())
    }

    /// Stop watching for configuration file changes
    pub fn stop_watching(&mut self) {
        info!("Stopping configuration file watching");
        self.watcher = None;
        self.fs_event_tx = None;
        self.fs_event_rx = None;
    }

    /// Get the current configuration (thread-safe read access)
    pub fn get_config(&self) -> MemoryStreamerConfig {
        self.current_config.read().clone()
    }

    /// Get a reference to the current configuration with read lock
    pub fn get_config_ref(&self) -> parking_lot::RwLockReadGuard<MemoryStreamerConfig> {
        self.current_config.read()
    }

    /// Subscribe to configuration change notifications
    pub fn subscribe_to_changes(&self) -> broadcast::Receiver<ConfigChangeEvent> {
        self.change_tx.subscribe()
    }

    /// Manually trigger a configuration reload
    pub async fn reload_config(&self) -> Result<()> {
        info!("Manual configuration reload triggered");
        self.perform_reload(ChangeType::ManualReload).await
    }

    /// Rollback to the last valid configuration
    pub async fn rollback_config(&self) -> Result<()> {
        info!("Configuration rollback triggered");
        
        let last_valid = self.last_valid_config.read().clone();
        
        // Validate the rollback configuration
        self.validator.validate(&last_valid)?;
        
        // Apply the rollback
        *self.current_config.write() = last_valid;
        
        // Update statistics
        {
            let mut stats = self.stats.write();
            stats.rollbacks += 1;
        }
        
        // Send notification
        self.send_change_notification(ConfigChangeEvent {
            timestamp: Instant::now(),
            change_type: ChangeType::Rollback,
            file_path: self.config_path.clone(),
            success: true,
            error: None,
        });
        
        info!("Configuration rollback completed successfully");
        Ok(())
    }

    /// Get current reload statistics
    pub fn get_stats(&self) -> ReloadStats {
        self.stats.read().clone()
    }

    /// Start the background task that handles file system events and reloads
    async fn start_reload_task(&self, fs_event_rx: Receiver<notify::Result<Event>>) {
        let config_path = self.config_path.clone();
        let current_config = Arc::clone(&self.current_config);
        let last_valid_config = Arc::clone(&self.last_valid_config);
        let stats = Arc::clone(&self.stats);
        let change_tx = self.change_tx.clone();
        let loader = self.loader.clone();
        let validator = self.validator.clone();
        let settings = self.settings.clone();

        tokio::spawn(async move {
            let mut last_event_time = Instant::now();
            let mut consecutive_failures = 0;

            loop {
                match fs_event_rx.try_recv() {
                    Ok(Ok(event)) => {
                        // Check if this event is for our configuration file
                        let should_reload = event.paths.iter().any(|path| {
                            path == &config_path || 
                            (path.file_name() == config_path.file_name() && 
                             path.parent() == config_path.parent())
                        });

                        if should_reload {
                            // Determine change type
                            let change_type = match event.kind {
                                EventKind::Create(_) => ChangeType::FileCreated,
                                EventKind::Remove(_) => ChangeType::FileDeleted,
                                EventKind::Modify(_) => ChangeType::FileModified,
                                _ => ChangeType::FileModified,
                            };

                            // Skip reload for file deletion
                            if change_type == ChangeType::FileDeleted {
                                warn!("Configuration file deleted: {}", config_path.display());
                                continue;
                            }

                            // Implement debouncing
                            let now = Instant::now();
                            if now.duration_since(last_event_time) < settings.debounce_delay {
                                debug!("Debouncing configuration reload");
                                continue;
                            }
                            last_event_time = now;

                            // Check consecutive failure limit
                            if consecutive_failures >= settings.max_consecutive_failures {
                                error!("Hot-reload disabled due to consecutive failures: {}", consecutive_failures);
                                continue;
                            }

                            if settings.verbose_logging {
                                info!("Configuration file changed: {:?}", change_type);
                            }

                            // Perform the reload
                            match Self::perform_reload_internal(
                                &config_path,
                                &loader,
                                &validator,
                                &current_config,
                                &last_valid_config,
                                &stats,
                                &change_tx,
                                change_type,
                                &settings,
                            ).await {
                                Ok(()) => {
                                    consecutive_failures = 0;
                                }
                                Err(e) => {
                                    consecutive_failures += 1;
                                    error!("Configuration reload failed: {}", e);
                                }
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        error!("File watcher error: {}", e);
                    }
                    Err(TryRecvError::Empty) => {
                        // No events available, sleep briefly
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    Err(TryRecvError::Disconnected) => {
                        info!("File watcher disconnected, stopping reload task");
                        break;
                    }
                }
            }
        });
    }

    /// Perform configuration reload (internal implementation)
    async fn perform_reload(&self, change_type: ChangeType) -> Result<()> {
        Self::perform_reload_internal(
            &self.config_path,
            &self.loader,
            &self.validator,
            &self.current_config,
            &self.last_valid_config,
            &self.stats,
            &self.change_tx,
            change_type,
            &self.settings,
        ).await
    }

    /// Internal reload implementation (static for use in background task)
    async fn perform_reload_internal(
        config_path: &Path,
        loader: &ConfigLoader,
        validator: &ConfigValidator,
        current_config: &Arc<RwLock<MemoryStreamerConfig>>,
        last_valid_config: &Arc<RwLock<MemoryStreamerConfig>>,
        stats: &Arc<RwLock<ReloadStats>>,
        change_tx: &broadcast::Sender<ConfigChangeEvent>,
        change_type: ChangeType,
        settings: &HotReloadSettings,
    ) -> Result<()> {
        let start_time = Instant::now();
        
        // Update statistics
        {
            let mut stats_guard = stats.write();
            stats_guard.total_reloads += 1;
        }

        // Load new configuration
        let new_config = if config_path.exists() {
            ConfigLoader::load_from_file(config_path)?
        } else {
            loader.load()?
        };

        // Validate new configuration
        if let Err(validation_error) = validator.validate(&new_config) {
            // Update failure statistics
            {
                let mut stats_guard = stats.write();
                stats_guard.failed_reloads += 1;
                stats_guard.last_failed_reload = Some(Instant::now());
            }

            // Send failure notification
            let event = ConfigChangeEvent {
                timestamp: start_time,
                change_type,
                file_path: config_path.to_path_buf(),
                success: false,
                error: Some(validation_error.to_string()),
            };
            
            let _ = change_tx.send(event);

            // Attempt rollback if enabled
            if settings.enable_rollback {
                warn!("Configuration validation failed, attempting rollback");
                let rollback_config = last_valid_config.read().clone();
                
                if validator.validate(&rollback_config).is_ok() {
                    *current_config.write() = rollback_config;
                    
                    // Send rollback notification
                    let rollback_event = ConfigChangeEvent {
                        timestamp: Instant::now(),
                        change_type: ChangeType::Rollback,
                        file_path: config_path.to_path_buf(),
                        success: true,
                        error: None,
                    };
                    
                    let _ = change_tx.send(rollback_event);
                    
                    {
                        let mut stats_guard = stats.write();
                        stats_guard.rollbacks += 1;
                    }
                    
                    warn!("Configuration rolled back to last valid state");
                }
            }

            return Err(validation_error);
        }

        // Save current config as last valid before updating
        *last_valid_config.write() = current_config.read().clone();

        // Apply new configuration
        *current_config.write() = new_config;

        // Update success statistics
        {
            let mut stats_guard = stats.write();
            stats_guard.successful_reloads += 1;
            stats_guard.last_successful_reload = Some(Instant::now());
        }

        // Send success notification
        let event = ConfigChangeEvent {
            timestamp: start_time,
            change_type,
            file_path: config_path.to_path_buf(),
            success: true,
            error: None,
        };

        let _ = change_tx.send(event);

        if settings.verbose_logging {
            info!("Configuration reloaded successfully in {:?}", start_time.elapsed());
        }

        Ok(())
    }

    /// Send configuration change notification
    fn send_change_notification(&self, event: ConfigChangeEvent) {
        let _ = self.change_tx.send(event);
    }
}

impl Default for HotReloadSettings {
    fn default() -> Self {
        Self {
            debounce_delay: Duration::from_millis(500),
            max_consecutive_failures: 5,
            enable_rollback: true,
            verbose_logging: false,
        }
    }
}

impl Clone for ConfigLoader {
    fn clone(&self) -> Self {
        // Create a new loader with the same settings
        ConfigLoader::new()
    }
}

impl Clone for ConfigValidator {
    fn clone(&self) -> Self {
        // Create a new validator with the same context
        ConfigValidator::new()
    }
}

/// Configuration hot-reload utilities
pub struct HotReloadUtils;

impl HotReloadUtils {
    /// Check if a file path represents a configuration file
    pub fn is_config_file(path: &Path) -> bool {
        if let Some(extension) = path.extension() {
            matches!(extension.to_str(), Some("toml") | Some("yaml") | Some("yml") | Some("json"))
        } else {
            false
        }
    }

    /// Validate file permissions for configuration hot-reload
    pub fn check_file_permissions(path: &Path) -> Result<()> {
        if !path.exists() {
            return Err(Error::Configuration { 
                message: format!("Configuration file does not exist: {}", path.display()) 
            });
        }

        if !path.is_file() {
            return Err(Error::Configuration { 
                message: format!("Configuration path is not a file: {}", path.display()) 
            });
        }

        // Check if file is readable
        match std::fs::File::open(path) {
            Ok(_) => Ok(()),
            Err(e) => Err(Error::Configuration { 
                message: format!("Cannot read configuration file: {}", e) 
            }),
        }
    }

    /// Create a backup of the current configuration file
    pub fn create_backup(config_path: &Path) -> Result<PathBuf> {
        let backup_path = config_path.with_extension(
            format!("{}.backup.{}", 
                config_path.extension().and_then(|s| s.to_str()).unwrap_or("toml"),
                chrono::Utc::now().format("%Y%m%d_%H%M%S")
            )
        );

        std::fs::copy(config_path, &backup_path)
            .map_err(|e| Error::Configuration { message: format!("Failed to create backup: {}", e) })?;

        info!("Configuration backup created: {}", backup_path.display());
        Ok(backup_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_hot_reload_manager_creation() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("test-config.toml");

        // Create a basic config file
        let config_content = r#"
[network]
port = 7878
bind_address = "0.0.0.0"

[performance]
target_latency_p99_us = 10
"#;
        fs::write(&config_path, config_content).expect("Failed to write config file");

        let manager = HotReloadManager::new(&config_path).expect("Failed to create manager");
        let config = manager.get_config();
        
        assert_eq!(config.network.port, 7878);
        assert_eq!(config.performance.target_latency_p99_us, 10);
    }

    #[tokio::test]
    async fn test_manual_reload() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("test-config.toml");

        // Create initial config
        let initial_config = r#"
[network]
port = 7878
"#;
        fs::write(&config_path, initial_config).expect("Failed to write initial config");

        let manager = HotReloadManager::new(&config_path).expect("Failed to create manager");
        assert_eq!(manager.get_config().network.port, 7878);

        // Update config file
        let updated_config = r#"
[network]
port = 9090
"#;
        fs::write(&config_path, updated_config).expect("Failed to write updated config");

        // Manually trigger reload
        manager.reload_config().await.expect("Failed to reload config");
        
        assert_eq!(manager.get_config().network.port, 9090);
    }

    #[test]
    fn test_hot_reload_settings() {
        let settings = HotReloadSettings {
            debounce_delay: Duration::from_millis(1000),
            max_consecutive_failures: 3,
            enable_rollback: false,
            verbose_logging: true,
        };

        assert_eq!(settings.debounce_delay, Duration::from_millis(1000));
        assert_eq!(settings.max_consecutive_failures, 3);
        assert!(!settings.enable_rollback);
        assert!(settings.verbose_logging);
    }
}