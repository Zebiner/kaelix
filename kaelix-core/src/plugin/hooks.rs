//! Plugin hook system for extensible message processing.

use crate::message::Message;
use crate::plugin::{HookError, PluginError, PluginId, ProcessingContext};
use async_trait::async_trait;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Hook points in the message processing pipeline.
///
/// Defines specific points where plugins can intercept and modify
/// message processing behavior. Each hook point represents a different
/// stage in the pipeline with specific semantics.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HookPoint {
    /// Before message validation
    PreValidation,

    /// After message validation, before processing
    PostValidation,

    /// Before message transformation
    PreTransformation,

    /// After message transformation
    PostTransformation,

    /// Before message routing
    PreRouting,

    /// After message routing
    PostRouting,

    /// Before message serialization
    PreSerialization,

    /// After message serialization
    PostSerialization,

    /// Before message processing starts
    PreProcessing,

    /// After message processing completes
    PostProcessing,

    /// Before error handling
    PreError,

    /// After error handling
    PostError,

    /// Custom hook point (user-defined)
    Custom(u64), // Hash of the custom hook name
}

impl HookPoint {
    /// Create a custom hook point with the given name.
    pub fn custom(name: &str) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        name.hash(&mut hasher);
        Self::Custom(hasher.finish())
    }

    /// Get the string name of this hook point.
    pub fn name(&self) -> &'static str {
        match self {
            Self::PreValidation => "pre_validation",
            Self::PostValidation => "post_validation",
            Self::PreTransformation => "pre_transformation",
            Self::PostTransformation => "post_transformation",
            Self::PreRouting => "pre_routing",
            Self::PostRouting => "post_routing",
            Self::PreSerialization => "pre_serialization",
            Self::PostSerialization => "post_serialization",
            Self::PreProcessing => "pre_processing",
            Self::PostProcessing => "post_processing",
            Self::PreError => "pre_error",
            Self::PostError => "post_error",
            Self::Custom(_) => "custom",
        }
    }

    /// Check if this is a pre-processing hook.
    pub fn is_pre_hook(&self) -> bool {
        matches!(
            self,
            Self::PreValidation
                | Self::PreTransformation
                | Self::PreRouting
                | Self::PreSerialization
                | Self::PreProcessing
                | Self::PreError
        )
    }

    /// Check if this is a post-processing hook.
    pub fn is_post_hook(&self) -> bool {
        matches!(
            self,
            Self::PostValidation
                | Self::PostTransformation
                | Self::PostRouting
                | Self::PostSerialization
                | Self::PostProcessing
                | Self::PostError
        )
    }
}

/// Hook execution priority.
///
/// Determines the order in which hooks are executed at each hook point.
/// Lower values indicate higher priority (execute first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct HookPriority(i32);

impl HookPriority {
    /// Highest priority (executes first).
    pub const HIGHEST: Self = Self(-1000);

    /// High priority.
    pub const HIGH: Self = Self(-100);

    /// Normal priority (default).
    pub const NORMAL: Self = Self(0);

    /// Low priority.
    pub const LOW: Self = Self(100);

    /// Lowest priority (executes last).
    pub const LOWEST: Self = Self(1000);

    /// Create a custom priority.
    pub const fn custom(priority: i32) -> Self {
        Self(priority)
    }

    /// Get the numeric priority value.
    pub const fn value(&self) -> i32 {
        self.0
    }
}

impl Default for HookPriority {
    fn default() -> Self {
        Self::NORMAL
    }
}

/// Hook function result.
///
/// Indicates whether the hook execution was successful and whether
/// processing should continue normally.
///
/// Note: This enum cannot be serialized due to containing ProcessingContext
/// which has non-serializable fields like Instant and Arc<RwLock<...>>.
#[derive(Debug, Clone)]
pub enum HookResult {
    /// Continue processing normally
    Continue,

    /// Stop processing (but not an error)
    Stop,

