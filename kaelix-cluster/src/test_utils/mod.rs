//! # Test Utilities Module
//!
//! Provides comprehensive testing infrastructure for distributed cluster operations,
//! including mock network simulation, performance benchmarking, and test helpers.
//!
//! ## Features
//!
//! - **Mock Network**: Realistic network simulation with latency, packet loss, and partitions
//! - **Test Harness**: Multi-node cluster simulation framework with complete lifecycle management
//! - **Test Helpers**: Common utilities for setting up test scenarios
//! - **Performance Benchmarks**: Tools for measuring cluster performance
//! - **Resource Limits**: Configurable resource constraints for test environments
//!
//! ## Usage
//!
//! This module is only available when testing or when the "dev" feature is enabled.
//! It provides essential infrastructure for testing distributed systems behavior
//! under various network conditions and failure scenarios.

use std::{future::Future, time::Duration};

pub mod mock_network;
pub mod test_harness;

// ================================================================================================
// Test Configuration Constants
// ================================================================================================

/// Default timeout for test operations (30 seconds)
pub const DEFAULT_TEST_TIMEOUT_MS: u64 = 30_000;

/// Maximum number of test nodes in a single test scenario
pub const MAX_TEST_NODES: usize = 100;

/// Default latency for test network conditions (1ms)
pub const DEFAULT_TEST_LATENCY_MS: u64 = 1;

/// Maximum memory usage per test node in megabytes
pub const MAX_MEMORY_PER_NODE_MB: usize = 100; // 100MB per test node

/// Maximum CPU usage percentage for test scenarios
pub const MAX_CPU_PERCENTAGE: u32 = 80; // 80% CPU usage limit

// ================================================================================================
// Test Helper Traits
// ================================================================================================

/// Trait for objects that can be used in cluster tests
pub trait TestClusterNode {
    /// Get the test node identifier
    fn test_id(&self) -> String;

    /// Check if the node is ready for testing
    fn is_test_ready(&self) -> bool;

    /// Cleanup test resources
    fn cleanup_test(&mut self);
}

/// Trait for test scenarios that can be executed
pub trait TestScenario {
    /// The error type for test operations
    type Error;

    /// Setup the test scenario
    fn setup(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Execute the test scenario
    fn execute(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Teardown and cleanup after the test
    fn teardown(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Get the scenario name for reporting
    fn name(&self) -> &str;
}

// ================================================================================================
// Test Helper Functions
// ================================================================================================

/// Create a test timeout duration from milliseconds
pub fn test_timeout(ms: u64) -> Duration {
    Duration::from_millis(ms)
}

/// Create a default test timeout (30 seconds)
pub fn default_test_timeout() -> Duration {
    test_timeout(DEFAULT_TEST_TIMEOUT_MS)
}

/// Generate a unique test identifier
pub fn generate_test_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);

    format!(
        "test-{}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        id
    )
}

/// Check if running in CI environment
pub fn is_ci_environment() -> bool {
    std::env::var("CI").is_ok()
        || std::env::var("GITHUB_ACTIONS").is_ok()
        || std::env::var("GITLAB_CI").is_ok()
}

/// Get appropriate test parallelism based on environment
pub fn test_parallelism() -> usize {
    if is_ci_environment() {
        // Limit parallelism in CI to avoid resource exhaustion
        std::cmp::min(num_cpus::get(), 4)
    } else {
        // Use all available cores in development
        num_cpus::get()
    }
}

// ================================================================================================
// Test Assertions and Validation
// ================================================================================================

/// Assert that a duration is within expected bounds
pub fn assert_duration_bounds(
    actual: Duration,
    expected: Duration,
    tolerance_percent: f64,
    message: &str,
) {
    let tolerance = expected.as_nanos() as f64 * (tolerance_percent / 100.0);
    let lower_bound = expected.as_nanos() as f64 - tolerance;
    let upper_bound = expected.as_nanos() as f64 + tolerance;
    let actual_nanos = actual.as_nanos() as f64;

    assert!(
        actual_nanos >= lower_bound && actual_nanos <= upper_bound,
        "{}: expected duration within {}% of {:?}, got {:?}",
        message,
        tolerance_percent,
        expected,
        actual
    );
}

/// Assert that a value is within a percentage range of expected value
pub fn assert_within_percent<T>(actual: T, expected: T, tolerance_percent: f64, message: &str)
where
    T: Into<f64> + Copy + std::fmt::Debug,
{
    let actual_f = actual.into();
    let expected_f = expected.into();
    let tolerance = expected_f.abs() * (tolerance_percent / 100.0);
    let diff = (actual_f - expected_f).abs();

    assert!(
        diff <= tolerance,
        "{}: expected within {}% of {:?}, got {:?} (diff: {:.2})",
        message,
        tolerance_percent,
        expected,
        actual,
        diff
    );
}

// ================================================================================================
// Test Resource Management
// ================================================================================================

/// Resource tracker for test scenarios
pub struct TestResourceTracker {
    /// Maximum memory usage per node in bytes
    max_memory_per_node: usize,
    /// Maximum CPU percentage to use
    max_cpu_percentage: u32,
    /// Active test nodes
    active_nodes: std::collections::HashSet<String>,
}

impl TestResourceTracker {
    /// Create a new resource tracker with default limits
    pub fn new() -> Self {
        Self {
            max_memory_per_node: MAX_MEMORY_PER_NODE_MB * 1024 * 1024,
            max_cpu_percentage: MAX_CPU_PERCENTAGE,
            active_nodes: std::collections::HashSet::new(),
        }
    }

