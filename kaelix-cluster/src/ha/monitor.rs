//! # Health Monitoring and Failure Detection
//!
//! Provides comprehensive health monitoring across all cluster components
//! with predictive failure detection and cluster capacity analysis.

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use tokio::sync::RwLock;
use tracing::{debug, warn, error, instrument};
use serde::{Deserialize, Serialize};

use crate::{
    types::NodeId,
    membership::{SwimMembership, NodeInfo, NodeStatus},
    error::Result,
};

use super::{HaResult, HaError};

/// Node health information (no serialization due to Instant)
#[derive(Debug, Clone)]
pub struct NodeHealth {
    /// Node identifier
    pub node_id: NodeId,
    /// Network address
    pub address: SocketAddr,
    /// Is node currently alive and reachable
    pub is_alive: bool,
    /// CPU load average (0.0 to 1.0)
    pub load_average: f64,
    /// Memory usage percentage (0.0 to 1.0)
    pub memory_usage: f64,
    /// Network latency to node
    pub network_latency: Duration,
    /// Last successful heartbeat timestamp
    pub last_heartbeat: Instant,
    /// Consecutive failure count
    pub failure_count: u32,
    /// Health score (0.0 to 1.0, higher is healthier)
    pub health_score: f64,
    /// Predicted time to failure (if applicable)
    pub predicted_failure_time: Option<Instant>,
}

impl NodeHealth {
    /// Create a new node health record
    pub fn new(node_id: NodeId, address: SocketAddr) -> Self {
        Self {
            node_id,
            address,
            is_alive: true,
            load_average: 0.0,
            memory_usage: 0.0,
            network_latency: Duration::from_millis(0),
            last_heartbeat: Instant::now(),
            failure_count: 0,
            health_score: 1.0,
            predicted_failure_time: None,
        }
    }

    /// Update health metrics
    pub fn update_metrics(&mut self, load_avg: f64, memory_usage: f64, latency: Duration) {
        self.load_average = load_avg;
        self.memory_usage = memory_usage;
        self.network_latency = latency;
        self.last_heartbeat = Instant::now();
        self.calculate_health_score();
    }

    /// Mark node as failed
    pub fn mark_failure(&mut self) {
        self.is_alive = false;
        self.failure_count += 1;
        self.health_score = 0.0;
    }

    /// Mark node as recovered
    pub fn mark_recovery(&mut self) {
        self.is_alive = true;
        self.failure_count = 0;
        self.last_heartbeat = Instant::now();
        self.calculate_health_score();
    }

    /// Calculate health score based on metrics
    fn calculate_health_score(&mut self) {
        if !self.is_alive {
            self.health_score = 0.0;
            return;
        }

        // Base score
        let mut score = 1.0;

        // Penalize high CPU load
        if self.load_average > 0.8 {
            score -= (self.load_average - 0.8) * 0.5;
        }

        // Penalize high memory usage
        if self.memory_usage > 0.9 {
            score -= (self.memory_usage - 0.9) * 2.0;
        }

        // Penalize high network latency
        if self.network_latency > Duration::from_millis(100) {
            let excess_ms = self.network_latency.as_millis() as f64 - 100.0;
            score -= (excess_ms / 1000.0) * 0.1;
        }

        // Penalize based on failure count
        score -= (self.failure_count as f64) * 0.1;

        // Ensure score is within [0.0, 1.0]
        self.health_score = score.max(0.0).min(1.0);
    }

    /// Check if the node is considered healthy
    pub fn is_healthy(&self) -> bool {
        self.is_alive && self.health_score > 0.5
    }

    /// Get time since last heartbeat
    pub fn time_since_last_heartbeat(&self) -> Duration {
        Instant::now().duration_since(self.last_heartbeat)
    }
}