    /// Modify the message and continue
    ModifyAndContinue(Message),

    /// Replace the processing context and continue
    UpdateContext(ProcessingContext),

    /// Both modify message and update context
    ModifyBoth(Message, ProcessingContext),
}

impl PartialEq for HookResult {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Continue, Self::Continue) => true,
            (Self::Stop, Self::Stop) => true,
            (Self::ModifyAndContinue(msg1), Self::ModifyAndContinue(msg2)) => msg1 == msg2,
            // ProcessingContext doesn't implement PartialEq, so these are never equal
            (Self::UpdateContext(_), Self::UpdateContext(_)) => false,
            (Self::ModifyBoth(msg1, _), Self::ModifyBoth(msg2, _)) => msg1 == msg2,
            _ => false,
        }
    }
}

impl Default for HookResult {
    fn default() -> Self {
        Self::Continue
    }
}

/// Hook function trait for implementing custom hooks.
#[async_trait]
pub trait Hook: Send + Sync + 'static {
    /// Execute the hook with the given message and context.
    ///
    /// # Parameters
    /// - `message`: The message being processed
    /// - `context`: Processing context with metadata
    ///
    /// # Returns
    /// - Hook result indicating how to proceed
    ///
    /// # Errors
    /// - Hook-specific processing errors
    async fn execute(
        &self,
        message: &Message,
        context: &ProcessingContext,
    ) -> Result<HookResult, HookError>;
}

/// Hook registration information.
#[derive(Clone)]
pub struct HookRegistration {
    /// Plugin that registered this hook
    pub plugin_id: PluginId,

    /// Hook point where this hook executes
    pub hook_point: HookPoint,

    /// Execution priority
    pub priority: HookPriority,

    /// Hook function (type-erased)
    pub hook: Arc<dyn Hook>,

    /// Registration timestamp
    pub registered_at: Instant,

    /// Optional hook metadata
    pub metadata: HashMap<String, String>,
}

impl std::fmt::Debug for HookRegistration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HookRegistration")
            .field("plugin_id", &self.plugin_id)
            .field("hook_point", &self.hook_point)
            .field("priority", &self.priority)
            .field("hook", &"<hook function>")
            .field("registered_at", &self.registered_at)
            .field("metadata", &self.metadata)
            .finish()
    }
}

impl HookRegistration {
    /// Create a new hook registration.
    pub fn new(
        plugin_id: PluginId,
        hook_point: HookPoint,
        priority: HookPriority,
        hook: Arc<dyn Hook>,
    ) -> Self {
        Self {
            plugin_id,
            hook_point,
            priority,
            hook,
            registered_at: Instant::now(),
            metadata: HashMap::new(),
        }
    }

    /// Add metadata to the hook registration.
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Get hook age (time since registration).
    pub fn age(&self) -> Duration {
        self.registered_at.elapsed()
    }
}

/// High-performance hook manager with O(1) lookups.
///
/// Manages plugin hooks with minimal overhead and efficient execution.
/// Uses concurrent data structures for lock-free hook registration
/// and lookup operations.
///
/// # Performance Characteristics
///
/// - Hook registration: O(1) amortized
/// - Hook lookup: O(1) + O(n) where n = hooks per point (typically small)
/// - Hook execution: O(n) where n = number of hooks at the point
/// - Memory usage: ~100 bytes per hook registration
///
/// # Examples
///
/// ```rust
/// use kaelix_core::plugin::*;
///
/// # tokio_test::block_on(async {
/// let mut manager = HookManager::new();
///
/// // Register a hook
/// let hook = Arc::new(MyCustomHook);
/// let registration = HookRegistration::new(
///     PluginId::new(),
///     HookPoint::PreProcessing,
///     HookPriority::NORMAL,
///     hook,
/// );
///
/// manager.register_hook(registration).await?;
///
/// // Execute hooks at a point
/// let message = Message::new("test.topic", Bytes::from("data")).unwrap();
/// let context = ProcessingContext::new(&message);
/// let results = manager.execute_hooks(HookPoint::PreProcessing, &message, &context).await?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// # });
/// ```
pub struct HookManager {
    /// Hook storage organized by hook point
    hooks: DashMap<HookPoint, Vec<HookRegistration>>,

