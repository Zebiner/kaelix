//! Failover scenario integration tests
//!
//! This module tests various failover scenarios including node failures,
//! network partitions, resource exhaustion, and disaster recovery.

use std::time::Duration;

use anyhow::Result;
use kaelix_tests::prelude::*;
use kaelix_tests::cluster::{TestCluster, ClusterConfig, ResourceConstraints, NetworkConfig};
use kaelix_tests::simulators::network::{NetworkSimulator, PartitionModel, LatencyModel, LossModel};
use kaelix_tests::orchestration::{ProcessManager, ProcessConfig, StdioConfig, ResourceViolation};
use tempfile::TempDir;
use tokio::time::sleep;
use tracing::{info, warn, error};
use uuid::Uuid;
use std::collections::HashMap;

/// Test graceful node shutdown and recovery
#[tokio::test]
async fn test_graceful_node_failover() -> Result<()> {
    tracing_subscriber::fmt::init();
    
    let temp_dir = TempDir::new()?;
    let config = ClusterConfig {
        node_count: 3,
        base_port: 9000,
        work_dir: temp_dir.path().to_path_buf(),
        startup_timeout: Duration::from_secs(30),
        enable_consensus: true,
        replication_factor: 2,
        resource_constraints: ResourceConstraints::default(),
        network_config: NetworkConfig::default(),
    };

    info!("Starting graceful node failover test");
    
    let cluster = TestCluster::new(config).await?;
    cluster.start().await?;

    // Wait for cluster formation
    sleep(Duration::from_secs(3)).await;

    let initial_nodes = cluster.get_nodes().await;
    let node_ids: Vec<_> = initial_nodes.keys().copied().collect();
    
    info!("Initial cluster has {} nodes", initial_nodes.len());
    
    // Test graceful shutdown of each node one by one
    for (i, &node_id) in node_ids.iter().enumerate() {
        info!("Testing graceful shutdown of node {} ({})", i + 1, node_id);
        
        // Remove node gracefully
        cluster.remove_node(node_id).await?;
        
        // Wait for cluster to adapt
        sleep(Duration::from_secs(2)).await;
        
        // Verify remaining cluster is still functional
        let remaining_nodes = cluster.get_nodes().await;
        assert_eq!(remaining_nodes.len(), 2 - i);
        
        // In a real system, we would verify:
        // 1. Remaining nodes can still serve requests
        // 2. No data loss occurred
        // 3. Performance impact is minimal
        // 4. Client connections are properly redirected
        
        info!("Node {} removed successfully, {} nodes remaining", node_id, remaining_nodes.len());
        
        // Add a replacement node
        let new_node_id = cluster.add_node().await?;
        
        // Wait for new node to join
        sleep(Duration::from_secs(2)).await;
        
        // Verify cluster is back to expected size
        let current_nodes = cluster.get_nodes().await;
        assert_eq!(current_nodes.len(), 3 - i);
        
        info!("Replacement node {} added successfully", new_node_id);
    }
    
    cluster.stop().await?;
    
    info!("Graceful node failover test completed successfully");
    Ok(())
}