    /// Register a test node
    pub fn register_node(&mut self, node_id: String) -> Result<(), String> {
        if self.active_nodes.len() >= MAX_TEST_NODES {
            return Err(format!("Maximum test nodes ({}) exceeded", MAX_TEST_NODES));
        }

        self.active_nodes.insert(node_id);
        Ok(())
    }

    /// Unregister a test node
    pub fn unregister_node(&mut self, node_id: &str) -> bool {
        self.active_nodes.remove(node_id)
    }

    /// Get current active node count
    pub fn active_node_count(&self) -> usize {
        self.active_nodes.len()
    }

    /// Check if resource limits are being respected
    pub fn check_resource_limits(&self) -> Result<(), String> {
        let node_count = self.active_nodes.len();
        let estimated_memory = node_count * self.max_memory_per_node;

        // Simple memory check (this is just an estimate)
        if estimated_memory > 1_000_000_000 {
            // 1GB total limit
            return Err(format!(
                "Estimated memory usage ({} MB) exceeds reasonable limits",
                estimated_memory / (1024 * 1024)
            ));
        }

        Ok(())
    }
}

impl Default for TestResourceTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeout_creation() {
        let timeout = test_timeout(5000);
        assert_eq!(timeout, Duration::from_millis(5000));

        let default_timeout = default_test_timeout();
        assert_eq!(default_timeout, Duration::from_millis(DEFAULT_TEST_TIMEOUT_MS));
    }

    #[test]
    fn test_id_generation() {
        let id1 = generate_test_id();
        let id2 = generate_test_id();

        assert_ne!(id1, id2);
        assert!(id1.starts_with("test-"));
        assert!(id2.starts_with("test-"));
    }

    #[test]
    fn test_parallelism_basic() {
        let parallelism = test_parallelism();
        // Simple check that function returns a reasonable value
        assert!(parallelism > 0);
    }

    #[test]
    fn test_duration_bounds_assertion() {
        let target = Duration::from_millis(100);
        let within_bounds = Duration::from_millis(105); // 5% over
        let out_of_bounds = Duration::from_millis(120); // 20% over

        // This should pass
        assert_duration_bounds(within_bounds, target, 10.0, "Should be within bounds");

        // This should panic
        let result = std::panic::catch_unwind(|| {
            assert_duration_bounds(out_of_bounds, target, 10.0, "Should be out of bounds");
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_within_percent_assertion() {
        // This should pass
        assert_within_percent(95.0, 100.0, 10.0, "Should be within 10%");

        // This should panic
        let result = std::panic::catch_unwind(|| {
            assert_within_percent(80.0, 100.0, 10.0, "Should be out of range");
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_resource_tracker() {
        let mut tracker = TestResourceTracker::new();

        assert_eq!(tracker.active_node_count(), 0);

        // Register some nodes
        tracker.register_node("node1".to_string()).unwrap();
        tracker.register_node("node2".to_string()).unwrap();

        assert_eq!(tracker.active_node_count(), 2);

        // Check resource limits
        tracker.check_resource_limits().unwrap();

        // Unregister a node
        assert!(tracker.unregister_node("node1"));
        assert_eq!(tracker.active_node_count(), 1);

        // Try to unregister non-existent node
        assert!(!tracker.unregister_node("nonexistent"));
    }
}