    /// Hook lookup by plugin ID for management
    plugin_hooks: DashMap<PluginId, Vec<HookPoint>>,

    /// Performance metrics
    metrics: HookManagerMetrics,

    /// Manager creation timestamp
    created_at: Instant,
}

impl HookManager {
    /// Create a new hook manager.
    pub fn new() -> Self {
        Self {
            hooks: DashMap::new(),
            plugin_hooks: DashMap::new(),
            metrics: HookManagerMetrics::new(),
            created_at: Instant::now(),
        }
    }

    /// Register a hook at the specified hook point.
    ///
    /// # Parameters
    /// - `registration`: Hook registration containing hook details
    ///
    /// # Returns
    /// - `Ok(())`: Hook registered successfully
    /// - `Err(HookError)`: Registration failed
    ///
    /// # Errors
    /// - `RegistrationFailed`: If registration fails due to conflicts
    pub async fn register_hook(&self, registration: HookRegistration) -> Result<(), HookError> {
        let hook_point = registration.hook_point.clone();
        let plugin_id = registration.plugin_id;

        // Update plugin hook tracking
        self.plugin_hooks
            .entry(plugin_id)
            .or_insert_with(Vec::new)
            .push(hook_point.clone());

        // Add hook to the hook point (maintaining priority order)
        let mut hooks = self.hooks.entry(hook_point.clone()).or_insert_with(Vec::new);
        hooks.push(registration);

        // Sort by priority to maintain execution order
        hooks.sort_by_key(|reg| reg.priority);

        // Record metrics
        self.metrics.record_registration();

        tracing::debug!(
            plugin_id = %plugin_id,
            hook_point = ?hook_point,
            "Hook registered successfully"
        );

        Ok(())
    }

    /// Unregister all hooks for a plugin.
    ///
    /// # Parameters
    /// - `plugin_id`: ID of the plugin whose hooks to unregister
    ///
    /// # Returns
    /// - Number of hooks unregistered
    pub async fn unregister_plugin_hooks(&self, plugin_id: PluginId) -> usize {
        let mut unregistered_count = 0;

        // Get all hook points for this plugin
        if let Some((_, hook_points)) = self.plugin_hooks.remove(&plugin_id) {
            for hook_point in hook_points {
                if let Some(mut hooks) = self.hooks.get_mut(&hook_point) {
                    let original_len = hooks.len();
                    hooks.retain(|reg| reg.plugin_id != plugin_id);
                    unregistered_count += original_len - hooks.len();
                }
            }
        }

        // Record metrics
        for _ in 0..unregistered_count {
            self.metrics.record_unregistration();
        }

        tracing::debug!(
            plugin_id = %plugin_id,
            unregistered_count = unregistered_count,
            "Plugin hooks unregistered"
        );

        unregistered_count
    }

