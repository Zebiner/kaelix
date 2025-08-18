//! Plugin lifecycle management with state machines.

use crate::plugin::{PluginError, PluginId, PluginResult};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::time::timeout;

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
        Self::Always {
            max_attempts: 10,
            restart_delay: Duration::from_secs(1),
        }
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
        self.plugin_id
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
    ///
    /// Validates the transition and updates the state machine accordingly.
    ///
    /// # Parameters
    /// - `target_state`: The state to transition to
    ///
    /// # Returns
    /// - `Ok(())`: Transition successful
    /// - `Err(PluginError)`: Invalid transition or transition failed
    pub async fn transition_to(&mut self, target_state: PluginState) -> PluginResult<()> {
        let current = self.current_state().await;

        // Validate transition
        if !current.can_transition_to(target_state) {
            return Err(PluginError::InvalidStateTransition {
                plugin_id: self.plugin_id.to_string(),
                from: format!("{:?}", current),
                to: format!("{:?}", target_state),
            });
        }

        // Perform the transition
        self.previous_state = Some(current);
        self.current_state = target_state;
        self.stats.record_transition();

        tracing::debug!(
            plugin_id = %self.plugin_id,
            from_state = ?current,
            to_state = ?target_state,
            "Plugin state transition"
        );

        // Handle special transition logic
        match target_state {
            PluginState::Failed => {
                self.handle_failure().await?;
            }
            PluginState::Loading => {
                // Reset restart attempts on manual restart
                if matches!(current, PluginState::Failed | PluginState::Stopped) {
                    self.restart_attempts = 0;
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Force transition to a state without validation (emergency use only).
    ///
    /// # Safety
    /// This bypasses all validation and should only be used in emergency
    /// situations where the normal state machine has become corrupted.
    pub async fn force_transition(&mut self, target_state: PluginState) {
        tracing::warn!(
            plugin_id = %self.plugin_id,
            from_state = ?self.current_state,
            to_state = ?target_state,
            "Force state transition (emergency)"
        );

        self.previous_state = Some(self.current_state);
        self.current_state = target_state;
    }

    /// Handle plugin failure and apply restart policy.
    async fn handle_failure(&mut self) -> PluginResult<()> {
        tracing::warn!(
            plugin_id = %self.plugin_id,
            restart_attempts = self.restart_attempts,
            "Plugin failed, evaluating restart policy"
        );

        match &self.restart_policy {
            RestartPolicy::Never => {
                tracing::info!(
                    plugin_id = %self.plugin_id,
                    "Plugin failure - no restart policy"
                );
            }
            RestartPolicy::Always { max_attempts, restart_delay } => {
                if self.restart_attempts < *max_attempts {
                    self.schedule_restart(*restart_delay).await?;
                } else {
                    tracing::error!(
                        plugin_id = %self.plugin_id,
                        max_attempts = max_attempts,
                        "Plugin exceeded maximum restart attempts"
                    );
                }
            }
            RestartPolicy::OnFailure {
                max_attempts,
                restart_delay,
                backoff_multiplier,
            } => {
                if self.restart_attempts < *max_attempts {
                    let delay = Duration::from_secs_f64(
                        restart_delay.as_secs_f64() * backoff_multiplier.powi(self.restart_attempts as i32)
                    );
                    self.schedule_restart(delay).await?;
                } else {
                    tracing::error!(
                        plugin_id = %self.plugin_id,
                        max_attempts = max_attempts,
                        "Plugin exceeded maximum restart attempts with backoff"
                    );
                }
            }
        }

        Ok(())
    }

    /// Schedule a plugin restart after the specified delay.
    async fn schedule_restart(&mut self, delay: Duration) -> PluginResult<()> {
        self.restart_attempts += 1;
        self.last_restart = Some(Instant::now());
        self.stats.record_restart();

        tracing::info!(
            plugin_id = %self.plugin_id,
            restart_attempt = self.restart_attempts,
            delay_ms = delay.as_millis(),
            "Scheduling plugin restart"
        );

        // In a real implementation, this would schedule an async task
        // to restart the plugin after the delay
        // For now, we just record the intent
        Ok(())
    }

    /// Perform a health check with timeout.
    ///
    /// # Parameters
    /// - `health_check`: Async function that performs the health check
    /// - `timeout_duration`: Maximum time to wait for health check
    ///
    /// # Returns
    /// - `Ok(true)`: Health check passed
    /// - `Ok(false)`: Health check failed
    /// - `Err(PluginError)`: Health check timed out or errored
    pub async fn health_check<F, Fut>(&self, health_check: F, timeout_duration: Duration) -> PluginResult<bool>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = PluginResult<bool>>,
    {
        let start_time = Instant::now();

        let result = match timeout(timeout_duration, health_check()).await {
            Ok(health_result) => {
                let success = health_result.unwrap_or(false);
                self.stats.record_health_check(success);
                Ok(success)
            }
            Err(_) => {
                self.stats.record_health_check(false);
                Err(PluginError::ExecutionError {
                    plugin_id: self.plugin_id.to_string(),
                    operation: "health_check".to_string(),
                    reason: format!("Health check timed out after {:?}", timeout_duration),
                })
            }
        };

        let check_duration = start_time.elapsed();
        tracing::debug!(
            plugin_id = %self.plugin_id,
            duration_ms = check_duration.as_millis(),
            success = result.as_ref().unwrap_or(&false),
            "Health check completed"
        );

        result
    }

    /// Get lifecycle uptime.
    pub fn uptime(&self) -> Duration {
        self.created_at.elapsed()
    }

    /// Get time since last restart.
    pub fn time_since_restart(&self) -> Option<Duration> {
        self.last_restart.map(|restart| restart.elapsed())
    }

    /// Get lifecycle statistics.
    pub fn stats(&self) -> &LifecycleStats {
        &self.stats
    }

    /// Reset restart attempts (useful for manual intervention).
    pub fn reset_restart_attempts(&mut self) {
        self.restart_attempts = 0;
        tracing::info!(
            plugin_id = %self.plugin_id,
            "Reset restart attempts"
        );
    }

    /// Update the restart policy.
    pub fn update_restart_policy(&mut self, policy: RestartPolicy) {
        self.restart_policy = policy;
        tracing::info!(
            plugin_id = %self.plugin_id,
            policy = ?self.restart_policy,
            "Updated restart policy"
        );
    }
}

/// Lifecycle performance statistics.
#[derive(Debug)]
pub struct LifecycleStats {
    /// Total number of state transitions
    total_transitions: AtomicU64,

    /// Total number of restart attempts
    total_restarts: AtomicU64,

    /// Total number of health checks performed
    total_health_checks: AtomicU64,

    /// Number of successful health checks
    successful_health_checks: AtomicU64,

    /// Statistics creation timestamp
    created_at: Instant,
}

impl Clone for LifecycleStats {
    fn clone(&self) -> Self {
        Self {
            total_transitions: AtomicU64::new(self.total_transitions.load(Ordering::Relaxed)),
            total_restarts: AtomicU64::new(self.total_restarts.load(Ordering::Relaxed)),
            total_health_checks: AtomicU64::new(self.total_health_checks.load(Ordering::Relaxed)),
            successful_health_checks: AtomicU64::new(self.successful_health_checks.load(Ordering::Relaxed)),
            created_at: self.created_at,
        }
    }
}

impl LifecycleStats {
    /// Create new lifecycle statistics.
    fn new() -> Self {
        Self {
            total_transitions: AtomicU64::new(0),
            total_restarts: AtomicU64::new(0),
            total_health_checks: AtomicU64::new(0),
            successful_health_checks: AtomicU64::new(0),
            created_at: Instant::now(),
        }
    }

    /// Record a state transition.
    fn record_transition(&self) {
        self.total_transitions.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a restart attempt.
    fn record_restart(&self) {
        self.total_restarts.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a health check.
    pub fn record_health_check(&self, successful: bool) {
        self.total_health_checks.fetch_add(1, Ordering::Relaxed);
        if successful {
            self.successful_health_checks.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Get total number of transitions.
    pub fn total_transitions(&self) -> u64 {
        self.total_transitions.load(Ordering::Relaxed)
    }

    /// Get total number of restarts.
    pub fn total_restarts(&self) -> u64 {
        self.total_restarts.load(Ordering::Relaxed)
    }

    /// Get total number of health checks.
    pub fn total_health_checks(&self) -> u64 {
        self.total_health_checks.load(Ordering::Relaxed)
    }

    /// Get health check success rate.
    pub fn health_check_success_rate(&self) -> f64 {
        let total = self.total_health_checks();
        let successful = self.successful_health_checks.load(Ordering::Relaxed);
        
        if total > 0 {
            (successful as f64 / total as f64) * 100.0
        } else {
            0.0
        }
    }

    /// Get statistics uptime.
    pub fn uptime(&self) -> Duration {
        self.created_at.elapsed()
    }
}

/// Immutable snapshot of lifecycle statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleStatsSnapshot {
    pub total_transitions: u64,
    pub total_restarts: u64,
    pub total_health_checks: u64,
    pub successful_health_checks: u64,
    pub health_check_success_rate: f64,
    pub uptime_seconds: u64,
}

impl From<&LifecycleStats> for LifecycleStatsSnapshot {
    fn from(stats: &LifecycleStats) -> Self {
        Self {
            total_transitions: stats.total_transitions(),
            total_restarts: stats.total_restarts(),
            total_health_checks: stats.total_health_checks(),
            successful_health_checks: stats.successful_health_checks.load(Ordering::Relaxed),
            health_check_success_rate: stats.health_check_success_rate(),
            uptime_seconds: stats.uptime().as_secs(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_state_transitions() {
        assert!(PluginState::Unloaded.can_transition_to(PluginState::Loading));
        assert!(PluginState::Loading.can_transition_to(PluginState::Initializing));
        assert!(PluginState::Initializing.can_transition_to(PluginState::Running));
        assert!(PluginState::Running.can_transition_to(PluginState::Paused));
        assert!(PluginState::Paused.can_transition_to(PluginState::Running));
        assert!(PluginState::Running.can_transition_to(PluginState::Stopped));

        // Invalid transitions
        assert!(!PluginState::Unloaded.can_transition_to(PluginState::Running));
        assert!(!PluginState::Stopped.can_transition_to(PluginState::Running));
    }

    #[test]
    fn test_plugin_state_checks() {
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

    #[test]
    fn test_restart_policy() {
        let conservative = RestartPolicy::conservative();
        match conservative {
            RestartPolicy::OnFailure { max_attempts, .. } => {
                assert_eq!(max_attempts, 3);
            }
            _ => panic!("Expected OnFailure policy"),
        }

        let aggressive = RestartPolicy::aggressive();
        match aggressive {
            RestartPolicy::Always { max_attempts, .. } => {
                assert_eq!(max_attempts, 10);
            }
            _ => panic!("Expected Always policy"),
        }

        let never = RestartPolicy::never();
        assert!(matches!(never, RestartPolicy::Never));
    }

    #[tokio::test]
    async fn test_lifecycle_creation() {
        let plugin_id = PluginId::new();
        let lifecycle = PluginLifecycle::new(plugin_id, RestartPolicy::conservative());
        
        assert_eq!(lifecycle.current_state().await, PluginState::Unloaded);
        assert_eq!(lifecycle.plugin_id(), plugin_id);
        assert_eq!(lifecycle.restart_attempts(), 0);
        assert!(lifecycle.previous_state().is_none());
    }

    #[tokio::test]
    async fn test_lifecycle_transitions() {
        let plugin_id = PluginId::new();
        let mut lifecycle = PluginLifecycle::new(plugin_id, RestartPolicy::conservative());

        // Valid transition sequence
        lifecycle.transition_to(PluginState::Loading).await.unwrap();
        assert_eq!(lifecycle.current_state().await, PluginState::Loading);
        assert_eq!(lifecycle.previous_state(), Some(PluginState::Unloaded));

        lifecycle.transition_to(PluginState::Initializing).await.unwrap();
        assert_eq!(lifecycle.current_state().await, PluginState::Initializing);

        lifecycle.transition_to(PluginState::Running).await.unwrap();
        assert_eq!(lifecycle.current_state().await, PluginState::Running);
        assert!(lifecycle.is_active().await);

        // Invalid transition should fail
        let result = lifecycle.transition_to(PluginState::Unloaded).await;
        assert!(result.is_err());
        assert_eq!(lifecycle.current_state().await, PluginState::Running); // State unchanged
    }

    #[tokio::test]
    async fn test_lifecycle_force_transition() {
        let plugin_id = PluginId::new();
        let mut lifecycle = PluginLifecycle::new(plugin_id, RestartPolicy::conservative());

        // Force transition should work even if invalid
        lifecycle.force_transition(PluginState::Running).await;
        assert_eq!(lifecycle.current_state().await, PluginState::Running);
        assert_eq!(lifecycle.previous_state(), Some(PluginState::Unloaded));
    }

    #[tokio::test]
    async fn test_lifecycle_health_check() {
        let plugin_id = PluginId::new();
        let lifecycle = PluginLifecycle::new(plugin_id, RestartPolicy::conservative());

        // Successful health check
        let result = lifecycle
            .health_check(|| async { Ok(true) }, Duration::from_millis(100))
            .await;
        assert!(result.unwrap());

        // Failed health check
        let result = lifecycle
            .health_check(|| async { Ok(false) }, Duration::from_millis(100))
            .await;
        assert!(!result.unwrap());

        // Timeout health check
        let result = lifecycle
            .health_check(
                || async {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    Ok(true)
                },
                Duration::from_millis(50),
            )
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_lifecycle_stats() {
        let stats = LifecycleStats::new();
        assert_eq!(stats.total_transitions(), 0);
        assert_eq!(stats.total_restarts(), 0);
        assert_eq!(stats.total_health_checks(), 0);
        assert_eq!(stats.health_check_success_rate(), 0.0);

        stats.record_transition();
        stats.record_restart();
        stats.record_health_check(true);
        stats.record_health_check(false);
        stats.record_health_check(true);

        assert_eq!(stats.total_transitions(), 1);
        assert_eq!(stats.total_restarts(), 1);
        assert_eq!(stats.total_health_checks(), 3);
        assert!((stats.health_check_success_rate() - 66.66666666666667).abs() < f64::EPSILON);
    }

    #[test]
    fn test_lifecycle_stats_snapshot() {
        let stats = LifecycleStats::new();
        stats.record_transition();
        stats.record_health_check(true);

        let snapshot = LifecycleStatsSnapshot::from(&stats);
        assert_eq!(snapshot.total_transitions, 1);
        assert_eq!(snapshot.total_health_checks, 1);
        assert_eq!(snapshot.successful_health_checks, 1);
        assert_eq!(snapshot.health_check_success_rate, 100.0);
    }

    #[test]
    fn test_lifecycle_stats_clone() {
        let stats = LifecycleStats::new();
        stats.record_transition();
        stats.record_restart();

        let cloned_stats = stats.clone();
        assert_eq!(cloned_stats.total_transitions(), 1);
        assert_eq!(cloned_stats.total_restarts(), 1);

        // Original and clone should be independent
        stats.record_transition();
        assert_eq!(stats.total_transitions(), 2);
        assert_eq!(cloned_stats.total_transitions(), 1);
    }
}