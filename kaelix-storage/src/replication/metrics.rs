//! # Replication Metrics and Monitoring
//!
//! Comprehensive metrics collection and monitoring for the replication system,
//! providing insights into performance, health, and operational characteristics.

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use kaelix_cluster::types::NodeId;

/// Comprehensive replication metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationMetrics {
    /// Throughput metrics
    pub throughput: ThroughputMetrics,
    
    /// Latency metrics
    pub latency: LatencyMetrics,
    
    /// Reliability metrics
    pub reliability: ReliabilityMetrics,
    
    /// Resource utilization metrics
    pub resources: ResourceMetrics,
    
    /// Per-replica metrics
    pub per_replica: HashMap<NodeId, ReplicaMetrics>,
    
    /// Timestamp when metrics were collected
    pub collected_at: SystemTime,
}

/// Throughput-related metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThroughputMetrics {
    /// Total operations per second
    pub total_ops_per_sec: f64,
    
    /// Write operations per second
    pub write_ops_per_sec: f64,
    
    /// Read operations per second
    pub read_ops_per_sec: f64,
    
    /// Bytes replicated per second
    pub bytes_per_sec: f64,
    
    /// Peak throughput achieved
    pub peak_ops_per_sec: f64,
    
    /// Average batch size
    pub avg_batch_size: f64,
}

/// Latency-related metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyMetrics {
    /// Average replication latency
    pub avg_replication_latency: Duration,
    
    /// P50 replication latency
    pub p50_replication_latency: Duration,
    
    /// P95 replication latency
    pub p95_replication_latency: Duration,
    
    /// P99 replication latency
    pub p99_replication_latency: Duration,
    
    /// Maximum replication latency observed
    pub max_replication_latency: Duration,
    
    /// Average quorum write latency
    pub avg_quorum_write_latency: Duration,
    
    /// Average quorum read latency
    pub avg_quorum_read_latency: Duration,
}

/// Reliability and consistency metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReliabilityMetrics {
    /// Total operations attempted
    pub total_operations: u64,
    
    /// Successful operations
    pub successful_operations: u64,
    
    /// Failed operations
    pub failed_operations: u64,
    
    /// Success rate (0.0 to 1.0)
    pub success_rate: f64,
    
    /// Number of consistency violations
    pub consistency_violations: u64,
    
    /// Number of replica failures
    pub replica_failures: u64,
    
    /// Number of recovery operations
    pub recovery_operations: u64,
    
    /// Current availability percentage
    pub availability_percentage: f64,
}

/// Resource utilization metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceMetrics {
    /// Memory usage in bytes
    pub memory_usage_bytes: u64,
    
    /// Network bandwidth utilization (bytes/sec)
    pub network_bandwidth_usage: f64,
    
    /// CPU usage percentage for replication
    pub cpu_usage_percentage: f64,
    
    /// Active connections count
    pub active_connections: usize,
    
    /// Buffer utilization percentage
    pub buffer_utilization: f64,
}

/// Per-replica specific metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicaMetrics {
    /// Replica node identifier
    pub replica_id: NodeId,
    
    /// Current lag in sequence numbers
    pub lag_offset: u64,
    
    /// Health score (0.0 to 1.0)
    pub health_score: f64,
    
    /// Last successful heartbeat
    pub last_heartbeat: SystemTime,
    
    /// Average response time
    pub avg_response_time: Duration,
    
    /// Number of failed operations
    pub failed_operations: u64,
    
    /// Bytes replicated to this replica
    pub bytes_replicated: u64,
    
    /// Connection status
    pub connection_status: ConnectionStatus,
}

/// Connection status for replicas
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    /// Connected and healthy
    Connected,
    /// Connected but degraded performance
    Degraded,
    /// Connection issues
    Unstable,
    /// Disconnected
    Disconnected,
}

/// Historical data point for trend analysis
#[derive(Debug, Clone)]
struct DataPoint {
    timestamp: Instant,
    value: f64,
}

/// Metrics collector and aggregator
pub struct ReplicationMetricsCollector {
    /// Current metrics snapshot
    current_metrics: Arc<RwLock<ReplicationMetrics>>,
    