    /// Execute all hooks at the specified hook point.
    ///
    /// Executes hooks in priority order (highest priority first).
    /// If any hook returns `Stop`, execution stops immediately.
    ///
    /// # Parameters
    /// - `hook_point`: Hook point to execute
    /// - `message`: Message being processed
    /// - `context`: Processing context
    ///
    /// # Returns
    /// - `Vec<HookResult>`: Results from all executed hooks
    ///
    /// # Errors
    /// - `ExecutionFailed`: If any hook execution fails
    pub async fn execute_hooks(
        &self,
        hook_point: HookPoint,
        message: &Message,
        context: &ProcessingContext,
    ) -> Result<Vec<HookResult>, HookError> {
        let start_time = Instant::now();
        let mut results = Vec::new();

        // Get hooks for this point (already sorted by priority)
        if let Some(hooks) = self.hooks.get(&hook_point) {
            for registration in hooks.iter() {
                let hook_start = Instant::now();

                match registration.hook.execute(message, context).await {
                    Ok(result) => {
                        let hook_duration = hook_start.elapsed();
                        self.metrics.record_execution(hook_duration, true);

                        // Check if we should stop processing
                        let should_stop = matches!(result, HookResult::Stop);
                        results.push(result);

                        if should_stop {
                            tracing::debug!(
                                plugin_id = %registration.plugin_id,
                                hook_point = ?hook_point,
                                "Hook execution stopped processing"
                            );
                            break;
                        }
                    }
                    Err(error) => {
                        let hook_duration = hook_start.elapsed();
                        self.metrics.record_execution(hook_duration, false);

                        tracing::error!(
                            plugin_id = %registration.plugin_id,
                            hook_point = ?hook_point,
                            error = %error,
                            "Hook execution failed"
                        );

                        return Err(HookError::ExecutionFailed {
                            plugin_id: registration.plugin_id.to_string(),
                            hook_point: hook_point.name().to_string(),
                            reason: error.to_string(),
                        });
                    }
                }
            }
        }

        let total_duration = start_time.elapsed();
        tracing::trace!(
            hook_point = ?hook_point,
            hook_count = results.len(),
            duration_ms = total_duration.as_millis(),
            "Hook execution completed"
        );

        Ok(results)
    }

    /// Get the number of hooks registered at a hook point.
    pub fn hook_count(&self, hook_point: &HookPoint) -> usize {
        self.hooks
            .get(hook_point)
            .map(|hooks| hooks.len())
            .unwrap_or(0)
    }

    /// Get the total number of registered hooks.
    pub fn total_hook_count(&self) -> usize {
        self.hooks.iter().map(|entry| entry.value().len()).sum()
    }

    /// Get all hook points that have registered hooks.
    pub fn active_hook_points(&self) -> Vec<HookPoint> {
        self.hooks
            .iter()
            .filter(|entry| !entry.value().is_empty())
            .map(|entry| entry.key().clone())
            .collect()
    }

    /// Get hooks registered by a specific plugin.
    pub fn plugin_hook_count(&self, plugin_id: PluginId) -> usize {
        self.plugin_hooks
            .get(&plugin_id)
            .map(|hooks| hooks.len())
            .unwrap_or(0)
    }

    /// Get detailed hook information for monitoring.
    pub fn hook_details(&self, hook_point: &HookPoint) -> Vec<HookRegistration> {
        self.hooks
            .get(hook_point)
            .map(|hooks| hooks.clone())
            .unwrap_or_default()
    }

    /// Check if any hooks are registered at a hook point.
    pub fn has_hooks(&self, hook_point: &HookPoint) -> bool {
        self.hook_count(hook_point) > 0
    }

    /// Get manager performance metrics.
    pub fn metrics(&self) -> &HookManagerMetrics {
        &self.metrics
    }

    /// Get manager uptime.
    pub fn uptime(&self) -> Duration {
        self.created_at.elapsed()
    }

    /// Clear all hooks (for testing/reset purposes).
    pub fn clear_all_hooks(&self) {
        self.hooks.clear();
        self.plugin_hooks.clear();
    }

    /// Execute hooks with timeout and error handling.
    ///
    /// This is a more robust version that includes timeout handling
    /// and enhanced error recovery.
    pub async fn execute_hooks_with_timeout(
        &self,
        hook_point: HookPoint,
        message: &Message,
        context: &ProcessingContext,
        timeout: Duration,
    ) -> Result<Vec<HookResult>, HookError> {
        let execution = self.execute_hooks(hook_point.clone(), message, context);

        match tokio::time::timeout(timeout, execution).await {
            Ok(result) => result,
            Err(_) => Err(HookError::ExecutionFailed {
                plugin_id: "timeout".to_string(),
                hook_point: hook_point.name().to_string(),
                reason: format!("Hook execution timed out after {:?}", timeout),
            }),
        }
    }
}

