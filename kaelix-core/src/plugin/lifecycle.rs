//! Plugin lifecycle management with state machines.

use crate::plugin::{PluginError, PluginId, PluginResult};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Plugin lifecycle states for state machine management.
///
/// Represents the complete lifecycle of a plugin from loading to shutdown,
/// with proper state transitions and error handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginState {
    /// Plugin is unloaded (initial state)
    Unloaded,

    /// Plugin is being loaded
    Loading,

    /// Plugin is being initialized
    Initializing,

    /// Plugin is running and processing messages
    Running,

    /// Plugin is paused (temporarily stopped but preserving state)
    Paused,

    /// Plugin is stopped (gracefully shut down)
    Stopped,

    /// Plugin is being reloaded (hot-reload in progress)
    Reloading,

    /// Plugin has failed and is in error state
    Failed,
}

impl Default for PluginState {
    fn default() -> Self {
        Self::Unloaded
    }
}

impl std::fmt::Display for PluginState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unloaded => write!(f, "Unloaded"),
            Self::Loading => write!(f, "Loading"),
            Self::Initializing => write!(f, "Initializing"),
            Self::Running => write!(f, "Running"),
            Self::Paused => write!(f, "Paused"),
            Self::Stopped => write!(f, "Stopped"),
            Self::Reloading => write!(f, "Reloading"),
            Self::Failed => write!(f, "Failed"),
        }
    }
}

impl PluginState {
    /// Check if the plugin is in an active state.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Running | Self::Paused)
    }

    /// Check if the plugin is in a transitional state.
    pub fn is_transitional(&self) -> bool {
        matches!(self, Self::Loading | Self::Initializing | Self::Reloading)
    }

    /// Check if the plugin is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Stopped | Self::Failed)
    }

    /// Get valid transition states from the current state.
    pub fn valid_transitions(&self) -> Vec<PluginState> {
        match self {
            Self::Unloaded => vec![Self::Loading],
            Self::Loading => vec![Self::Initializing, Self::Failed],
            Self::Initializing => vec![Self::Running, Self::Failed],
            Self::Running => vec![Self::Paused, Self::Stopped, Self::Reloading, Self::Failed],
            Self::Paused => vec![Self::Running, Self::Stopped, Self::Failed],
            Self::Stopped => vec![Self::Loading],
            Self::Reloading => vec![Self::Running, Self::Failed],
            Self::Failed => vec![Self::Loading],
        }
    }

    /// Check if a transition to the target state is valid.
    pub fn can_transition_to(&self, target: PluginState) -> bool {
        self.valid_transitions().contains(&target)
    }
}

/// Plugin restart policies for automatic recovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RestartPolicy {
    /// Never restart the plugin automatically
    Never,

    /// Always restart the plugin on failure
    Always {
        /// Maximum number of restart attempts
        max_attempts: u32,
        /// Delay between restart attempts
        restart_delay: Duration,
    },

    /// Restart only on specific failure conditions
    OnFailure {
        /// Maximum number of restart attempts
        max_attempts: u32,
        /// Delay between restart attempts
        restart_delay: Duration,
        /// Backoff multiplier for restart delays
        backoff_multiplier: f64,
    },
}

impl RestartPolicy {
    /// Create a conservative restart policy for production use.
    pub fn conservative() -> Self {
        Self::OnFailure {
            max_attempts: 3,
            restart_delay: Duration::from_secs(5),
            backoff_multiplier: 2.0,
        }
    }

    /// Create an aggressive restart policy for development.
    pub fn aggressive() -> Self {
        Self::Always { max_attempts: 10, restart_delay: Duration::from_secs(1) }
    }

    /// Create a no-restart policy.
    pub fn never() -> Self {
        Self::Never
    }
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self::conservative()
    }
}