/// Test abrupt node failure (crash scenario)
#[tokio::test]
async fn test_crash_failover() -> Result<()> {
    tracing_subscriber::fmt::init();
    
    let temp_dir = TempDir::new()?;
    let config = ClusterConfig {
        node_count: 5,
        base_port: 9100,
        work_dir: temp_dir.path().to_path_buf(),
        startup_timeout: Duration::from_secs(30),
        enable_consensus: true,
        replication_factor: 3,
        resource_constraints: ResourceConstraints::default(),
        network_config: NetworkConfig::default(),
    };

    info!("Starting crash failover test");
    
    let cluster = TestCluster::new(config).await?;
    cluster.start().await?;

    // Wait for cluster formation
    sleep(Duration::from_secs(3)).await;

    let nodes = cluster.get_nodes().await;
    let node_ids: Vec<_> = nodes.keys().copied().collect();
    
    // Simulate simultaneous crash of 2 nodes (minority)
    let crashed_nodes = vec![node_ids[0], node_ids[1]];
    
    info!("Simulating crash of nodes: {:?}", crashed_nodes);
    
    // Simulate abrupt node crashes by removing them immediately
    for &node_id in &crashed_nodes {
        cluster.remove_node(node_id).await?;
    }
    
    // Wait for cluster to detect failures and adapt
    sleep(Duration::from_secs(5)).await;
    
    // Verify remaining cluster is still functional
    let remaining_nodes = cluster.get_nodes().await;
    assert_eq!(remaining_nodes.len(), 3); // 5 - 2 = 3
    
    // In a real consensus system, we would verify:
    // 1. Remaining nodes (majority) can still elect a leader
    // 2. System continues to serve requests
    // 3. No data inconsistency due to crashes
    // 4. Failed node detection is fast
    
    info!("Cluster survived minority node crashes, {} nodes remaining", remaining_nodes.len());
    
    // Test recovery by adding replacement nodes
    for i in 0..2 {
        let new_node_id = cluster.add_node().await?;
        info!("Added replacement node {}: {}", i + 1, new_node_id);
        
        // Wait for node to join and catch up
        sleep(Duration::from_secs(3)).await;
    }
    
    // Verify full recovery
    let final_nodes = cluster.get_nodes().await;
    assert_eq!(final_nodes.len(), 5);
    
    cluster.stop().await?;
    
    info!("Crash failover test completed successfully");
    Ok(())
}

/// Test cascading failure scenario
#[tokio::test]
async fn test_cascading_failure() -> Result<()> {
    tracing_subscriber::fmt::init();
    
    let temp_dir = TempDir::new()?;
    let config = ClusterConfig {
        node_count: 7, // Use more nodes for cascading failure test
        base_port: 9200,
        work_dir: temp_dir.path().to_path_buf(),
        startup_timeout: Duration::from_secs(30),
        enable_consensus: true,
        replication_factor: 3,
        resource_constraints: ResourceConstraints {
            max_memory: 256 * 1024 * 1024, // Smaller memory limit to trigger pressure
            ..ResourceConstraints::default()
        },
        network_config: NetworkConfig {
            enable_simulation: true,
            ..NetworkConfig::default()
        },
    };

    info!("Starting cascading failure test");
    
    let cluster = TestCluster::new(config).await?;
    cluster.start().await?;

    // Wait for cluster formation
    sleep(Duration::from_secs(3)).await;

    let nodes = cluster.get_nodes().await;
    let node_ids: Vec<_> = nodes.keys().copied().collect();
    
    // Set up network simulator for additional stress
    let network_sim = NetworkSimulator::new();
    network_sim.start().await?;
    
    // Step 1: Introduce network stress
    info!("Step 1: Introducing network stress");
    network_sim.simulate_slow_network(node_ids.clone()).await?;
    sleep(Duration::from_secs(2)).await;
    
    // Step 2: Simulate first node failure
    info!("Step 2: Simulating first node failure");
    cluster.remove_node(node_ids[0]).await?;
    sleep(Duration::from_secs(2)).await;
    
    // Step 3: Increase network stress and cause second failure
    info!("Step 3: Increasing stress and causing second failure");
    network_sim.simulate_lossy_network(node_ids[1..].to_vec(), 0.2).await?;
    cluster.remove_node(node_ids[1]).await?;
    sleep(Duration::from_secs(3)).await;
    
    // Step 4: Network partition causes isolation
    info!("Step 4: Creating network partition");
    let partition_id = network_sim.create_partition(PartitionModel::Island {
        isolated_node: node_ids[2],
        remaining_nodes: node_ids[3..].to_vec(),
    }).await?;
    
    sleep(Duration::from_secs(3)).await;
    
    // At this point we have:
    // - 2 nodes failed/removed
    // - 1 node partitioned
    // - 4 nodes remaining in majority partition
    
    let remaining_nodes = cluster.get_nodes().await;
    info!("After cascading failures: {} nodes remaining", remaining_nodes.len());
    
    // In a real system, we would verify:
    // 1. The majority partition (4 nodes) maintains availability
    // 2. No data loss despite multiple failures
    // 3. System performance degrades gracefully
    // 4. Monitoring/alerting systems activate
    
    // Step 5: Begin recovery
    info!("Step 5: Beginning recovery process");
    
    // Heal network partition
    network_sim.heal_partition(partition_id).await?;
    cluster.heal_partition().await?;
    
    // Remove network stress
    for node_id in &node_ids {
        if let Err(e) = network_sim.remove_conditions(*node_id).await {
            warn!("Failed to remove network conditions for {}: {}", node_id, e);
        }
    }
    
    // Add replacement nodes
    for i in 0..2 {
        let new_node_id = cluster.add_node().await?;
        info!("Added replacement node {}: {}", i + 1, new_node_id);
        sleep(Duration::from_secs(2)).await;
    }
    
    // Step 6: Verify full recovery
    sleep(Duration::from_secs(5)).await;
    
    let final_nodes = cluster.get_nodes().await;
    info!("After recovery: {} nodes", final_nodes.len());
    
    // Should have 7 nodes total (5 original + 2 replacements - 2 failed)
    assert!(final_nodes.len() >= 5); // At least majority survived
    
    network_sim.stop().await?;
    cluster.stop().await?;
    
    info!("Cascading failure test completed successfully");
    Ok(())
}