    /// Historical throughput data
    throughput_history: Arc<RwLock<VecDeque<DataPoint>>>,
    
    /// Historical latency data
    latency_history: Arc<RwLock<VecDeque<Duration>>>,
    
    /// Start time for metrics collection
    start_time: Instant,
    
    /// Collection interval
    collection_interval: Duration,
    
    /// Maximum history size
    max_history_size: usize,
}

impl Default for ThroughputMetrics {
    fn default() -> Self {
        Self {
            total_ops_per_sec: 0.0,
            write_ops_per_sec: 0.0,
            read_ops_per_sec: 0.0,
            bytes_per_sec: 0.0,
            peak_ops_per_sec: 0.0,
            avg_batch_size: 0.0,
        }
    }
}

impl Default for LatencyMetrics {
    fn default() -> Self {
        Self {
            avg_replication_latency: Duration::from_millis(0),
            p50_replication_latency: Duration::from_millis(0),
            p95_replication_latency: Duration::from_millis(0),
            p99_replication_latency: Duration::from_millis(0),
            max_replication_latency: Duration::from_millis(0),
            avg_quorum_write_latency: Duration::from_millis(0),
            avg_quorum_read_latency: Duration::from_millis(0),
        }
    }
}

impl Default for ReliabilityMetrics {
    fn default() -> Self {
        Self {
            total_operations: 0,
            successful_operations: 0,
            failed_operations: 0,
            success_rate: 0.0,
            consistency_violations: 0,
            replica_failures: 0,
            recovery_operations: 0,
            availability_percentage: 100.0,
        }
    }
}

impl Default for ResourceMetrics {
    fn default() -> Self {
        Self {
            memory_usage_bytes: 0,
            network_bandwidth_usage: 0.0,
            cpu_usage_percentage: 0.0,
            active_connections: 0,
            buffer_utilization: 0.0,
        }
    }
}

impl ReplicationMetricsCollector {
    /// Create a new metrics collector
    pub fn new() -> Self {
        let initial_metrics = ReplicationMetrics {
            throughput: ThroughputMetrics::default(),
            latency: LatencyMetrics::default(),
            reliability: ReliabilityMetrics::default(),
            resources: ResourceMetrics::default(),
            per_replica: HashMap::new(),
            collected_at: SystemTime::now(),
        };

        Self {
            current_metrics: Arc::new(RwLock::new(initial_metrics)),
            throughput_history: Arc::new(RwLock::new(VecDeque::new())),
            latency_history: Arc::new(RwLock::new(VecDeque::new())),
            start_time: Instant::now(),
            collection_interval: Duration::from_secs(1),
            max_history_size: 3600, // 1 hour at 1-second intervals
        }
    }

    /// Record a write operation with its latency and bytes
    pub async fn record_write_operation(&self, latency: Duration, bytes: usize, success: bool) {
        let mut metrics = self.current_metrics.write().await;
        
        // Update reliability metrics
        metrics.reliability.total_operations += 1;
        if success {
            metrics.reliability.successful_operations += 1;
        } else {
            metrics.reliability.failed_operations += 1;
        }
        
        // Recalculate success rate
        metrics.reliability.success_rate = metrics.reliability.successful_operations as f64
            / metrics.reliability.total_operations as f64;

        // Record latency
        self.record_latency(latency).await;
        
        // Update throughput (will be calculated in periodic update)
        drop(metrics);
    }

    /// Record a read operation with its latency
    pub async fn record_read_operation(&self, latency: Duration, success: bool) {
        let mut metrics = self.current_metrics.write().await;
        
        metrics.reliability.total_operations += 1;
        if success {
            metrics.reliability.successful_operations += 1;
        } else {
            metrics.reliability.failed_operations += 1;
        }
        
        metrics.reliability.success_rate = metrics.reliability.successful_operations as f64
            / metrics.reliability.total_operations as f64;

        drop(metrics);
        self.record_latency(latency).await;
    }