/// Plugin lifecycle manager with state machine and restart capabilities.
///
/// Manages the complete lifecycle of a plugin including state transitions,
/// restart policies, and error recovery.
///
/// # State Machine
///
/// ```text
/// Unloaded -> Loading -> Initializing -> Running
///     ^          |           |            |
///     |          |           |            v
///     |          |           |         Paused
///     |          |           |            |
///     |          |           |            v
///     |          |           |        Stopped
///     |          |           |            |
///     |          |           |            |
///     |          v           v            |
///     |       Failed <-------------------+
///     |          |
///     +----------+
/// ```
///
/// # Examples
///
/// ```rust
/// use kaelix_core::plugin::*;
///
/// # tokio_test::block_on(async {
/// let mut lifecycle = PluginLifecycle::new(
///     PluginId::new(),
///     RestartPolicy::conservative(),
/// );
///
/// // Transition through states
/// lifecycle.transition_to(PluginState::Loading).await?;
/// lifecycle.transition_to(PluginState::Initializing).await?;
/// lifecycle.transition_to(PluginState::Running).await?;
///
/// assert_eq!(lifecycle.current_state().await, PluginState::Running);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// # });
/// ```
pub struct PluginLifecycle {
    /// Plugin ID
    plugin_id: PluginId,

    /// Current plugin state
    current_state: PluginState,

    /// Previous plugin state
    previous_state: Option<PluginState>,

    /// Restart policy
    restart_policy: RestartPolicy,

    /// Number of restart attempts
    restart_attempts: u32,

    /// Last restart timestamp
    last_restart: Option<Instant>,

    /// Lifecycle creation timestamp
    created_at: Instant,

    /// Lifecycle statistics
    stats: LifecycleStats,
}

impl PluginLifecycle {
    /// Create a new plugin lifecycle manager.
    pub fn new(plugin_id: PluginId, restart_policy: RestartPolicy) -> Self {
        Self {
            plugin_id,
            current_state: PluginState::Unloaded,
            previous_state: None,
            restart_policy,
            restart_attempts: 0,
            last_restart: None,
            created_at: Instant::now(),
            stats: LifecycleStats::new(),
        }
    }

    /// Get the current plugin state.
    pub async fn current_state(&self) -> PluginState {
        self.current_state
    }

    /// Get the previous plugin state.
    pub fn previous_state(&self) -> Option<PluginState> {
        self.previous_state
    }

    /// Get the plugin ID.
    pub fn plugin_id(&self) -> PluginId {
        self.plugin_id.clone()
    }

    /// Get the restart policy.
    pub fn restart_policy(&self) -> &RestartPolicy {
        &self.restart_policy
    }

    /// Get the number of restart attempts.
    pub fn restart_attempts(&self) -> u32 {
        self.restart_attempts
    }

    /// Check if the plugin is in an active state.
    pub async fn is_active(&self) -> bool {
        self.current_state().await.is_active()
    }

    /// Check if the plugin is in a transitional state.
    pub async fn is_transitional(&self) -> bool {
        self.current_state().await.is_transitional()
    }

    /// Check if the plugin is in a terminal state.
    pub async fn is_terminal(&self) -> bool {
        self.current_state().await.is_terminal()
    }

    /// Transition to a new state.
    pub async fn transition_to(&mut self, new_state: PluginState) -> PluginResult<()> {
        if !self.current_state.can_transition_to(new_state) {
            return Err(PluginError::InvalidStateTransition {
                plugin_id: self.plugin_id.to_string(),
                from: self.current_state.to_string(),
                to: new_state.to_string(),
            });
        }

        self.previous_state = Some(self.current_state);
        self.current_state = new_state;

        // Update statistics
        self.stats.state_transitions.fetch_add(1, Ordering::Relaxed);

        tracing::info!(
            plugin_id = %self.plugin_id,
            from_state = ?self.previous_state,
            to_state = ?new_state,
            "Plugin state transition"
        );

        Ok(())
    }

    /// Force transition to a state (bypass validation).
    pub async fn force_transition_to(&mut self, new_state: PluginState) {
        self.previous_state = Some(self.current_state);
        self.current_state = new_state;

        // Update statistics
        self.stats.forced_transitions.fetch_add(1, Ordering::Relaxed);

        tracing::warn!(
            plugin_id = %self.plugin_id,
            from_state = ?self.previous_state,
            to_state = ?new_state,
            "Forced plugin state transition"
        );
    }