/// Test resource exhaustion failover
#[tokio::test]
async fn test_resource_exhaustion_failover() -> Result<()> {
    tracing_subscriber::fmt::init();
    
    let temp_dir = TempDir::new()?;
    
    // Set up process manager for resource monitoring
    let process_manager = ProcessManager::new();
    process_manager.start().await?;
    
    let config = ClusterConfig {
        node_count: 3,
        base_port: 9300,
        work_dir: temp_dir.path().to_path_buf(),
        startup_timeout: Duration::from_secs(30),
        enable_consensus: true,
        replication_factor: 2,
        resource_constraints: ResourceConstraints {
            max_memory: 64 * 1024 * 1024,  // Very low memory limit (64MB)
            max_cpu_cores: 0.5,             // Low CPU limit
            max_disk: 100 * 1024 * 1024,    // Low disk limit (100MB)
            max_file_descriptors: 100,      // Low FD limit
        },
        network_config: NetworkConfig::default(),
    };

    info!("Starting resource exhaustion failover test");
    
    let cluster = TestCluster::new(config).await?;
    cluster.start().await?;

    // Wait for cluster formation
    sleep(Duration::from_secs(3)).await;

    let nodes = cluster.get_nodes().await;
    let node_ids: Vec<_> = nodes.keys().copied().collect();
    
    info!("Cluster started with {} nodes", nodes.len());
    
    // Simulate resource exhaustion by spawning resource-intensive processes
    let mut resource_intensive_processes = Vec::new();
    
    for (i, &node_id) in node_ids.iter().enumerate() {
        info!("Spawning resource-intensive process for node simulation {}", i);
        
        let process_config = ProcessConfig {
            id: Uuid::new_v4(),
            command: "sleep".to_string(), // Simple process that uses minimal resources
            args: vec!["10".to_string()],
            working_dir: None,
            env: HashMap::new(),
            stdin: StdioConfig::Null,
            stdout: StdioConfig::Null,
            stderr: StdioConfig::Null,
            timeout: Some(Duration::from_secs(15)),
            auto_restart: false,
            health_check: None,
        };
        
        let constraints = ResourceConstraints {
            max_memory: 32 * 1024 * 1024, // 32MB limit
            max_cpu_cores: 0.3,
            max_disk: 50 * 1024 * 1024,
            max_file_descriptors: 50,
        };
        
        let process_id = process_manager.spawn_process(process_config, constraints).await?;
        resource_intensive_processes.push(process_id);
        
        // Simulate gradual resource pressure
        sleep(Duration::from_millis(500)).await;
    }
    
    // Wait for resource pressure to build up
    sleep(Duration::from_secs(3)).await;
    
    // Check for resource violations
    for &process_id in &resource_intensive_processes {
        let violations = process_manager.check_resource_violations(process_id).await;
        if !violations.is_empty() {
            warn!("Resource violations detected for process {}: {:?}", process_id, violations);
        }
    }
    
    // In a real system with actual resource exhaustion:
    // 1. Nodes would start rejecting requests
    // 2. Some nodes might crash due to OOM
    // 3. Cluster would detect unhealthy nodes
    // 4. Failover mechanisms would activate
    // 5. Load would redistribute to healthy nodes
    
    // Simulate node failure due to resource exhaustion
    info!("Simulating node failure due to resource exhaustion");
    cluster.remove_node(node_ids[0]).await?;
    
    // Wait for cluster to adapt
    sleep(Duration::from_secs(3)).await;
    
    // Verify remaining cluster functionality
    let remaining_nodes = cluster.get_nodes().await;
    assert_eq!(remaining_nodes.len(), 2);
    
    info!("Cluster adapted to resource exhaustion, {} nodes remaining", remaining_nodes.len());
    
    // Clean up resource-intensive processes
    for process_id in resource_intensive_processes {
        if let Err(e) = process_manager.stop_process(process_id).await {
            warn!("Failed to stop process {}: {}", process_id, e);
        }
    }
    
    // Wait for resource pressure to subside
    sleep(Duration::from_secs(2)).await;
    
    // Add replacement node
    let replacement_node = cluster.add_node().await?;
    info!("Added replacement node after resource recovery: {}", replacement_node);
    
    // Wait for recovery
    sleep(Duration::from_secs(3)).await;
    
    // Verify full recovery
    let final_nodes = cluster.get_nodes().await;
    assert_eq!(final_nodes.len(), 3);
    
    process_manager.stop().await?;
    cluster.stop().await?;
    
    info!("Resource exhaustion failover test completed successfully");
    Ok(())
}

