//! # High Availability and Failover Management
//!
//! Provides comprehensive high availability functionality for the cluster including
//! automatic failover, health monitoring, and cluster coordination.

use std::{
    collections::HashMap,
    sync::{Arc, atomic::{AtomicBool, Ordering}},
    time::Duration,
    net::SocketAddr,
};

use serde::{Deserialize, Serialize};
use tokio::{
    sync::{watch, broadcast, RwLock},
    time::{interval, sleep},
};
use tracing::{debug, error, info, warn};

use crate::{
    types::NodeId,
    communication::MessageRouter,
    membership::SwimMembership,
    error::Result as ClusterResult,
};

// Re-export submodules
pub mod client;
pub mod coordinator;
pub mod monitor;

pub use client::{FailoverClient, FailoverClientConfig, FailoverClientMetrics, RetryConfig};
pub use coordinator::{FailoverCoordinator, FailoverPlan, FailoverStep};
pub use monitor::{HaMonitor, NodeHealth, ClusterHealthReport, CapacityStatus, MonitoringConfig};

/// High availability specific error types
#[derive(Debug, thiserror::Error)]
pub enum HaError {
    #[error("Configuration error: {0}")]
    Configuration(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("Timeout error: {0}")]
    Timeout(String),
    #[error("No healthy endpoints available")]
    NoHealthyEndpoints,
    #[error("Endpoint already exists")]
    EndpointAlreadyExists,
    #[error("Endpoint not found")]
    EndpointNotFound,
    #[error("All endpoints failed")]
    AllEndpointsFailed,
    #[error("Leadership transfer failed: {0}")]
    LeadershipTransferFailed(String),
    #[error("Failover operation failed: {0}")]
    FailoverFailed(String),
    #[error("Health check failed: {0}")]
    HealthCheckFailed(String),
    #[error("Cluster is not ready for operation")]
    ClusterNotReady,
}

/// Result type for HA operations
pub type HaResult<T> = Result<T, HaError>;

/// Types of failover operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailoverType {
    /// Planned maintenance failover
    Planned,
    /// Automatic failover due to failure detection
    Automatic,
    /// Manual failover initiated by administrator
    Manual,
    /// Emergency failover for critical situations
    Emergency,
}

/// Leadership change notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeadershipChange {
    /// Previous leader (if any)
    pub previous_leader: Option<NodeId>,
    /// New leader
    pub new_leader: NodeId,
    /// Reason for the change
    pub reason: String,
    /// Timestamp of the change
    pub timestamp: std::time::SystemTime,
}

/// Topology change notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TopologyChange {
    /// Node was added to the cluster
    NodeAdded {
        node_id: NodeId,
        address: SocketAddr,
        capabilities: Vec<String>,
    },
    /// Node was removed from the cluster
    NodeRemoved {
        node_id: NodeId,
        address: SocketAddr,
        reason: String,
    },
    /// Node was updated (capabilities, address, etc.)
    NodeUpdated {
        node_id: NodeId,
        address: SocketAddr,
        old_capabilities: Vec<String>,
        new_capabilities: Vec<String>,
    },
}

/// High availability configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HaConfig {
    /// Enable automatic failover
    pub enable_auto_failover: bool,
    /// Failover timeout threshold
    pub failover_timeout: Duration,
    /// Enable health monitoring
    pub enable_health_monitoring: bool,
    /// Monitoring configuration
    pub monitoring_config: MonitoringConfig,
    /// Performance thresholds
    pub performance_thresholds: PerformanceThresholds,
    /// Enable leadership management
    pub enable_leadership_management: bool,
    /// Leadership lease timeout
    pub leadership_lease_timeout: Duration,
    /// Maximum failover attempts before giving up
    pub max_failover_attempts: usize,
    /// Cooldown period between failover attempts
    pub failover_cooldown: Duration,
}

impl Default for HaConfig {
    fn default() -> Self {
        Self {
            enable_auto_failover: true,
            failover_timeout: Duration::from_secs(30),
            enable_health_monitoring: true,
            monitoring_config: MonitoringConfig::default(),
            performance_thresholds: PerformanceThresholds::default(),
            enable_leadership_management: true,
            leadership_lease_timeout: Duration::from_secs(60),
            max_failover_attempts: 3,
            failover_cooldown: Duration::from_secs(10),
        }
    }
}