    /// Check if a restart should be attempted based on the restart policy.
    pub fn should_restart(&self) -> bool {
        match &self.restart_policy {
            RestartPolicy::Never => false,
            RestartPolicy::Always { max_attempts, .. } => self.restart_attempts < *max_attempts,
            RestartPolicy::OnFailure { max_attempts, .. } => {
                self.current_state == PluginState::Failed && self.restart_attempts < *max_attempts
            },
        }
    }

    /// Get the restart delay based on the restart policy and attempt count.
    pub fn get_restart_delay(&self) -> Option<Duration> {
        if !self.should_restart() {
            return None;
        }

        match &self.restart_policy {
            RestartPolicy::Never => None,
            RestartPolicy::Always { restart_delay, .. } => Some(*restart_delay),
            RestartPolicy::OnFailure { restart_delay, backoff_multiplier, .. } => {
                let delay = restart_delay.as_millis() as f64
                    * backoff_multiplier.powi(self.restart_attempts as i32);
                Some(Duration::from_millis(delay as u64))
            },
        }
    }

    /// Increment restart attempts and update timestamp.
    pub fn increment_restart_attempts(&mut self) {
        self.restart_attempts += 1;
        self.last_restart = Some(Instant::now());
        self.stats.restart_attempts.fetch_add(1, Ordering::Relaxed);
    }

    /// Reset restart attempts (called after successful restart).
    pub fn reset_restart_attempts(&mut self) {
        self.restart_attempts = 0;
        self.last_restart = None;
    }

    /// Get lifecycle statistics.
    pub fn get_stats(&self) -> &LifecycleStats {
        &self.stats
    }

    /// Get lifecycle uptime.
    pub fn uptime(&self) -> Duration {
        self.created_at.elapsed()
    }

    /// Get time since last restart.
    pub fn time_since_last_restart(&self) -> Option<Duration> {
        self.last_restart.map(|restart_time| restart_time.elapsed())
    }

    /// Check if the lifecycle is healthy.
    pub fn is_healthy(&self) -> bool {
        !self.current_state.is_terminal() && self.restart_attempts < 10
    }
}

/// Lifecycle statistics for monitoring and debugging.
#[derive(Debug, Default)]
pub struct LifecycleStats {
    /// Total number of state transitions
    pub state_transitions: AtomicU64,

    /// Number of forced state transitions
    pub forced_transitions: AtomicU64,

    /// Number of restart attempts
    pub restart_attempts: AtomicU64,

    /// Time spent in each state (in milliseconds)
    pub time_in_running: AtomicU64,
    pub time_in_paused: AtomicU64,
    pub time_in_failed: AtomicU64,
}

impl LifecycleStats {
    /// Create new lifecycle statistics.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get total state transitions.
    pub fn total_transitions(&self) -> u64 {
        self.state_transitions.load(Ordering::Relaxed)
    }

    /// Get total forced transitions.
    pub fn total_forced_transitions(&self) -> u64 {
        self.forced_transitions.load(Ordering::Relaxed)
    }

    /// Get total restart attempts.
    pub fn total_restart_attempts(&self) -> u64 {
        self.restart_attempts.load(Ordering::Relaxed)
    }