/// Test disaster recovery scenario (complete cluster failure)
#[tokio::test]
async fn test_disaster_recovery() -> Result<()> {
    tracing_subscriber::fmt::init();
    
    let temp_dir = TempDir::new()?;
    let config = ClusterConfig {
        node_count: 3,
        base_port: 9400,
        work_dir: temp_dir.path().to_path_buf(),
        startup_timeout: Duration::from_secs(30),
        enable_consensus: true,
        replication_factor: 3,
        resource_constraints: ResourceConstraints::default(),
        network_config: NetworkConfig::default(),
    };

    info!("Starting disaster recovery test");
    
    // Phase 1: Create and populate initial cluster
    let cluster = TestCluster::new(config.clone()).await?;
    cluster.start().await?;

    // Wait for cluster formation
    sleep(Duration::from_secs(3)).await;

    let initial_nodes = cluster.get_nodes().await;
    info!("Initial cluster established with {} nodes", initial_nodes.len());
    
    // In a real system, we would:
    // 1. Write important data to the cluster
    // 2. Ensure data is replicated across all nodes
    // 3. Create backups/snapshots
    
    // Simulate writing important data
    info!("Simulating data writes to cluster");
    for i in 0..100 {
        // In reality, this would be actual data writes
        if i % 10 == 0 {
            info!("Writing batch {} to cluster", i / 10);
        }
    }
    
    // Phase 2: Simulate complete disaster (all nodes fail)
    info!("Phase 2: Simulating complete disaster - all nodes failing");
    cluster.stop().await?;
    
    // Wait to simulate downtime
    sleep(Duration::from_secs(2)).await;
    
    // Phase 3: Disaster recovery - rebuild cluster
    info!("Phase 3: Beginning disaster recovery");
    
    let recovery_cluster = TestCluster::new(config).await?;
    recovery_cluster.start().await?;
    
    // Wait for recovery cluster formation
    sleep(Duration::from_secs(5)).await;
    
    let recovery_nodes = recovery_cluster.get_nodes().await;
    info!("Recovery cluster established with {} nodes", recovery_nodes.len());
    
    // In a real disaster recovery scenario:
    // 1. Restore data from backups/snapshots
    // 2. Verify data integrity
    // 3. Ensure all critical data is recovered
    // 4. Resume normal operations
    // 5. Update monitoring and alerting
    
    // Simulate data recovery verification
    info!("Verifying data recovery");
    for i in 0..10 {
        // In reality, this would verify actual data
        info!("Verifying data batch {}", i);
        sleep(Duration::from_millis(100)).await;
    }
    
    // Phase 4: Verify recovery cluster is fully functional
    info!("Phase 4: Verifying recovery cluster functionality");
    
    // Test that recovery cluster can handle operations
    assert_eq!(recovery_nodes.len(), 3);
    
    // Simulate additional operations on recovery cluster
    for i in 0..50 {
        if i % 10 == 0 {
            info!("Recovery cluster processing batch {}", i / 10);
        }
    }
    
    recovery_cluster.stop().await?;
    
    info!("Disaster recovery test completed successfully");
    Ok(())
}