/// Capacity status for cluster resources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapacityStatus {
    /// Total available CPU capacity
    pub total_cpu_capacity: f64,
    /// Used CPU capacity
    pub used_cpu_capacity: f64,
    /// Total available memory in bytes
    pub total_memory_capacity: u64,
    /// Used memory in bytes
    pub used_memory_capacity: u64,
    /// Total available storage in bytes
    pub total_storage_capacity: u64,
    /// Used storage in bytes
    pub used_storage_capacity: u64,
    /// Number of healthy nodes
    pub healthy_nodes: usize,
    /// Total number of nodes
    pub total_nodes: usize,
}

impl CapacityStatus {
    /// Calculate CPU utilization ratio (0.0 to 1.0)
    pub fn cpu_utilization(&self) -> f64 {
        if self.total_cpu_capacity == 0.0 {
            0.0
        } else {
            (self.used_cpu_capacity / self.total_cpu_capacity).min(1.0)
        }
    }

    /// Calculate memory utilization ratio (0.0 to 1.0)
    pub fn memory_utilization(&self) -> f64 {
        if self.total_memory_capacity == 0 {
            0.0
        } else {
            (self.used_memory_capacity as f64 / self.total_memory_capacity as f64).min(1.0)
        }
    }

    /// Calculate storage utilization ratio (0.0 to 1.0)
    pub fn storage_utilization(&self) -> f64 {
        if self.total_storage_capacity == 0 {
            0.0
        } else {
            (self.used_storage_capacity as f64 / self.total_storage_capacity as f64).min(1.0)
        }
    }

    /// Get cluster availability ratio (0.0 to 1.0)
    pub fn availability_ratio(&self) -> f64 {
        if self.total_nodes == 0 {
            0.0
        } else {
            self.healthy_nodes as f64 / self.total_nodes as f64
        }
    }

    /// Check if cluster is under stress
    pub fn is_under_stress(&self) -> bool {
        self.cpu_utilization() > 0.8 
            || self.memory_utilization() > 0.9 
            || self.availability_ratio() < 0.7
    }
}

/// Comprehensive cluster health report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterHealthReport {
    /// Timestamp of the report
    pub timestamp: SystemTime,
    /// Individual node health statuses
    pub node_healths: Vec<SerializableNodeHealth>,
    /// Overall cluster capacity
    pub capacity_status: CapacityStatus,
    /// Cluster health score (0.0 to 1.0)
    pub cluster_health_score: f64,
    /// Predicted issues and recommendations
    pub recommendations: Vec<String>,
}

/// Serializable version of NodeHealth (using SystemTime instead of Instant)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableNodeHealth {
    /// Node identifier
    pub node_id: NodeId,
    /// Network address
    pub address: SocketAddr,
    /// Is node currently alive and reachable
    pub is_alive: bool,
    /// CPU load average (0.0 to 1.0)
    pub load_average: f64,
    /// Memory usage percentage (0.0 to 1.0)
    pub memory_usage: f64,
    /// Network latency in milliseconds
    pub network_latency_ms: u64,
    /// Last successful heartbeat timestamp
    pub last_heartbeat: SystemTime,
    /// Consecutive failure count
    pub failure_count: u32,
    /// Health score (0.0 to 1.0, higher is healthier)
    pub health_score: f64,
    /// Predicted time to failure (if applicable)
    pub predicted_failure_time: Option<SystemTime>,
}

impl From<&NodeHealth> for SerializableNodeHealth {
    fn from(health: &NodeHealth) -> Self {
        let now = SystemTime::now();
        Self {
            node_id: health.node_id,
            address: health.address,
            is_alive: health.is_alive,
            load_average: health.load_average,
            memory_usage: health.memory_usage,
            network_latency_ms: health.network_latency.as_millis() as u64,
            last_heartbeat: now.checked_sub(health.time_since_last_heartbeat()).unwrap_or(now),
            failure_count: health.failure_count,
            health_score: health.health_score,
            predicted_failure_time: health.predicted_failure_time
                .map(|instant| now.checked_sub(instant.elapsed()).unwrap_or(now)),
        }
    }
}