    /// Check if the lifecycle has excessive forced transitions.
    pub fn has_excessive_forced_transitions(&self) -> bool {
        let total = self.total_transitions();
        let forced = self.total_forced_transitions();

        if total == 0 {
            false
        } else {
            (forced as f64 / total as f64) > 0.1 // More than 10% forced transitions
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_lifecycle_creation() {
        let plugin_id = Uuid::new_v4();
        let lifecycle = PluginLifecycle::new(plugin_id, RestartPolicy::conservative());

        assert_eq!(lifecycle.current_state().await, PluginState::Unloaded);
        assert_eq!(lifecycle.plugin_id(), plugin_id);
        assert!(!lifecycle.is_active().await);
    }

    #[tokio::test]
    async fn test_valid_state_transitions() {
        let plugin_id = Uuid::new_v4();
        let mut lifecycle = PluginLifecycle::new(plugin_id, RestartPolicy::conservative());

        // Valid transition sequence
        lifecycle.transition_to(PluginState::Loading).await.unwrap();
        assert_eq!(lifecycle.current_state().await, PluginState::Loading);

        lifecycle.transition_to(PluginState::Initializing).await.unwrap();
        assert_eq!(lifecycle.current_state().await, PluginState::Initializing);

        lifecycle.transition_to(PluginState::Running).await.unwrap();
        assert_eq!(lifecycle.current_state().await, PluginState::Running);
        assert!(lifecycle.is_active().await);

        lifecycle.transition_to(PluginState::Paused).await.unwrap();
        assert_eq!(lifecycle.current_state().await, PluginState::Paused);
        assert!(lifecycle.is_active().await);

        lifecycle.transition_to(PluginState::Stopped).await.unwrap();
        assert_eq!(lifecycle.current_state().await, PluginState::Stopped);
        assert!(lifecycle.is_terminal().await);
    }

    #[tokio::test]
    async fn test_invalid_state_transitions() {
        let plugin_id = Uuid::new_v4();
        let mut lifecycle = PluginLifecycle::new(plugin_id, RestartPolicy::conservative());

        // Invalid transition: Unloaded -> Running
        let result = lifecycle.transition_to(PluginState::Running).await;
        assert!(result.is_err());
        assert_eq!(lifecycle.current_state().await, PluginState::Unloaded);
    }

    #[tokio::test]
    async fn test_forced_transitions() {
        let plugin_id = Uuid::new_v4();
        let mut lifecycle = PluginLifecycle::new(plugin_id, RestartPolicy::conservative());

        // Force invalid transition
        lifecycle.force_transition_to(PluginState::Running).await;
        assert_eq!(lifecycle.current_state().await, PluginState::Running);

        let stats = lifecycle.get_stats();
        assert_eq!(stats.total_forced_transitions(), 1);
    }

    #[test]
    fn test_restart_policies() {
        let plugin_id = Uuid::new_v4();

        // Never restart
        let lifecycle_never = PluginLifecycle::new(plugin_id, RestartPolicy::Never);
        assert!(!lifecycle_never.should_restart());

        // Always restart
        let lifecycle_always = PluginLifecycle::new(
            plugin_id,
            RestartPolicy::Always { max_attempts: 3, restart_delay: Duration::from_secs(1) },
        );
        assert!(lifecycle_always.should_restart());

        // On failure restart
        let mut lifecycle_failure = PluginLifecycle::new(
            plugin_id,
            RestartPolicy::OnFailure {
                max_attempts: 3,
                restart_delay: Duration::from_secs(1),
                backoff_multiplier: 2.0,
            },
        );

        // Not failed, so no restart
        assert!(!lifecycle_failure.should_restart());

        // Simulate failure
        lifecycle_failure.current_state = PluginState::Failed;
        assert!(lifecycle_failure.should_restart());
    }

    #[test]
    fn test_restart_delay_calculation() {
        let plugin_id = Uuid::new_v4();
        let mut lifecycle = PluginLifecycle::new(
            plugin_id,
            RestartPolicy::OnFailure {
                max_attempts: 3,
                restart_delay: Duration::from_secs(1),
                backoff_multiplier: 2.0,
            },
        );

        lifecycle.current_state = PluginState::Failed;

        // First attempt: 1 second
        assert_eq!(lifecycle.get_restart_delay(), Some(Duration::from_secs(1)));

        // Second attempt: 2 seconds
        lifecycle.increment_restart_attempts();
        assert_eq!(lifecycle.get_restart_delay(), Some(Duration::from_secs(2)));

        // Third attempt: 4 seconds
        lifecycle.increment_restart_attempts();
        assert_eq!(lifecycle.get_restart_delay(), Some(Duration::from_secs(4)));

        // Fourth attempt: should not restart
        lifecycle.increment_restart_attempts();
        assert_eq!(lifecycle.get_restart_delay(), None);
    }

    #[test]
    fn test_plugin_state_properties() {
        assert!(PluginState::Running.is_active());
        assert!(PluginState::Paused.is_active());
        assert!(!PluginState::Stopped.is_active());

        assert!(PluginState::Loading.is_transitional());
        assert!(PluginState::Initializing.is_transitional());
        assert!(!PluginState::Running.is_transitional());

        assert!(PluginState::Stopped.is_terminal());
        assert!(PluginState::Failed.is_terminal());
        assert!(!PluginState::Running.is_terminal());
    }
}