    /// Record replica-specific metrics
    pub async fn record_replica_metrics(&self, replica_id: NodeId, replica_metrics: ReplicaMetrics) {
        let mut metrics = self.current_metrics.write().await;
        metrics.per_replica.insert(replica_id, replica_metrics);
    }

    /// Record a consistency violation
    pub async fn record_consistency_violation(&self) {
        let mut metrics = self.current_metrics.write().await;
        metrics.reliability.consistency_violations += 1;
    }

    /// Record a replica failure
    pub async fn record_replica_failure(&self, replica_id: NodeId) {
        let mut metrics = self.current_metrics.write().await;
        metrics.reliability.replica_failures += 1;
        
        // Update replica status if it exists
        if let Some(replica_metrics) = metrics.per_replica.get_mut(&replica_id) {
            replica_metrics.connection_status = ConnectionStatus::Disconnected;
            replica_metrics.health_score = 0.0;
        }
    }

    /// Record a recovery operation
    pub async fn record_recovery_operation(&self) {
        let mut metrics = self.current_metrics.write().await;
        metrics.reliability.recovery_operations += 1;
    }

    /// Update resource utilization metrics
    pub async fn update_resource_metrics(&self, resources: ResourceMetrics) {
        let mut metrics = self.current_metrics.write().await;
        metrics.resources = resources;
    }

    /// Get current metrics snapshot
    pub async fn get_current_metrics(&self) -> ReplicationMetrics {
        let mut metrics = self.current_metrics.write().await;
        metrics.collected_at = SystemTime::now();
        metrics.clone()
    }

    /// Start periodic metrics collection
    pub async fn start_periodic_collection(&self) {
        let current_metrics = Arc::clone(&self.current_metrics);
        let throughput_history = Arc::clone(&self.throughput_history);
        let collection_interval = self.collection_interval;
        let max_history_size = self.max_history_size;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(collection_interval);
            let mut last_operations = 0u64;
            let mut last_collection = Instant::now();

            loop {
                interval.tick().await;
                
                let now = Instant::now();
                let elapsed = now.duration_since(last_collection);
                
                // Calculate throughput
                let mut metrics = current_metrics.write().await;
                let current_operations = metrics.reliability.total_operations;
                let operations_delta = current_operations - last_operations;
                
                let ops_per_sec = operations_delta as f64 / elapsed.as_secs_f64();
                metrics.throughput.total_ops_per_sec = ops_per_sec;
                
                // Update peak if necessary
                if ops_per_sec > metrics.throughput.peak_ops_per_sec {
                    metrics.throughput.peak_ops_per_sec = ops_per_sec;
                }
                
                drop(metrics);
                
                // Record throughput history
                let mut history = throughput_history.write().await;
                history.push_back(DataPoint {
                    timestamp: now,
                    value: ops_per_sec,
                });
                
                // Limit history size
                if history.len() > max_history_size {
                    history.pop_front();
                }
                drop(history);
                
                // Update for next iteration
                last_operations = current_operations;
                last_collection = now;
            }
        });
    }

    /// Calculate throughput statistics over a time window
    pub async fn get_throughput_statistics(&self, window: Duration) -> ThroughputStatistics {
        let history = self.throughput_history.read().await;
        let cutoff_time = Instant::now() - window;
        
        let recent_data: Vec<f64> = history
            .iter()
            .filter(|dp| dp.timestamp >= cutoff_time)
            .map(|dp| dp.value)
            .collect();
        
        if recent_data.is_empty() {
            return ThroughputStatistics::default();
        }
        
        let sum: f64 = recent_data.iter().sum();
        let average = sum / recent_data.len() as f64;
        let max = recent_data.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let min = recent_data.iter().copied().fold(f64::INFINITY, f64::min);
        
        ThroughputStatistics {
            average,
            maximum: max,
            minimum: min,
            data_points: recent_data.len(),
        }
    }

    /// Calculate latency percentiles
    pub async fn get_latency_percentiles(&self) -> LatencyPercentiles {
        let history = self.latency_history.read().await;
        
        if history.is_empty() {
            return LatencyPercentiles::default();
        }
        
        let mut sorted_latencies: Vec<Duration> = history.iter().copied().collect();
        sorted_latencies.sort();
        
        let len = sorted_latencies.len();
        let p50_idx = (len as f64 * 0.50) as usize;
        let p95_idx = (len as f64 * 0.95) as usize;
        let p99_idx = (len as f64 * 0.99) as usize;
        
        LatencyPercentiles {
            p50: sorted_latencies[p50_idx.min(len - 1)],
            p95: sorted_latencies[p95_idx.min(len - 1)],
            p99: sorted_latencies[p99_idx.min(len - 1)],
            max: sorted_latencies[len - 1],
            sample_size: len,
        }
    }

    /// Generate a metrics report
    pub async fn generate_report(&self) -> MetricsReport {
        let current = self.get_current_metrics().await;
        let throughput_stats = self.get_throughput_statistics(Duration::from_minutes(5)).await;
        let latency_percentiles = self.get_latency_percentiles().await;
        
        MetricsReport {
            timestamp: SystemTime::now(),
            uptime: self.start_time.elapsed(),
            current_metrics: current,
            throughput_statistics: throughput_stats,
            latency_percentiles,
        }
    }

    // ========================================
    // Private Implementation Methods
    // ========================================

    /// Record latency data point
    async fn record_latency(&self, latency: Duration) {
        let mut history = self.latency_history.write().await;
        history.push_back(latency);
        
        // Limit history size
        if history.len() > self.max_history_size {
            history.pop_front();
        }
    }
}