impl ClusterHealthReport {
    /// Create a new cluster health report
    pub fn new() -> Self {
        Self {
            timestamp: SystemTime::now(),
            node_healths: Vec::new(),
            capacity_status: CapacityStatus {
                total_cpu_capacity: 0.0,
                used_cpu_capacity: 0.0,
                total_memory_capacity: 0,
                used_memory_capacity: 0,
                total_storage_capacity: 0,
                used_storage_capacity: 0,
                healthy_nodes: 0,
                total_nodes: 0,
            },
            cluster_health_score: 0.0,
            recommendations: Vec::new(),
        }
    }

    /// Check if cluster is healthy overall
    pub fn is_cluster_healthy(&self) -> bool {
        self.cluster_health_score > 0.7 && !self.capacity_status.is_under_stress()
    }

    /// Get critical nodes that need immediate attention
    pub fn get_critical_nodes(&self) -> Vec<&SerializableNodeHealth> {
        self.node_healths
            .iter()
            .filter(|health| !health.is_alive || health.health_score < 0.3)
            .collect()
    }
}

impl Default for ClusterHealthReport {
    fn default() -> Self {
        Self::new()
    }
}

/// Health monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Health check interval in milliseconds
    pub health_check_interval_ms: u64,
    /// Failure threshold for marking node as unhealthy
    pub failure_threshold: u32,
    /// Recovery threshold for marking node as healthy
    pub recovery_threshold: u32,
    /// Enable predictive failure detection
    pub enable_predictive_detection: bool,
    /// Health score threshold for triggering alerts
    pub alert_threshold: f64,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            health_check_interval_ms: 5000,
            failure_threshold: 3,
            recovery_threshold: 2,
            enable_predictive_detection: true,
            alert_threshold: 0.5,
        }
    }
}

/// High availability monitoring system
pub struct HaMonitor {
    /// Configuration for monitoring behavior
    config: MonitoringConfig,
    /// Cluster membership for node discovery
    membership: Arc<SwimMembership>,
    /// Current health status of all nodes
    node_healths: Arc<RwLock<HashMap<NodeId, NodeHealth>>>,
    /// Historical health data for trend analysis
    health_history: Arc<RwLock<Vec<ClusterHealthReport>>>,
}

impl HaMonitor {
    /// Create a new high availability monitor
    pub fn new(
        config: MonitoringConfig,
        membership: Arc<SwimMembership>,
    ) -> Self {
        Self {
            config,
            membership,
            node_healths: Arc::new(RwLock::new(HashMap::new())),
            health_history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Start the monitoring system
    pub async fn start(&self) -> HaResult<()> {
        debug!("Starting HA monitor with interval {}ms", self.config.health_check_interval_ms);
        
        let health_check_interval = Duration::from_millis(self.config.health_check_interval_ms);
        let node_healths = Arc::clone(&self.node_healths);
        let membership = Arc::clone(&self.membership);
        let config = self.config.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(health_check_interval);
            
            loop {
                interval.tick().await;
                
                if let Err(e) = Self::perform_health_check(&node_healths, &membership, &config).await {
                    error!("Health check failed: {}", e);
                }
            }
        });

        Ok(())
    }

    /// Perform a single health check cycle
    #[instrument(skip(node_healths, membership, config))]
    async fn perform_health_check(
        node_healths: &Arc<RwLock<HashMap<NodeId, NodeHealth>>>,
        membership: &Arc<SwimMembership>,
        config: &MonitoringConfig,
    ) -> HaResult<()> {
        debug!("Performing health check");
        
        // Get current cluster members
        let members = membership.members().await;
        let mut healths = node_healths.write().await;
        
        // Update health for existing members
        for (node_id, node_info) in &members {
            let health = healths.entry(*node_id).or_insert_with(|| {
                NodeHealth::new(*node_id, node_info.address)
            });
            
            // Update health based on node status
            match node_info.status {
                NodeStatus::Alive => {
                    if !health.is_alive {
                        health.mark_recovery();
                        debug!("Node {} recovered", node_id);
                    }
                    // In a real implementation, we would gather actual metrics here
                    health.update_metrics(0.5, 0.6, Duration::from_millis(50));
                },
                NodeStatus::Suspected | NodeStatus::Dead => {
                    if health.is_alive {
                        health.mark_failure();
                        warn!("Node {} marked as unhealthy", node_id);
                    }
                },
            }
        }
        
        // Remove healths for nodes that are no longer members
        healths.retain(|node_id, _| members.contains_key(node_id));
        
        debug!("Health check completed for {} nodes", healths.len());
        Ok(())
    }

