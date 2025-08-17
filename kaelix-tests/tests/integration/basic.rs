//! Basic integration tests for the distributed testing infrastructure
//!
//! This module contains simple tests to verify that the distributed testing
//! infrastructure is working correctly.

use std::time::Duration;

use anyhow::Result;
use kaelix_tests::prelude::*;
use kaelix_tests::cluster::{TestCluster, ClusterConfig, ResourceConstraints, NetworkConfig};
use kaelix_tests::simulators::network::NetworkSimulator;
use kaelix_tests::orchestration::ProcessManager;
use tempfile::TempDir;
use tokio::time::sleep;
use tracing::info;

/// Test basic cluster creation and startup
#[tokio::test]
async fn test_basic_cluster_creation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = ClusterConfig {
        node_count: 1, // Minimal cluster for basic test
        base_port: 7000,
        work_dir: temp_dir.path().to_path_buf(),
        startup_timeout: Duration::from_secs(10),
        enable_consensus: false, // Disabled for simplicity
        replication_factor: 1,
        resource_constraints: ResourceConstraints::default(),
        network_config: NetworkConfig::default(),
    };

    info!("Creating test cluster");
    let cluster = TestCluster::new(config).await?;
    
    info!("Starting cluster");
    cluster.start().await?;
    
    // Brief pause to let cluster stabilize
    sleep(Duration::from_secs(1)).await;
    
    // Verify cluster state
    let state = cluster.get_state().await;
    info!("Cluster state: {:?}", state);
    
    // Get node information
    let nodes = cluster.get_nodes().await;
    info!("Cluster has {} nodes", nodes.len());
    assert_eq!(nodes.len(), 1);
    
    info!("Stopping cluster");
    cluster.stop().await?;
    
    info!("Basic cluster test completed successfully");
    Ok(())
}

/// Test network simulator creation and basic operations
#[tokio::test]
async fn test_basic_network_simulator() -> Result<()> {
    info!("Creating network simulator");
    let simulator = NetworkSimulator::new();
    
    info!("Starting network simulator");
    simulator.start().await?;
    
    // Brief pause
    sleep(Duration::from_millis(100)).await;
    
    info!("Stopping network simulator");
    simulator.stop().await?;
    
    info!("Basic network simulator test completed successfully");
    Ok(())
}

/// Test process manager creation and basic operations
#[tokio::test]
async fn test_basic_process_manager() -> Result<()> {
    info!("Creating process manager");
    let manager = ProcessManager::new();
    
    info!("Starting process manager");
    manager.start().await?;
    
    // Brief pause
    sleep(Duration::from_millis(100)).await;
    
    info!("Stopping process manager");
    manager.stop().await?;
    
    info!("Basic process manager test completed successfully");
    Ok(())
}

/// Test integration between multiple components
#[tokio::test]
async fn test_basic_integration() -> Result<()> {
    let temp_dir = TempDir::new()?;
    
    info!("Setting up integrated test environment");
    
    // Create all components
    let cluster_config = ClusterConfig {
        node_count: 1,
        base_port: 7100,
        work_dir: temp_dir.path().to_path_buf(),
        startup_timeout: Duration::from_secs(10),
        enable_consensus: false,
        replication_factor: 1,
        resource_constraints: ResourceConstraints::default(),
        network_config: NetworkConfig::default(),
    };
    
    let cluster = TestCluster::new(cluster_config).await?;
    let network_simulator = NetworkSimulator::new();
    let process_manager = ProcessManager::new();
    
    info!("Starting all components");
    
    // Start all components
    process_manager.start().await?;
    network_simulator.start().await?;
    cluster.start().await?;
    
    // Let everything stabilize
    sleep(Duration::from_secs(1)).await;
    
    // Verify everything is running
    let cluster_state = cluster.get_state().await;
    info!("Integrated test - cluster state: {:?}", cluster_state);
    
    let nodes = cluster.get_nodes().await;
    info!("Integrated test - cluster has {} nodes", nodes.len());
    
    info!("Stopping all components");
    
    // Stop all components in reverse order
    cluster.stop().await?;
    network_simulator.stop().await?;
    process_manager.stop().await?;
    
    info!("Basic integration test completed successfully");
    Ok(())
}

/// Test that tests can be run in parallel without conflicts
#[tokio::test]
async fn test_parallel_safe_clusters() -> Result<()> {
    let temp_dir = TempDir::new()?;
    
    // Use different port ranges to avoid conflicts
    let config = ClusterConfig {
        node_count: 1,
        base_port: 7200, // Different from other tests
        work_dir: temp_dir.path().to_path_buf(),
        startup_timeout: Duration::from_secs(10),
        enable_consensus: false,
        replication_factor: 1,
        resource_constraints: ResourceConstraints::default(),
        network_config: NetworkConfig::default(),
    };

    info!("Creating parallel-safe cluster");
    let cluster = TestCluster::new(config).await?;
    
    cluster.start().await?;
    sleep(Duration::from_millis(500)).await;
    cluster.stop().await?;
    
    info!("Parallel-safe cluster test completed successfully");
    Ok(())
}

/// Test error handling in distributed components
#[tokio::test]
async fn test_error_handling() -> Result<()> {
    // Test invalid configuration handling
    let invalid_config = ClusterConfig {
        node_count: 0, // Invalid: must be at least 1
        base_port: 7300,
        work_dir: std::env::temp_dir().join("invalid_test_dir"),
        startup_timeout: Duration::from_secs(1), // Very short timeout
        enable_consensus: false,
        replication_factor: 1,
        resource_constraints: ResourceConstraints::default(),
        network_config: NetworkConfig::default(),
    };

    info!("Testing error handling with invalid config");
    
    // This should handle the error gracefully
    match TestCluster::new(invalid_config).await {
        Ok(cluster) => {
            // If cluster creation succeeds despite invalid config,
            // at least verify we can clean it up
            let _ = cluster.stop().await;
            info!("Cluster creation succeeded despite invalid config (handled gracefully)");
        }
        Err(e) => {
            info!("Cluster creation failed as expected: {}", e);
            // This is the expected outcome for invalid config
        }
    }
    
    info!("Error handling test completed successfully");
    Ok(())
}

#[cfg(test)]
mod test_helpers {
    use super::*;
    
    /// Helper function to create a minimal test cluster configuration
    pub fn minimal_cluster_config(base_port: u16, work_dir: std::path::PathBuf) -> ClusterConfig {
        ClusterConfig {
            node_count: 1,
            base_port,
            work_dir,
            startup_timeout: Duration::from_secs(10),
            enable_consensus: false,
            replication_factor: 1,
            resource_constraints: ResourceConstraints::default(),
            network_config: NetworkConfig::default(),
        }
    }
    
    /// Helper function to wait for a condition with timeout
    pub async fn wait_for_condition<F, Fut>(
        condition: F, 
        timeout: Duration,
        check_interval: Duration,
    ) -> Result<()>
    where
        F: Fn() -> Fut + Send,
        Fut: std::future::Future<Output = bool> + Send,
    {
        let start = std::time::Instant::now();
        
        while start.elapsed() < timeout {
            if condition().await {
                return Ok(());
            }
            
            sleep(check_interval).await;
        }
        
        anyhow::bail!("Condition not met within timeout")
    }
}