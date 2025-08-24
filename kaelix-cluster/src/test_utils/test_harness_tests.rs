//! Comprehensive Test Suite for Cluster Test Harness
//!
//! Validates the full functionality and reliability of the ClusterTestHarness
//! with extensive test coverage across various scenarios.

use super::mock_network::MockNetwork;
use super::test_harness::{
    ClusterTestHarness, ClusterTopology, EventRecorder, HarnessStatistics, MockNodeId,
    NetworkConditions, NetworkPartition, NodeState,
};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;

// Type alias for test results
type TestResult = Result<(), Box<dyn std::error::Error>>;

// Utility macro for generating tests with various node counts and topologies
macro_rules! generate_topology_tests {
    ($($name:ident, $node_count:expr, $topology:expr);*) => {
        $(
            #[tokio::test]
            async fn $name() -> TestResult {
                let harness = ClusterTestHarness::builder()
                    .node_count($node_count)
                    .topology($topology)
                    .timeout(Duration::from_secs(10))
                    .build()
                    .await?;

                assert_eq!(harness.nodes().len(), $node_count);
                assert!(harness.initialized);

                Ok(())
            }
        )*
    };
}

generate_topology_tests! {
    test_mesh_topology_3_nodes, 3, ClusterTopology::mesh();
    test_ring_topology_5_nodes, 5, ClusterTopology::ring();
    test_star_topology_4_nodes, 4, ClusterTopology::star();
    test_custom_topology_6_nodes, 6, ClusterTopology::custom(
        HashMap::from([
            (0, vec![1, 2]),
            (1, vec![0, 3]),
            (2, vec![0, 4]),
            (3, vec![1, 5]),
            (4, vec![2, 5]),
            (5, vec![3, 4])
        ])
    )
}

#[tokio::test]
async fn test_harness_multiple_node_configurations() -> TestResult {
    let configurations = vec![
        (2, ClusterTopology::Mesh),
        (3, ClusterTopology::Ring),
        (5, ClusterTopology::Star { center_node: 0 }),
        (10, ClusterTopology::Mesh),
    ];

    for (node_count, topology) in configurations {
        let harness = ClusterTestHarness::builder()
            .node_count(node_count)
            .topology(topology)
            .timeout(Duration::from_secs(10))
            .build()
            .await?;

        assert_eq!(harness.nodes().len(), node_count);
        assert!(harness.initialized);
    }

    Ok(())
}

#[tokio::test]
async fn test_cluster_lifecycle_with_event_recording() -> TestResult {
    let mut harness = ClusterTestHarness::builder()
        .node_count(3)
        .enable_event_recording()
        .timeout(Duration::from_secs(10))
        .build()
        .await?;

    // Start cluster and validate events
    harness.start_cluster().await?;

    // Wait for leader election
    let leader = harness.wait_for_leader_election().await?;
    assert!(harness.node_ids().contains(&leader));

    // Verify events were recorded
    let events = harness.recorder().events().await;
    assert!(events.iter().any(|e| e.category() == "node_lifecycle"));
    assert!(events.iter().any(|e| e.category() == "leader_election"));

    // Shutdown cluster
    harness.shutdown().await?;

    Ok(())
}

#[tokio::test]
async fn test_node_failure_and_recovery_scenarios() -> TestResult {
    let mut harness = ClusterTestHarness::builder()
        .node_count(5)
        .timeout(Duration::from_secs(15))
        .build()
        .await?;

    harness.start_cluster().await?;

    // Identify nodes to manipulate
    let node_ids = harness.node_ids();
    let failure_node = node_ids[0];
    let recovery_node = node_ids[1];

    // Fail a node
    harness
        .fail_nodes(&[failure_node], "test simulated failure".to_string())
        .await?;
    assert_eq!(harness.active_node_count().await, 4);

    // Recover the failed node
    harness.recover_nodes(&[failure_node]).await?;
    assert_eq!(harness.active_node_count().await, 5);

    // Stop a node
    harness.stop_nodes(&[recovery_node]).await?;
    assert_eq!(harness.active_node_count().await, 4);

    // Restart the stopped node
    harness.start_nodes(&[recovery_node]).await?;
    assert_eq!(harness.active_node_count().await, 5);

    harness.shutdown().await?;

    Ok(())
}

#[tokio::test]
async fn test_complex_network_partition() -> TestResult {
    let mut harness = ClusterTestHarness::builder()
        .node_count(6)
        .enable_event_recording()
        .timeout(Duration::from_secs(20))
        .build()
        .await?;

    harness.start_cluster().await?;

    let node_ids = harness.node_ids();
    let partition = NetworkPartition::new().split(&node_ids[0..3], &node_ids[3..6]);

    // Apply network partition
    harness.apply_network_partition(partition).await?;

    // Wait for split-brain detection
    harness.wait_for_split_brain_detection().await?;

    // Verify partition events
    let events = harness.recorder().events_by_category("network_partition").await;
    assert!(!events.is_empty());

    // Heal network partition
    harness.heal_network_partition().await?;

    // Validate cluster convergence
    harness.wait_for_cluster_convergence().await?;

    harness.shutdown().await?;

    Ok(())
}