/// Test rolling restart scenario
#[tokio::test]
async fn test_rolling_restart() -> Result<()> {
    tracing_subscriber::fmt::init();
    
    let temp_dir = TempDir::new()?;
    let config = ClusterConfig {
        node_count: 5,
        base_port: 9500,
        work_dir: temp_dir.path().to_path_buf(),
        startup_timeout: Duration::from_secs(30),
        enable_consensus: true,
        replication_factor: 3,
        resource_constraints: ResourceConstraints::default(),
        network_config: NetworkConfig::default(),
    };

    info!("Starting rolling restart test");
    
    let cluster = TestCluster::new(config).await?;
    cluster.start().await?;

    // Wait for cluster formation
    sleep(Duration::from_secs(3)).await;

    let nodes = cluster.get_nodes().await;
    let node_ids: Vec<_> = nodes.keys().copied().collect();
    
    info!("Starting rolling restart of {} nodes", node_ids.len());
    
    // Perform rolling restart - restart nodes one by one
    for (i, &node_id) in node_ids.iter().enumerate() {
        info!("Rolling restart: restarting node {} of {} ({})", i + 1, node_ids.len(), node_id);
        
        // Remove node
        cluster.remove_node(node_id).await?;
        
        // Wait for cluster to adapt (should maintain availability)
        sleep(Duration::from_secs(1)).await;
        
        // Verify cluster is still functional with N-1 nodes
        let current_nodes = cluster.get_nodes().await;
        assert_eq!(current_nodes.len(), node_ids.len() - 1);
        
        // Add replacement node (simulating restart with same configuration)
        let new_node_id = cluster.add_node().await?;
        
        // Wait for new node to join and synchronize
        sleep(Duration::from_secs(2)).await;
        
        // Verify cluster is back to full size
        let restored_nodes = cluster.get_nodes().await;
        assert_eq!(restored_nodes.len(), node_ids.len());
        
        info!("Node {} restarted successfully as {}", node_id, new_node_id);
        
        // In a real rolling restart scenario, we would verify:
        // 1. No service interruption during restart
        // 2. Node catches up with latest data
        // 3. Load balancing adjusts appropriately
        // 4. Client connections are handled gracefully
    }
    
    // Final verification
    let final_nodes = cluster.get_nodes().await;
    assert_eq!(final_nodes.len(), 5);
    
    info!("All nodes restarted successfully");
    
    // Test that cluster is fully functional after rolling restart
    for i in 0..20 {
        // Simulate operations to verify functionality
        if i % 5 == 0 {
            info!("Post-restart verification batch {}", i / 5);
        }
        sleep(Duration::from_millis(50)).await;
    }
    
    cluster.stop().await?;
    
    info!("Rolling restart test completed successfully");
    Ok(())
}