/// Throughput statistics over a time window
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThroughputStatistics {
    /// Average throughput
    pub average: f64,
    /// Maximum throughput
    pub maximum: f64,
    /// Minimum throughput
    pub minimum: f64,
    /// Number of data points
    pub data_points: usize,
}

impl Default for ThroughputStatistics {
    fn default() -> Self {
        Self {
            average: 0.0,
            maximum: 0.0,
            minimum: 0.0,
            data_points: 0,
        }
    }
}

/// Latency percentiles
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyPercentiles {
    /// 50th percentile latency
    pub p50: Duration,
    /// 95th percentile latency
    pub p95: Duration,
    /// 99th percentile latency
    pub p99: Duration,
    /// Maximum latency
    pub max: Duration,
    /// Sample size
    pub sample_size: usize,
}

impl Default for LatencyPercentiles {
    fn default() -> Self {
        Self {
            p50: Duration::from_millis(0),
            p95: Duration::from_millis(0),
            p99: Duration::from_millis(0),
            max: Duration::from_millis(0),
            sample_size: 0,
        }
    }
}

/// Comprehensive metrics report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsReport {
    /// Report generation timestamp
    pub timestamp: SystemTime,
    /// System uptime
    pub uptime: Duration,
    /// Current metrics snapshot
    pub current_metrics: ReplicationMetrics,
    /// Throughput statistics
    pub throughput_statistics: ThroughputStatistics,
    /// Latency percentiles
    pub latency_percentiles: LatencyPercentiles,
}