#[tokio::test]
async fn test_performance_and_scalability() -> TestResult {
    // Test with a large number of nodes
    let node_count = 50;
    let start = std::time::Instant::now();

    let mut harness = ClusterTestHarness::builder()
        .node_count(node_count)
        .timeout(Duration::from_secs(60))
        .build()
        .await?;

    harness.start_cluster().await?;

    // Verify node count and active status
    assert_eq!(harness.nodes().len(), node_count);
    assert_eq!(harness.active_node_count().await, node_count);

    // Measure startup time
    let startup_duration = start.elapsed();
    assert!(startup_duration < Duration::from_secs(30), "Cluster startup took too long");

    // Perform some operations
    harness.wait_for_leader_election().await?;

    // Collect and validate statistics
    let stats: HarnessStatistics = harness.statistics().await;
    assert_eq!(stats.total_nodes, node_count);
    assert_eq!(stats.active_nodes, node_count);
    assert!(!stats.network_partition_active);

    harness.shutdown().await?;

    Ok(())
}

#[tokio::test]
async fn test_detailed_event_recording() -> TestResult {
    let mut harness = ClusterTestHarness::builder()
        .node_count(4)
        .enable_event_recording()
        .timeout(Duration::from_secs(15))
        .build()
        .await?;

    let recorder = harness.recorder().clone();

    harness.start_cluster().await?;

    // Simulate various events
    let node_ids = harness.node_ids();
    harness.fail_nodes(&[node_ids[0]], "test failure".to_string()).await?;
    harness.recover_nodes(&[node_ids[0]]).await?;

    let events = recorder.events().await;

    // Validate event categories and sequence
    let event_categories: Vec<_> = events.iter().map(|e| e.category()).collect();
    assert!(event_categories.contains(&"node_lifecycle"));
    assert!(event_categories.contains(&"node_health"));
    assert!(event_categories.contains(&"leader_election"));

    // Assert event sequence
    harness
        .assert_event_sequence(&[
            ExpectedEvent::NodeStartup,
            ExpectedEvent::LeaderElection,
            ExpectedEvent::NodeFailure,
            ExpectedEvent::NodeRecovery,
        ])
        .await?;

    harness.shutdown().await?;

    Ok(())
}

#[tokio::test]
async fn test_network_conditions_simulation() -> TestResult {
    let network_conditions = NetworkConditions::new()
        .with_latency(Duration::from_millis(50))
        .with_packet_loss(0.1)
        .with_bandwidth(1_000_000); // 1 Mbps

    let mut harness = ClusterTestHarness::builder()
        .node_count(3)
        .network_conditions(network_conditions)
        .timeout(Duration::from_secs(15))
        .build()
        .await?;

    harness.start_cluster().await?;

    // Verify network conditions applied
    let network = harness.network();
    assert!(network.current_conditions().is_some());

    harness.shutdown().await?;

    Ok(())
}

// Add this test to your test_harness_tests.rs
#[tokio::test]
async fn test_extensive_failure_recovery_scenario() -> TestResult {
    let mut harness = ClusterTestHarness::builder()
        .node_count(7)
        .timeout(Duration::from_secs(30))
        .build()
        .await?;

    harness.start_cluster().await?;

    let node_ids = harness.node_ids();

    // Fail multiple nodes
    let nodes_to_fail = &node_ids[0..3];
    harness.fail_nodes(nodes_to_fail, "cascading failure test".to_string()).await?;

    // Verify reduced active nodes
    assert_eq!(harness.active_node_count().await, 4);

    // Recover nodes
    harness.recover_nodes(nodes_to_fail).await?;

    // Verify full recovery
    assert_eq!(harness.active_node_count().await, 7);

    harness.shutdown().await?;

    Ok(())
}

#[tokio::test]
async fn test_custom_topology_connectivity() -> TestResult {
    let custom_topology = ClusterTopology::custom(HashMap::from([
        (0, vec![1, 2]),
        (1, vec![0, 3]),
        (2, vec![0, 4]),
        (3, vec![1, 5]),
        (4, vec![2, 5]),
        (5, vec![3, 4]),
    ]));

    let mut harness = ClusterTestHarness::builder()
        .node_count(6)
        .topology(custom_topology)
        .timeout(Duration::from_secs(15))
        .build()
        .await?;

    harness.start_cluster().await?;

    // Verify cluster formation
    assert_eq!(harness.active_node_count().await, 6);

    harness.shutdown().await?;

    Ok(())
}

// Error Handling and Edge Case Tests
#[tokio::test]
async fn test_invalid_configuration_handling() -> TestResult {
    // Test configurations that should trigger errors
    let invalid_configs = vec![
        (0, ClusterTopology::Mesh),    // Zero nodes
        (1001, ClusterTopology::Ring), // Too many nodes
    ];

    for (node_count, topology) in invalid_configs {
        let result = ClusterTestHarness::builder()
            .node_count(node_count)
            .topology(topology)
            .timeout(Duration::from_secs(5))
            .build()
            .await;

        assert!(result.is_err(), "Expected error for invalid configuration");
    }

    Ok(())
}