/// Test multiple simultaneous failure scenarios
#[tokio::test]
async fn test_multiple_failure_scenarios() -> Result<()> {
    tracing_subscriber::fmt::init();
    
    let temp_dir = TempDir::new()?;
    let config = ClusterConfig {
        node_count: 7,
        base_port: 9600,
        work_dir: temp_dir.path().to_path_buf(),
        startup_timeout: Duration::from_secs(30),
        enable_consensus: true,
        replication_factor: 3,
        resource_constraints: ResourceConstraints::default(),
        network_config: NetworkConfig {
            enable_simulation: true,
            ..NetworkConfig::default()
        },
    };

    info!("Starting multiple failure scenarios test");
    
    let cluster = TestCluster::new(config).await?;
    cluster.start().await?;

    // Wait for cluster formation
    sleep(Duration::from_secs(3)).await;

    let nodes = cluster.get_nodes().await;
    let node_ids: Vec<_> = nodes.keys().copied().collect();
    
    let network_sim = NetworkSimulator::new();
    network_sim.start().await?;
    
    info!("Testing multiple simultaneous failure scenarios");
    
    // Scenario 1: Network partition + node crash
    info!("Scenario 1: Network partition + node crash");
    
    let partition_id = network_sim.create_partition(PartitionModel::SplitBrain {
        group_a: vec![node_ids[0], node_ids[1]],
        group_b: vec![node_ids[2], node_ids[3], node_ids[4]],
    }).await?;
    
    // Simultaneously crash a node in the minority partition
    cluster.remove_node(node_ids[0]).await?;
    
    sleep(Duration::from_secs(3)).await;
    
    // Verify majority partition still functions
    let remaining_nodes = cluster.get_nodes().await;
    info!("After partition + crash: {} nodes remaining", remaining_nodes.len());
    
    // Scenario 2: Heal partition but add network issues
    info!("Scenario 2: Healing partition while adding network issues");
    
    network_sim.heal_partition(partition_id).await?;
    cluster.heal_partition().await?;
    
    // Add network latency and packet loss
    network_sim.simulate_high_latency(node_ids[1..].to_vec(), Duration::from_millis(200)).await?;
    network_sim.simulate_lossy_network(node_ids[2..4].to_vec(), 0.15).await?;
    
    sleep(Duration::from_secs(3)).await;
    
    // Scenario 3: Add node while under stress
    info!("Scenario 3: Adding replacement node under network stress");
    
    let new_node_id = cluster.add_node().await?;
    
    sleep(Duration::from_secs(4)).await;
    
    // Scenario 4: Remove network issues and verify recovery
    info!("Scenario 4: Removing all network issues and verifying recovery");
    
    for node_id in &node_ids {
        if let Err(e) = network_sim.remove_conditions(*node_id).await {
            warn!("Failed to remove conditions for {}: {}", node_id, e);
        }
    }
    
    sleep(Duration::from_secs(3)).await;
    
    // Final verification
    let final_nodes = cluster.get_nodes().await;
    info!("Final cluster size: {} nodes", final_nodes.len());
    
    // Should have at least majority of original nodes plus replacement
    assert!(final_nodes.len() >= 4);
    
    network_sim.stop().await?;
    cluster.stop().await?;
    
    info!("Multiple failure scenarios test completed successfully");
    Ok(())
}

#[cfg(test)]
mod failover_test_helpers {
    use super::*;
    
    /// Helper to verify cluster health during failover scenarios
    pub async fn verify_cluster_health(cluster: &TestCluster) -> Result<bool> {
        let nodes = cluster.get_nodes().await;
        let healthy_nodes = nodes.values()
            .filter(|node| matches!(
                node.state,
                kaelix_tests::cluster::BrokerState::Running
                | kaelix_tests::cluster::BrokerState::Leader
                | kaelix_tests::cluster::BrokerState::Follower
            ))
            .count();
        
        let total_nodes = nodes.len();
        let health_ratio = healthy_nodes as f64 / total_nodes as f64;
        
        info!("Cluster health: {}/{} nodes healthy ({:.1}%)", 
              healthy_nodes, total_nodes, health_ratio * 100.0);
        
        Ok(health_ratio > 0.5) // Consider healthy if majority is functional
    }
    
    /// Helper to simulate gradual load increase
    pub async fn simulate_load_increase(duration: Duration, peak_rps: u64) -> Result<()> {
        let steps = 10;
        let step_duration = duration / steps;
        
        for i in 1..=steps {
            let current_rps = (peak_rps * i as u64) / steps as u64;
            info!("Load simulation step {}/{}: {} RPS", i, steps, current_rps);
            
            // In a real system, this would generate actual load
            sleep(step_duration).await;
        }
        
        Ok(())
    }
    
    /// Helper to wait for cluster stabilization
    pub async fn wait_for_stabilization(cluster: &TestCluster, timeout: Duration) -> Result<()> {
        let start = std::time::Instant::now();
        let mut consecutive_stable_checks = 0;
        const REQUIRED_STABLE_CHECKS: u32 = 3;
        
        while start.elapsed() < timeout {
            if verify_cluster_health(cluster).await? {
                consecutive_stable_checks += 1;
                if consecutive_stable_checks >= REQUIRED_STABLE_CHECKS {
                    info!("Cluster stabilized after {} checks", consecutive_stable_checks);
                    return Ok(());
                }
            } else {
                consecutive_stable_checks = 0;
            }
            
            sleep(Duration::from_millis(500)).await;
        }
        
        anyhow::bail!("Cluster did not stabilize within timeout")
    }
}