/// Replication statistics summary
pub type ReplicationStats = ReplicationMetrics;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_collector_creation() {
        let collector = ReplicationMetricsCollector::new();
        let metrics = collector.get_current_metrics().await;
        
        assert_eq!(metrics.reliability.total_operations, 0);
        assert_eq!(metrics.throughput.total_ops_per_sec, 0.0);
    }

    #[tokio::test]
    async fn test_write_operation_recording() {
        let collector = ReplicationMetricsCollector::new();
        
        collector.record_write_operation(
            Duration::from_millis(10),
            1024,
            true,
        ).await;
        
        let metrics = collector.get_current_metrics().await;
        assert_eq!(metrics.reliability.total_operations, 1);
        assert_eq!(metrics.reliability.successful_operations, 1);
        assert_eq!(metrics.reliability.success_rate, 1.0);
    }

    #[tokio::test]
    async fn test_read_operation_recording() {
        let collector = ReplicationMetricsCollector::new();
        
        collector.record_read_operation(Duration::from_millis(5), true).await;
        collector.record_read_operation(Duration::from_millis(8), false).await;
        
        let metrics = collector.get_current_metrics().await;
        assert_eq!(metrics.reliability.total_operations, 2);
        assert_eq!(metrics.reliability.successful_operations, 1);
        assert_eq!(metrics.reliability.failed_operations, 1);
        assert_eq!(metrics.reliability.success_rate, 0.5);
    }

    #[tokio::test]
    async fn test_replica_metrics_recording() {
        let collector = ReplicationMetricsCollector::new();
        let replica_id = NodeId::generate();
        
        let replica_metrics = ReplicaMetrics {
            replica_id,
            lag_offset: 100,
            health_score: 0.9,
            last_heartbeat: SystemTime::now(),
            avg_response_time: Duration::from_millis(15),
            failed_operations: 2,
            bytes_replicated: 10240,
            connection_status: ConnectionStatus::Connected,
        };
        
        collector.record_replica_metrics(replica_id, replica_metrics).await;
        
        let metrics = collector.get_current_metrics().await;
        assert_eq!(metrics.per_replica.len(), 1);
        assert!(metrics.per_replica.contains_key(&replica_id));
        
        let recorded = &metrics.per_replica[&replica_id];
        assert_eq!(recorded.lag_offset, 100);
        assert_eq!(recorded.health_score, 0.9);
    }

    #[tokio::test]
    async fn test_consistency_violation_recording() {
        let collector = ReplicationMetricsCollector::new();
        
        collector.record_consistency_violation().await;
        collector.record_consistency_violation().await;
        
        let metrics = collector.get_current_metrics().await;
        assert_eq!(metrics.reliability.consistency_violations, 2);
    }

    #[tokio::test]
    async fn test_replica_failure_recording() {
        let collector = ReplicationMetricsCollector::new();
        let replica_id = NodeId::generate();
        
        // First add a healthy replica
        let replica_metrics = ReplicaMetrics {
            replica_id,
            lag_offset: 0,
            health_score: 1.0,
            last_heartbeat: SystemTime::now(),
            avg_response_time: Duration::from_millis(10),
            failed_operations: 0,
            bytes_replicated: 0,
            connection_status: ConnectionStatus::Connected,
        };
        
        collector.record_replica_metrics(replica_id, replica_metrics).await;
        
        // Record failure
        collector.record_replica_failure(replica_id).await;
        
        let metrics = collector.get_current_metrics().await;
        assert_eq!(metrics.reliability.replica_failures, 1);
        
        let failed_replica = &metrics.per_replica[&replica_id];
        assert_eq!(failed_replica.connection_status, ConnectionStatus::Disconnected);
        assert_eq!(failed_replica.health_score, 0.0);
    }

    #[tokio::test]
    async fn test_latency_percentiles_calculation() {
        let collector = ReplicationMetricsCollector::new();
        
        // Record some latencies
        for i in 1..=100 {
            collector.record_latency(Duration::from_millis(i)).await;
        }
        
        let percentiles = collector.get_latency_percentiles().await;
        assert_eq!(percentiles.sample_size, 100);
        assert!(percentiles.p50 < percentiles.p95);
        assert!(percentiles.p95 < percentiles.p99);
        assert!(percentiles.p99 <= percentiles.max);
    }

    #[tokio::test]
    async fn test_metrics_report_generation() {
        let collector = ReplicationMetricsCollector::new();
        
        // Record some operations
        collector.record_write_operation(Duration::from_millis(10), 1024, true).await;
        collector.record_read_operation(Duration::from_millis(5), true).await;
        
        let report = collector.generate_report().await;
        
        assert_eq!(report.current_metrics.reliability.total_operations, 2);
        assert_eq!(report.current_metrics.reliability.successful_operations, 2);
        assert!(report.uptime > Duration::from_millis(0));
    }

    #[test]
    fn test_connection_status_serialization() {
        let status = ConnectionStatus::Connected;
        let serialized = serde_json::to_string(&status).unwrap();
        let deserialized: ConnectionStatus = serde_json::from_str(&serialized).unwrap();
        
        assert_eq!(status, deserialized);
    }
}