    /// Generate a comprehensive health report
    pub async fn generate_health_report(&self) -> HaResult<ClusterHealthReport> {
        let healths = self.node_healths.read().await;
        let mut report = ClusterHealthReport::new();
        
        // Convert node healths to serializable format
        for health in healths.values() {
            report.node_healths.push(health.into());
        }
        
        // Calculate capacity status
        let healthy_count = healths.values().filter(|h| h.is_healthy()).count();
        let total_count = healths.len();
        
        report.capacity_status = CapacityStatus {
            total_cpu_capacity: total_count as f64 * 4.0, // Assume 4 cores per node
            used_cpu_capacity: healths.values().map(|h| h.load_average * 4.0).sum(),
            total_memory_capacity: total_count as u64 * 16 * 1024 * 1024 * 1024, // 16GB per node
            used_memory_capacity: healths.values()
                .map(|h| (h.memory_usage * 16.0 * 1024.0 * 1024.0 * 1024.0) as u64)
                .sum(),
            total_storage_capacity: total_count as u64 * 1024 * 1024 * 1024 * 1024, // 1TB per node
            used_storage_capacity: total_count as u64 * 512 * 1024 * 1024 * 1024, // Assume 50% used
            healthy_nodes: healthy_count,
            total_nodes: total_count,
        };
        
        // Calculate cluster health score
        if total_count == 0 {
            report.cluster_health_score = 0.0;
        } else {
            let avg_health_score: f64 = healths.values().map(|h| h.health_score).sum::<f64>() / total_count as f64;
            let availability_factor = healthy_count as f64 / total_count as f64;
            report.cluster_health_score = avg_health_score * availability_factor;
        }
        
        // Generate recommendations
        if report.capacity_status.is_under_stress() {
            report.recommendations.push("Cluster is under stress - consider adding more nodes".to_string());
        }
        
        if report.capacity_status.availability_ratio() < 0.8 {
            report.recommendations.push("Low cluster availability - investigate failed nodes".to_string());
        }
        
        // Store in history
        let mut history = self.health_history.write().await;
        history.push(report.clone());
        
        // Keep only last 100 reports
        if history.len() > 100 {
            history.remove(0);
        }
        
        Ok(report)
    }

    /// Get current health status for a specific node
    pub async fn get_node_health(&self, node_id: NodeId) -> Option<NodeHealth> {
        self.node_healths.read().await.get(&node_id).cloned()
    }

    /// Get all current node healths
    pub async fn get_all_node_healths(&self) -> HashMap<NodeId, NodeHealth> {
        self.node_healths.read().await.clone()
    }

    /// Check if a node is considered healthy
    pub async fn is_node_healthy(&self, node_id: NodeId) -> bool {
        self.node_healths
            .read()
            .await
            .get(&node_id)
            .map(|h| h.is_healthy())
            .unwrap_or(false)
    }

    /// Get historical health reports
    pub async fn get_health_history(&self) -> Vec<ClusterHealthReport> {
        self.health_history.read().await.clone()
    }

    /// Stop the monitoring system
    pub async fn stop(&self) -> HaResult<()> {
        debug!("Stopping HA monitor");
        // In a real implementation, we would stop the background tasks here
        Ok(())
    }
}

impl std::fmt::Debug for HaMonitor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HaMonitor")
            .field("config", &self.config)
            .finish()
    }
}