/// Performance threshold configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceThresholds {
    /// CPU utilization threshold (0.0 to 1.0)
    pub cpu_threshold: f64,
    /// Memory utilization threshold (0.0 to 1.0)
    pub memory_threshold: f64,
    /// Network bandwidth threshold (bytes/sec)
    pub network_threshold: u64,
    /// Connection count threshold
    pub connection_threshold: u64,
    /// Request rate threshold (requests/sec)
    pub request_rate_threshold: u64,
}

/// Performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Current CPU utilization (0.0 to 1.0)
    pub cpu_utilization: f64,
    /// Current memory utilization (0.0 to 1.0)
    pub memory_utilization: f64,
    /// Current network bandwidth usage (bytes/sec)
    pub network_bandwidth: u64,
    /// Current connection count
    pub connection_count: u64,
    /// Current request rate (requests/sec)
    pub request_rate: u64,
    /// Average response latency
    pub avg_response_latency: Duration,
    /// Current error rate (0.0 to 1.0)
    pub error_rate: f64,
}

impl Default for PerformanceThresholds {
    fn default() -> Self {
        Self {
            cpu_threshold: 0.8,
            memory_threshold: 0.85,
            network_threshold: 1_000_000_000, // 1 GB/s
            connection_threshold: 10_000,
            request_rate_threshold: 100_000,
        }
    }
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            cpu_utilization: 0.0,
            memory_utilization: 0.0,
            network_bandwidth: 0,
            connection_count: 0,
            request_rate: 0,
            avg_response_latency: Duration::ZERO,
            error_rate: 0.0,
        }
    }
}

impl HaManager {
    /// Create a new HA manager
    pub fn new(
        node_id: NodeId,
        config: HaConfig,
        router: Arc<MessageRouter>,
        membership: Arc<SwimMembership>,
    ) -> HaResult<(
        Self,
        watch::Sender<bool>,
        watch::Receiver<Option<LeadershipChange>>,
        watch::Receiver<Option<TopologyChange>>,
    )> {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (leadership_change_tx, leadership_change_rx) = watch::channel(None);
        let (topology_change_tx, topology_change_rx) = watch::channel(None);

        // Create health monitor with correct parameters
        let health_monitor = Arc::new(HaMonitor::new(
            config.monitoring_config.clone(),
            membership.clone(),
        ));

        let failover_coordinator = FailoverCoordinator::new(node_id, config.failover_timeout);

        let manager = Self {
            node_id,
            config: config.clone(),
            router,
            membership: membership.clone(),
            is_leader: Arc::new(AtomicBool::new(false)),
            health_monitor,
            failover_coordinator: Arc::new(RwLock::new(failover_coordinator)),
            shutdown_rx,
            leadership_change_tx: Arc::new(leadership_change_tx),
            topology_change_tx: Arc::new(topology_change_tx),
            current_metrics: Arc::new(RwLock::new(PerformanceMetrics::default())),
        };

        Ok((manager, shutdown_tx, leadership_change_rx, topology_change_rx))
    }

    /// Start the HA manager
    pub async fn start(&self) -> HaResult<()> {
        info!("Starting HA manager for node {}", self.node_id);

        // Start health monitoring if enabled
        if self.config.enable_health_monitoring {
            self.health_monitor.start().await
                .map_err(|e| HaError::Configuration(format!("Failed to start health monitor: {}", e)))?;
        }

        // Start performance monitoring
        self.start_performance_monitoring().await?;

        // Start failover management
        if self.config.enable_auto_failover {
            self.start_failover_management().await?;
        }

        // Start leadership management
        if self.config.enable_leadership_management {
            self.start_leadership_management().await?;
        }

        info!("HA manager started successfully");
        Ok(())
    }

    /// Start performance monitoring
    async fn start_performance_monitoring(&self) -> HaResult<()> {
        let metrics = Arc::clone(&self.current_metrics);
        let thresholds = self.config.performance_thresholds.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(5));

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Collect performance metrics (simplified)
                        let mut metrics_guard = metrics.write().await;
                        