impl Default for HookManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Hook manager performance metrics.
#[derive(Debug)]
pub struct HookManagerMetrics {
    /// Total hook registrations
    total_registrations: AtomicU64,

    /// Total hook unregistrations
    total_unregistrations: AtomicU64,

    /// Total hook executions
    total_executions: AtomicU64,

    /// Successful hook executions
    successful_executions: AtomicU64,

    /// Failed hook executions
    failed_executions: AtomicU64,

    /// Average execution time per hook
    avg_execution_time: Arc<parking_lot::RwLock<Duration>>,

    /// Metrics creation timestamp
    created_at: Instant,
}

impl Clone for HookManagerMetrics {
    fn clone(&self) -> Self {
        Self {
            total_registrations: AtomicU64::new(self.total_registrations.load(Ordering::Relaxed)),
            total_unregistrations: AtomicU64::new(self.total_unregistrations.load(Ordering::Relaxed)),
            total_executions: AtomicU64::new(self.total_executions.load(Ordering::Relaxed)),
            successful_executions: AtomicU64::new(self.successful_executions.load(Ordering::Relaxed)),
            failed_executions: AtomicU64::new(self.failed_executions.load(Ordering::Relaxed)),
            avg_execution_time: self.avg_execution_time.clone(),
            created_at: self.created_at,
        }
    }
}

impl HookManagerMetrics {
    /// Create new hook manager metrics.
    fn new() -> Self {
        Self {
            total_registrations: AtomicU64::new(0),
            total_unregistrations: AtomicU64::new(0),
            total_executions: AtomicU64::new(0),
            successful_executions: AtomicU64::new(0),
            failed_executions: AtomicU64::new(0),
            avg_execution_time: Arc::new(parking_lot::RwLock::new(Duration::default())),
            created_at: Instant::now(),
        }
    }

    /// Record a hook registration.
    fn record_registration(&self) {
        self.total_registrations.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a hook unregistration.
    fn record_unregistration(&self) {
        self.total_unregistrations.fetch_add(1, Ordering::Relaxed);
    }

    /// Record hook execution.
    fn record_execution(&self, duration: Duration, success: bool) {
        self.total_executions.fetch_add(1, Ordering::Relaxed);
        
        if success {
            self.successful_executions.fetch_add(1, Ordering::Relaxed);
        } else {
            self.failed_executions.fetch_add(1, Ordering::Relaxed);
        }

        // Update average execution time (simplified)
        {
            let mut avg = self.avg_execution_time.write();
            let total = self.total_executions.load(Ordering::Relaxed);
            *avg = (*avg * (total - 1) as u32 + duration) / total as u32;
        }
    }

    /// Get total registrations.
    pub fn total_registrations(&self) -> u64 {
        self.total_registrations.load(Ordering::Relaxed)
    }

    /// Get total executions.
    pub fn total_executions(&self) -> u64 {
        self.total_executions.load(Ordering::Relaxed)
    }

    /// Get execution success rate.
    pub fn execution_success_rate(&self) -> f64 {
        let total = self.total_executions();
        let successful = self.successful_executions.load(Ordering::Relaxed);
        
        if total > 0 {
            (successful as f64 / total as f64) * 100.0
        } else {
            0.0
        }
    }

    /// Get average execution time.
    pub fn average_execution_time(&self) -> Duration {
        *self.avg_execution_time.read()
    }

    /// Get metrics uptime.
    pub fn uptime(&self) -> Duration {
        self.created_at.elapsed()
    }
}

// Extension trait for PluginError to determine criticality
trait PluginErrorExt {
    fn is_critical(&self) -> bool;
}

impl PluginErrorExt for PluginError {
    fn is_critical(&self) -> bool {
        matches!(
            self,
            PluginError::SecurityViolation { .. }
                | PluginError::ResourceLimitExceeded { .. }
                | PluginError::Internal { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_point_creation() {
        assert_eq!(HookPoint::PreValidation.name(), "pre_validation");
        assert_eq!(HookPoint::PostProcessing.name(), "post_processing");

        let custom = HookPoint::custom("my_custom_hook");
        assert!(matches!(custom, HookPoint::Custom(_)));
        assert_eq!(custom.name(), "custom");

        // Same name should produce same hash
        let custom2 = HookPoint::custom("my_custom_hook");
        assert_eq!(custom, custom2);
    }

    #[test]
    fn test_hook_point_classification() {
        assert!(HookPoint::PreValidation.is_pre_hook());
        assert!(!HookPoint::PreValidation.is_post_hook());

        assert!(HookPoint::PostProcessing.is_post_hook());
        assert!(!HookPoint::PostProcessing.is_pre_hook());

        let custom = HookPoint::custom("test");
        assert!(!custom.is_pre_hook());
        assert!(!custom.is_post_hook());
    }

    #[test]
    fn test_hook_priority() {
        assert!(HookPriority::HIGHEST < HookPriority::HIGH);
        assert!(HookPriority::HIGH < HookPriority::NORMAL);
        assert!(HookPriority::NORMAL < HookPriority::LOW);
        assert!(HookPriority::LOW < HookPriority::LOWEST);

        let custom = HookPriority::custom(-50);
        assert!(HookPriority::HIGHEST < custom);
        assert!(custom < HookPriority::HIGH);
    }

    #[test]
    fn test_hook_result() {
        let result = HookResult::default();
        assert!(matches!(result, HookResult::Continue));

        let stop = HookResult::Stop;
        assert!(matches!(stop, HookResult::Stop));
    }

    #[test]
    fn test_hook_registration() {
        use crate::plugin::PluginId;
        use std::sync::Arc;

        struct TestHook;

        #[async_trait]
        impl Hook for TestHook {
            async fn execute(
                &self,
                _message: &Message,
                _context: &ProcessingContext,
            ) -> Result<HookResult, HookError> {
                Ok(HookResult::Continue)
            }
        }

        let plugin_id = PluginId::new();
        let hook = Arc::new(TestHook);
        let registration = HookRegistration::new(
            plugin_id,
            HookPoint::PreProcessing,
            HookPriority::NORMAL,
            hook,
        );

        assert_eq!(registration.plugin_id, plugin_id);
        assert_eq!(registration.hook_point, HookPoint::PreProcessing);
        assert_eq!(registration.priority, HookPriority::NORMAL);
        assert!(registration.age() >= Duration::ZERO);
    }

    #[test]
    fn test_hook_manager_creation() {
        let manager = HookManager::new();
        assert_eq!(manager.total_hook_count(), 0);
        assert!(manager.active_hook_points().is_empty());
        assert!(!manager.has_hooks(&HookPoint::PreProcessing));
        assert!(manager.uptime() >= Duration::ZERO);
    }

    #[test]
    fn test_hook_manager_metrics() {
        let metrics = HookManagerMetrics::new();
        assert_eq!(metrics.total_registrations(), 0);
        assert_eq!(metrics.total_executions(), 0);
        assert_eq!(metrics.execution_success_rate(), 0.0);
        assert_eq!(metrics.average_execution_time(), Duration::default());

        metrics.record_registration();
        assert_eq!(metrics.total_registrations(), 1);

        metrics.record_execution(Duration::from_millis(10), true);
        assert_eq!(metrics.total_executions(), 1);
        assert_eq!(metrics.execution_success_rate(), 100.0);

        metrics.record_execution(Duration::from_millis(20), false);
        assert_eq!(metrics.total_executions(), 2);
        assert_eq!(metrics.execution_success_rate(), 50.0);
    }

    #[tokio::test]
    async fn test_hook_manager_registration() {
        use bytes::Bytes;

        struct TestHook;

        #[async_trait]
        impl Hook for TestHook {
            async fn execute(
                &self,
                _message: &Message,
                _context: &ProcessingContext,
            ) -> Result<HookResult, HookError> {
                Ok(HookResult::Continue)
            }
        }

        let manager = HookManager::new();
        let plugin_id = PluginId::new();
        let hook = Arc::new(TestHook);

        let registration = HookRegistration::new(
            plugin_id,
            HookPoint::PreProcessing,
            HookPriority::NORMAL,
            hook,
        );

        manager.register_hook(registration).await.unwrap();

        assert_eq!(manager.total_hook_count(), 1);
        assert!(manager.has_hooks(&HookPoint::PreProcessing));
        assert_eq!(manager.plugin_hook_count(plugin_id), 1);

        let unregistered = manager.unregister_plugin_hooks(plugin_id).await;
        assert_eq!(unregistered, 1);
        assert_eq!(manager.total_hook_count(), 0);
    }

    #[tokio::test]
    async fn test_hook_execution() {
        use bytes::Bytes;

        struct TestHook {
            result: HookResult,
        }

        #[async_trait]
        impl Hook for TestHook {
            async fn execute(
                &self,
                _message: &Message,
                _context: &ProcessingContext,
            ) -> Result<HookResult, HookError> {
                Ok(self.result.clone())
            }
        }

        let manager = HookManager::new();
        let plugin_id = PluginId::new();

        // Register multiple hooks with different priorities
        let high_prio_hook = Arc::new(TestHook {
            result: HookResult::Continue,
        });
        let low_prio_hook = Arc::new(TestHook {
            result: HookResult::Continue,
        });

        let high_registration = HookRegistration::new(
            plugin_id,
            HookPoint::PreProcessing,
            HookPriority::HIGH,
            high_prio_hook,
        );

        let low_registration = HookRegistration::new(
            plugin_id,
            HookPoint::PreProcessing,
            HookPriority::LOW,
            low_prio_hook,
        );

        manager.register_hook(high_registration).await.unwrap();
        manager.register_hook(low_registration).await.unwrap();

        let message = Message::new("test.topic".into(), Bytes::from("test")).unwrap();
        let context = ProcessingContext::new(&message);

        let results = manager
            .execute_hooks(HookPoint::PreProcessing, &message, &context)
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| matches!(r, HookResult::Continue)));
    }

    #[tokio::test]
    async fn test_hook_execution_stop() {
        use bytes::Bytes;

        struct StopHook;

        #[async_trait]
        impl Hook for StopHook {
            async fn execute(
                &self,
                _message: &Message,
                _context: &ProcessingContext,
            ) -> Result<HookResult, HookError> {
                Ok(HookResult::Stop)
            }
        }

        struct ContinueHook;

        #[async_trait]
        impl Hook for ContinueHook {
            async fn execute(
                &self,
                _message: &Message,
                _context: &ProcessingContext,
            ) -> Result<HookResult, HookError> {
                Ok(HookResult::Continue)
            }
        }

        let manager = HookManager::new();
        let plugin_id = PluginId::new();

        // Register stop hook with higher priority
        let stop_registration = HookRegistration::new(
            plugin_id,
            HookPoint::PreProcessing,
            HookPriority::HIGH,
            Arc::new(StopHook),
        );

        // Register continue hook with lower priority (should not execute)
        let continue_registration = HookRegistration::new(
            plugin_id,
            HookPoint::PreProcessing,
            HookPriority::LOW,
            Arc::new(ContinueHook),
        );

        manager.register_hook(stop_registration).await.unwrap();
        manager.register_hook(continue_registration).await.unwrap();

        let message = Message::new("test.topic".into(), Bytes::from("test")).unwrap();
        let context = ProcessingContext::new(&message);

        let results = manager
            .execute_hooks(HookPoint::PreProcessing, &message, &context)
            .await
            .unwrap();

        // Should only have one result (the stop hook)
        assert_eq!(results.len(), 1);
        assert!(matches!(results[0], HookResult::Stop));
    }
}