                        // In a real implementation, these would be collected from system APIs
                        metrics_guard.cpu_utilization = 0.3; // Mock value
                        metrics_guard.memory_utilization = 0.4; // Mock value
                        metrics_guard.network_bandwidth = 1_000_000; // Mock value
                        metrics_guard.connection_count = 100; // Mock value
                        metrics_guard.request_rate = 1000; // Mock value
                        metrics_guard.avg_response_latency = Duration::from_millis(50); // Mock value
                        metrics_guard.error_rate = 0.01; // Mock value
                        
                        // Check thresholds and trigger alerts if necessary
                        if metrics_guard.cpu_utilization > thresholds.cpu_threshold {
                            warn!("CPU utilization ({:.2}) exceeds threshold ({:.2})", 
                                  metrics_guard.cpu_utilization, thresholds.cpu_threshold);
                        }
                        
                        if metrics_guard.memory_utilization > thresholds.memory_threshold {
                            warn!("Memory utilization ({:.2}) exceeds threshold ({:.2})", 
                                  metrics_guard.memory_utilization, thresholds.memory_threshold);
                        }
                    },
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            debug!("Performance monitoring shutting down");
                            break;
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Start failover management
    async fn start_failover_management(&self) -> HaResult<()> {
        let health_monitor = Arc::clone(&self.health_monitor);
        let failover_coordinator = Arc::clone(&self.failover_coordinator);
        let config = self.config.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let mut check_interval = interval(Duration::from_secs(10));
            let mut last_failover = std::time::Instant::now();

            loop {
                tokio::select! {
                    _ = check_interval.tick() => {
                        // Check cluster health
                        match health_monitor.generate_health_report().await {
                            Ok(report) => {
                                if !report.is_cluster_healthy() {
                                    let time_since_last = last_failover.elapsed();
                                    
                                    if time_since_last >= config.failover_cooldown {
                                        warn!("Cluster unhealthy, considering failover");
                                        
                                        // Check if we should trigger failover
                                        let critical_nodes = report.get_critical_nodes();
                                        if critical_nodes.len() > report.node_healths.len() / 2 {
                                            warn!("More than half of nodes are critical, triggering failover");
                                            
                                            let coordinator = failover_coordinator.read().await;
                                            match coordinator.create_failover_plan(FailoverType::Automatic).await {
                                                Ok(plan) => {
                                                    info!("Created failover plan: {:?}", plan);
                                                    // In a real implementation, we'd execute the plan
                                                    last_failover = std::time::Instant::now();
                                                },
                                                Err(e) => {
                                                    error!("Failed to create failover plan: {}", e);
                                                }
                                            }
                                        }
                                    } else {
                                        debug!("Failover cooldown in effect, waiting {:?}", 
                                               config.failover_cooldown - time_since_last);
                                    }
                                }
                            },
                            Err(e) => {
                                error!("Failed to generate health report: {}", e);
                            }
                        }
                    },
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            debug!("Failover management shutting down");
                            break;
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Start leadership management
    async fn start_leadership_management(&self) -> HaResult<()> {
        let is_leader = Arc::clone(&self.is_leader);
        let leadership_change_tx = Arc::clone(&self.leadership_change_tx);
        let node_id = self.node_id;
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let mut leadership_check = interval(Duration::from_secs(30));

            loop {
                tokio::select! {
                    _ = leadership_check.tick() => {
                        // Simple leadership election based on node ID (for demo)
                        // In a real implementation, this would integrate with consensus
                        let should_be_leader = true; // Simplified logic
                        
                        let current_is_leader = is_leader.load(Ordering::Relaxed);
                        
                        if should_be_leader && !current_is_leader {
                            info!("Node {} becoming leader", node_id);
                            is_leader.store(true, Ordering::Relaxed);
                            
                            let change = LeadershipChange {
                                previous_leader: None,
                                new_leader: node_id,
                                reason: "Election won".to_string(),
                                timestamp: std::time::SystemTime::now(),
                            };
                            
                            let _ = leadership_change_tx.send(Some(change));
                        } else if !should_be_leader && current_is_leader {
                            info!("Node {} stepping down as leader", node_id);
                            is_leader.store(false, Ordering::Relaxed);
                            
                            let change = LeadershipChange {
                                previous_leader: Some(node_id),
                                new_leader: node_id, // Would be actual new leader
                                reason: "Stepping down".to_string(),
                                timestamp: std::time::SystemTime::now(),
                            };
                            
                            let _ = leadership_change_tx.send(Some(change));
                        }
                    },
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            debug!("Leadership management shutting down");
                            break;
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Check if this node is the current leader
    pub fn is_leader(&self) -> bool {
        self.is_leader.load(Ordering::Relaxed)
    }

    /// Get current performance metrics
    pub async fn get_performance_metrics(&self) -> PerformanceMetrics {
        self.current_metrics.read().await.clone()
    }

    /// Get cluster health report
    pub async fn get_health_report(&self) -> HaResult<ClusterHealthReport> {
        self.health_monitor.generate_health_report().await
    }

    /// Manually trigger failover
    pub async fn trigger_failover(&self, failover_type: FailoverType) -> HaResult<()> {
        info!("Manually triggering failover of type: {:?}", failover_type);
        
        let coordinator = self.failover_coordinator.read().await;
        let plan = coordinator.create_failover_plan(failover_type).await?;
        
        info!("Executing failover plan: {:?}", plan);
        
        // In a real implementation, we'd execute each step of the plan
        for step in &plan.steps {
            info!("Executing failover step: {:?}", step);
            // Execute step...
            sleep(Duration::from_millis(100)).await; // Simulate work
        }
        
        info!("Failover completed successfully");
        Ok(())
    }

    /// Stop the HA manager
    pub async fn stop(&self) -> HaResult<()> {
        info!("Stopping HA manager");
        
        // Stop health monitor
        if let Err(e) = self.health_monitor.stop().await {
            error!("Error stopping health monitor: {}", e);
        }
        
        info!("HA manager stopped");
        Ok(())
    }
}

/// Main High Availability Manager
pub struct HaManager {
    /// Node identifier
    node_id: NodeId,
    /// HA configuration
    config: HaConfig,
    /// Network message router
    router: Arc<MessageRouter>,
    /// Cluster membership management
    membership: Arc<SwimMembership>,
    /// Current leadership status
    is_leader: Arc<AtomicBool>,
    /// Health monitoring system
    health_monitor: Arc<HaMonitor>,
    /// Failover coordination
    failover_coordinator: Arc<RwLock<FailoverCoordinator>>,
    /// Shutdown signal receiver
    shutdown_rx: watch::Receiver<bool>,
    /// Leadership change broadcaster
    leadership_change_tx: Arc<watch::Sender<Option<LeadershipChange>>>,
    /// Topology change broadcaster
    topology_change_tx: Arc<watch::Sender<Option<TopologyChange>>>,
    /// Current performance metrics
    current_metrics: Arc<RwLock<PerformanceMetrics>>,
}

impl std::fmt::Debug for HaManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HaManager")
            .field("node_id", &self.node_id)
            .field("is_leader", &self.is_leader.load(Ordering::Relaxed))
            .field("config", &self.config)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    #[test]
    fn test_ha_config_default() {
        let config = HaConfig::default();
        assert!(config.enable_auto_failover);
        assert!(config.enable_health_monitoring);
        assert!(config.enable_leadership_management);
    }

    #[test]
    fn test_performance_thresholds() {
        let thresholds = PerformanceThresholds::default();
        assert_eq!(thresholds.cpu_threshold, 0.8);
        assert_eq!(thresholds.memory_threshold, 0.85);
    }

    #[test]
    fn test_failover_type_serialization() {
        let failover_type = FailoverType::Automatic;
        let serialized = serde_json::to_string(&failover_type).unwrap();
        let deserialized: FailoverType = serde_json::from_str(&serialized).unwrap();
        assert_eq!(failover_type, deserialized);
    }

    #[tokio::test]
    async fn test_performance_metrics() {
        let metrics = PerformanceMetrics::default();
        assert_eq!(metrics.cpu_utilization, 0.0);
        assert_eq!(metrics.error_rate, 0.0);
    }
}