//! # Metrics Export Infrastructure
//!
//! High-performance metrics export to external monitoring systems with
//! batching, retry logic, and multiple format support.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::{mpsc, oneshot, Notify};
use tokio::task::JoinHandle;
use tokio::time::{interval, MissedTickBehavior};
use serde::{Serialize, Deserialize};

use crate::telemetry::{
    TelemetryError, Result, ExportConfig, ExportTarget, MetricsSnapshot,
    MetricsRegistry, RetryBackoffConfig,
};

/// High-performance metrics exporter with batching and retry logic
pub struct MetricsExporter {
    /// Export targets configuration
    targets: Vec<ExportTarget>,
    
    /// Export queue for batching
    export_queue: Arc<ExportQueue>,
    
    /// Export scheduler handle
    scheduler_handle: parking_lot::Mutex<Option<JoinHandle<()>>>,
    
    /// Shutdown signal
    shutdown: Arc<AtomicBool>,
    
    /// Export statistics
    stats: ExportStats,
    
    /// Configuration
    config: ExportConfig,
    
    /// Metrics registry reference
    registry: Arc<dyn MetricsRegistry + Send + Sync>,
}

impl MetricsExporter {
    /// Creates a new metrics exporter
    pub async fn new(
        config: ExportConfig,
        registry: Arc<dyn MetricsRegistry + Send + Sync>,
    ) -> Result<Self> {
        let export_queue = Arc::new(ExportQueue::new(config.settings.batch_size * 10));
        
        Ok(Self {
            targets: config.targets.clone(),
            export_queue,
            scheduler_handle: parking_lot::Mutex::new(None),
            shutdown: Arc::new(AtomicBool::new(false)),
            stats: ExportStats::new(),
            config,
            registry,
        })
    }

    /// Exports a metrics snapshot to all configured targets
    pub async fn export_snapshot(&self, snapshot: MetricsSnapshot) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        // Queue the snapshot for batch processing
        self.export_queue.enqueue(snapshot)?;
        
        self.stats.snapshots_queued.fetch_add(1, Ordering::Relaxed);
        
        Ok(())
    }

    /// Exports to a specific target
    pub async fn export_to_target(
        &self,
        target: &ExportTarget,
        metrics: MetricsSnapshot,
    ) -> Result<()> {
        let export_start = Instant::now();
        
        match target {
            ExportTarget::Prometheus { endpoint, .. } => {
                self.export_to_prometheus(endpoint, &metrics).await?;
            }
            ExportTarget::OpenTelemetry { endpoint, headers, timeout } => {
                self.export_to_opentelemetry(endpoint, headers, *timeout, &metrics).await?;
            }
            ExportTarget::InfluxDB { endpoint, database, retention_policy, basic_auth, timeout } => {
                self.export_to_influxdb(
                    endpoint,
                    database,
                    retention_policy.as_deref(),
                    basic_auth,
                    *timeout,
                    &metrics,
                ).await?;
            }
            ExportTarget::Custom { name, config } => {
                self.export_to_custom(name, config, &metrics).await?;
            }
        }

        // Update export statistics
        let export_duration = export_start.elapsed();
        self.stats.exports_completed.fetch_add(1, Ordering::Relaxed);
        self.stats.total_export_time_ns.fetch_add(
            export_duration.as_nanos() as u64,
            Ordering::Relaxed,
        );

        Ok(())
    }

    /// Starts scheduled export processing
    pub async fn start_scheduled_export(&mut self) -> Result<()> {
        if self.targets.is_empty() {
            return Ok(()); // No targets to export to
        }

        let queue = Arc::clone(&self.export_queue);
        let targets = self.targets.clone();
        let config = self.config.clone();
        let shutdown = Arc::clone(&self.shutdown);
        let stats = self.stats.clone();

        let handle = tokio::spawn(async move {
            Self::export_scheduler_task(queue, targets, config, shutdown, stats).await;
        });

        *self.scheduler_handle.lock() = Some(handle);
        
        Ok(())
    }

    /// Stops scheduled export processing
    pub async fn stop_export(&mut self) -> Result<()> {
        self.shutdown.store(true, Ordering::Relaxed);
        
        if let Some(handle) = self.scheduler_handle.lock().take() {
            handle.abort();
            let _ = handle.await;
        }
        
        Ok(())
    }

    /// Export scheduler task
    async fn export_scheduler_task(
        queue: Arc<ExportQueue>,
        targets: Vec<ExportTarget>,
        config: ExportConfig,
        shutdown: Arc<AtomicBool>,
        stats: ExportStats,
    ) {
        let mut export_timer = interval(config.settings.default_interval);
        export_timer.set_missed_tick_behavior(MissedTickBehavior::Skip);

        while !shutdown.load(Ordering::Relaxed) {
            tokio::select! {
                _ = export_timer.tick() => {
                    if let Err(e) = Self::process_export_batch(&queue, &targets, &config, &stats).await {
                        stats.export_errors.fetch_add(1, Ordering::Relaxed);
                        tracing::warn!("Export batch processing failed: {}", e);
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    // Check for shutdown more frequently
                    continue;
                }
            }
        }

        // Final export on shutdown
        let _ = Self::process_export_batch(&queue, &targets, &config, &stats).await;
    }

    /// Processes a batch of exports
    async fn process_export_batch(
        queue: &Arc<ExportQueue>,
        targets: &[ExportTarget],
        config: &ExportConfig,
        stats: &ExportStats,
    ) -> Result<()> {
        let batch = queue.dequeue_batch(config.settings.batch_size);
        if batch.is_empty() {
            return Ok(());
        }

        // Merge snapshots into a single aggregated snapshot
        let merged_snapshot = Self::merge_snapshots(batch)?;

        // Export to all targets concurrently
        let export_futures: Vec<_> = targets
            .iter()
            .map(|target| Self::export_with_retry(target, merged_snapshot.clone(), config, stats))
            .collect();

        // Wait for all exports to complete
        let results = futures::future::join_all(export_futures).await;
        
        // Count successes and failures
        let mut successes = 0;
        let mut failures = 0;
        
        for result in results {
            match result {
                Ok(()) => successes += 1,
                Err(e) => {
                    failures += 1;
                    tracing::warn!("Export failed: {}", e);
                }
            }
        }

        stats.successful_exports.fetch_add(successes, Ordering::Relaxed);
        stats.failed_exports.fetch_add(failures, Ordering::Relaxed);

        Ok(())
    }

    /// Exports with retry logic
    async fn export_with_retry(
        target: &ExportTarget,
        snapshot: MetricsSnapshot,
        config: &ExportConfig,
        stats: &ExportStats,
    ) -> Result<()> {
        let mut attempt = 0;
        let mut delay = config.settings.retry_backoff.initial_delay;

        loop {
            match Self::export_to_single_target(target, &snapshot).await {
                Ok(()) => return Ok(()),
                Err(e) if attempt >= config.settings.max_retries => {
                    stats.retry_exhausted.fetch_add(1, Ordering::Relaxed);
                    return Err(e);
                }
                Err(e) => {
                    if !e.is_retryable() {
                        return Err(e);
                    }
                    
                    attempt += 1;
                    stats.retries_attempted.fetch_add(1, Ordering::Relaxed);
                    
                    // Apply jitter if configured
                    let actual_delay = if config.settings.retry_backoff.jitter {
                        let jitter = (rand::random::<f64>() - 0.5) * 0.1; // ±5% jitter
                        let jitter_factor = 1.0 + jitter;
                        Duration::from_nanos((delay.as_nanos() as f64 * jitter_factor) as u64)
                    } else {
                        delay
                    };
                    
                    tokio::time::sleep(actual_delay).await;
                    
                    // Calculate next delay with exponential backoff
                    delay = std::cmp::min(
                        Duration::from_nanos(
                            (delay.as_nanos() as f64 * config.settings.retry_backoff.multiplier) as u64
                        ),
                        config.settings.retry_backoff.max_delay,
                    );
                }
            }
        }
    }

    /// Exports to a single target without retry
    async fn export_to_single_target(
        target: &ExportTarget,
        snapshot: &MetricsSnapshot,
    ) -> Result<()> {
        match target {
            ExportTarget::Prometheus { endpoint, .. } => {
                Self::export_to_prometheus_sync(endpoint, snapshot).await
            }
            ExportTarget::OpenTelemetry { endpoint, headers, timeout } => {
                Self::export_to_opentelemetry_sync(endpoint, headers, *timeout, snapshot).await
            }
            ExportTarget::InfluxDB { endpoint, database, retention_policy, basic_auth, timeout } => {
                Self::export_to_influxdb_sync(
                    endpoint,
                    database,
                    retention_policy.as_deref(),
                    basic_auth,
                    *timeout,
                    snapshot,
                ).await
            }
            ExportTarget::Custom { name, config } => {
                Self::export_to_custom_sync(name, config, snapshot).await
            }
        }
    }

    /// Merges multiple snapshots into a single aggregated snapshot
    fn merge_snapshots(snapshots: Vec<MetricsSnapshot>) -> Result<MetricsSnapshot> {
        if snapshots.is_empty() {
            return Err(TelemetryError::export("No snapshots to merge"));
        }

        if snapshots.len() == 1 {
            return Ok(snapshots.into_iter().next().unwrap());
        }

        // For simplicity, return the latest snapshot
        // In production, this would perform proper aggregation
        let latest = snapshots
            .into_iter()
            .max_by_key(|s| s.timestamp)
            .unwrap();

        Ok(latest)
    }

    /// Exports to Prometheus endpoint
    async fn export_to_prometheus(&self, endpoint: &str, metrics: &MetricsSnapshot) -> Result<()> {
        Self::export_to_prometheus_sync(endpoint, metrics).await
    }

    /// Exports to Prometheus (synchronous implementation)
    async fn export_to_prometheus_sync(endpoint: &str, metrics: &MetricsSnapshot) -> Result<()> {
        let prometheus_data = Self::format_prometheus_data(metrics)?;
        
        let client = reqwest::Client::new();
        let response = client
            .post(endpoint)
            .header("Content-Type", "text/plain")
            .body(prometheus_data)
            .send()
            .await
            .map_err(|e| TelemetryError::network_with_context(
                format!("Prometheus export failed: {}", e),
                Some(endpoint),
                None,
            ))?;

        if !response.status().is_success() {
            return Err(TelemetryError::network_with_context(
                format!("Prometheus export failed with status: {}", response.status()),
                Some(endpoint),
                Some(response.status().as_u16()),
            ));
        }

        Ok(())
    }

    /// Exports to OpenTelemetry endpoint
    async fn export_to_opentelemetry(
        &self,
        endpoint: &str,
        headers: &HashMap<String, String>,
        timeout: Duration,
        metrics: &MetricsSnapshot,
    ) -> Result<()> {
        Self::export_to_opentelemetry_sync(endpoint, headers, timeout, metrics).await
    }

    /// Exports to OpenTelemetry (synchronous implementation)
    async fn export_to_opentelemetry_sync(
        endpoint: &str,
        headers: &HashMap<String, String>,
        timeout: Duration,
        metrics: &MetricsSnapshot,
    ) -> Result<()> {
        let otlp_data = Self::format_otlp_data(metrics)?;
        
        let mut request = reqwest::Client::new()
            .post(endpoint)
            .header("Content-Type", "application/x-protobuf")
            .timeout(timeout)
            .body(otlp_data);

        // Add custom headers
        for (key, value) in headers {
            request = request.header(key, value);
        }

        let response = request.send().await
            .map_err(|e| TelemetryError::network_with_context(
                format!("OpenTelemetry export failed: {}", e),
                Some(endpoint),
                None,
            ))?;

        if !response.status().is_success() {
            return Err(TelemetryError::network_with_context(
                format!("OpenTelemetry export failed with status: {}", response.status()),
                Some(endpoint),
                Some(response.status().as_u16()),
            ));
        }

        Ok(())
    }

    /// Exports to InfluxDB
    async fn export_to_influxdb(
        &self,
        endpoint: &str,
        database: &str,
        retention_policy: Option<&str>,
        basic_auth: &Option<crate::telemetry::BasicAuth>,
        timeout: Duration,
        metrics: &MetricsSnapshot,
    ) -> Result<()> {
        Self::export_to_influxdb_sync(endpoint, database, retention_policy, basic_auth, timeout, metrics).await
    }

    /// Exports to InfluxDB (synchronous implementation)
    async fn export_to_influxdb_sync(
        endpoint: &str,
        database: &str,
        retention_policy: Option<&str>,
        basic_auth: &Option<crate::telemetry::BasicAuth>,
        timeout: Duration,
        metrics: &MetricsSnapshot,
    ) -> Result<()> {
        let line_protocol_data = Self::format_influxdb_data(metrics)?;
        
        let mut url = format!("{}/write?db={}", endpoint, database);
        if let Some(rp) = retention_policy {
            url.push_str(&format!("&rp={}", rp));
        }

        let mut request = reqwest::Client::new()
            .post(&url)
            .header("Content-Type", "text/plain")
            .timeout(timeout)
            .body(line_protocol_data);

        // Add basic authentication if configured
        if let Some(auth) = basic_auth {
            request = request.basic_auth(&auth.username, Some(&auth.password));
        }

        let response = request.send().await
            .map_err(|e| TelemetryError::network_with_context(
                format!("InfluxDB export failed: {}", e),
                Some(endpoint),
                None,
            ))?;

        if !response.status().is_success() {
            return Err(TelemetryError::network_with_context(
                format!("InfluxDB export failed with status: {}", response.status()),
                Some(endpoint),
                Some(response.status().as_u16()),
            ));
        }

        Ok(())
    }

    /// Exports to custom endpoint
    async fn export_to_custom(
        &self,
        name: &str,
        config: &HashMap<String, serde_json::Value>,
        metrics: &MetricsSnapshot,
    ) -> Result<()> {
        Self::export_to_custom_sync(name, config, metrics).await
    }

    /// Exports to custom endpoint (synchronous implementation)
    async fn export_to_custom_sync(
        _name: &str,
        _config: &HashMap<String, serde_json::Value>,
        _metrics: &MetricsSnapshot,
    ) -> Result<()> {
        // Custom export implementation would go here
        // For now, just return success
        Ok(())
    }

    /// Formats metrics in Prometheus exposition format
    fn format_prometheus_data(metrics: &MetricsSnapshot) -> Result<String> {
        let mut output = String::new();

        // Format counters
        for counter in &metrics.counters {
            output.push_str(&format!(
                "{} {}\n",
                counter.key.as_str().replace('.', "_"),
                counter.value
            ));
        }

        // Format gauges
        for gauge in &metrics.gauges {
            output.push_str(&format!(
                "{} {}\n",
                gauge.key.as_str().replace('.', "_"),
                gauge.value
            ));
        }

        // Format histograms
        for histogram in &metrics.histograms {
            let base_name = histogram.key.as_str().replace('.', "_");
            
            // Histogram buckets
            for bucket in &histogram.buckets {
                output.push_str(&format!(
                    "{}_bucket{{le=\"{}\"}} {}\n",
                    base_name, bucket.le, bucket.cumulative_count
                ));
            }
            
            // Histogram count and sum
            output.push_str(&format!("{}_count {}\n", base_name, histogram.count));
            output.push_str(&format!("{}_sum {}\n", base_name, histogram.sum));
        }

        Ok(output)
    }

    /// Formats metrics in OTLP format (simplified)
    fn format_otlp_data(metrics: &MetricsSnapshot) -> Result<Vec<u8>> {
        // This would be a proper OTLP protobuf encoding in production
        // For simplicity, using JSON representation
        let json_data = serde_json::to_vec(metrics)
            .map_err(|e| TelemetryError::serialization_with_format(
                format!("OTLP formatting error: {}", e),
                "otlp"
            ))?;
        
        Ok(json_data)
    }

    /// Formats metrics in InfluxDB Line Protocol format
    fn format_influxdb_data(metrics: &MetricsSnapshot) -> Result<String> {
        let mut output = String::new();
        let timestamp = metrics.timestamp
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();

        // Format counters
        for counter in &metrics.counters {
            output.push_str(&format!(
                "{} value={} {}\n",
                counter.key.as_str(),
                counter.value,
                timestamp
            ));
        }

        // Format gauges
        for gauge in &metrics.gauges {
            output.push_str(&format!(
                "{} value={} {}\n",
                gauge.key.as_str(),
                gauge.value,
                timestamp
            ));
        }

        // Format histograms (simplified - would include percentiles in production)
        for histogram in &metrics.histograms {
            output.push_str(&format!(
                "{} count={},sum={} {}\n",
                histogram.key.as_str(),
                histogram.count,
                histogram.sum,
                timestamp
            ));
        }

        Ok(output)
    }

    /// Returns export success rate
    pub fn success_rate(&self) -> f64 {
        let successful = self.stats.successful_exports.load(Ordering::Relaxed) as f64;
        let total = successful + self.stats.failed_exports.load(Ordering::Relaxed) as f64;
        
        if total > 0.0 {
            successful / total
        } else {
            1.0 // No exports yet, assume 100% success rate
        }
    }

    /// Returns export statistics
    pub fn export_stats(&self) -> ExportStatsSnapshot {
        let successful = self.stats.successful_exports.load(Ordering::Relaxed);
        let failed = self.stats.failed_exports.load(Ordering::Relaxed);
        let total_exports = successful + failed;
        
        ExportStatsSnapshot {
            snapshots_queued: self.stats.snapshots_queued.load(Ordering::Relaxed),
            exports_completed: self.stats.exports_completed.load(Ordering::Relaxed),
            successful_exports: successful,
            failed_exports: failed,
            success_rate: self.success_rate(),
            export_errors: self.stats.export_errors.load(Ordering::Relaxed),
            retries_attempted: self.stats.retries_attempted.load(Ordering::Relaxed),
            retry_exhausted: self.stats.retry_exhausted.load(Ordering::Relaxed),
            average_export_time_ns: if total_exports > 0 {
                self.stats.total_export_time_ns.load(Ordering::Relaxed) / total_exports
            } else {
                0
            },
            queue_size: self.export_queue.size(),
        }
    }

    /// Returns estimated memory usage
    pub fn memory_usage(&self) -> usize {
        let base_size = std::mem::size_of::<Self>();
        let queue_size = self.export_queue.memory_usage();
        let targets_size = self.targets.len() * std::mem::size_of::<ExportTarget>();
        
        base_size + queue_size + targets_size
    }

    /// Returns estimated CPU usage
    pub fn cpu_usage_estimate(&self) -> f64 {
        let total_exports = self.stats.exports_completed.load(Ordering::Relaxed);
        if total_exports == 0 {
            return 0.0;
        }

        let avg_export_time_ns = self.stats.total_export_time_ns.load(Ordering::Relaxed) / total_exports;
        let exports_per_second = total_exports as f64 / 60.0; // Rough estimate

        // CPU usage estimation based on export rate
        exports_per_second * (avg_export_time_ns as f64) / 1_000_000_000.0
    }
}

/// Export queue for batching snapshots
pub struct ExportQueue {
    /// Queue of snapshots waiting for export
    queue: crossbeam_queue::ArrayQueue<MetricsSnapshot>,
    
    /// Queue size tracking
    size: AtomicU64,
}

impl ExportQueue {
    /// Creates a new export queue
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: crossbeam_queue::ArrayQueue::new(capacity),
            size: AtomicU64::new(0),
        }
    }

    /// Enqueues a snapshot for export
    pub fn enqueue(&self, snapshot: MetricsSnapshot) -> Result<()> {
        self.queue.push(snapshot)
            .map_err(|_| TelemetryError::export("Export queue full"))?;
        
        self.size.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Dequeues a batch of snapshots
    pub fn dequeue_batch(&self, max_size: usize) -> Vec<MetricsSnapshot> {
        let mut batch = Vec::with_capacity(max_size);
        
        for _ in 0..max_size {
            if let Ok(snapshot) = self.queue.pop() {
                batch.push(snapshot);
                self.size.fetch_sub(1, Ordering::Relaxed);
            } else {
                break;
            }
        }
        
        batch
    }

    /// Returns current queue size
    pub fn size(&self) -> usize {
        self.size.load(Ordering::Relaxed) as usize
    }

    /// Returns queue capacity
    pub fn capacity(&self) -> usize {
        self.queue.capacity()
    }

    /// Returns estimated memory usage
    pub fn memory_usage(&self) -> usize {
        let base_size = std::mem::size_of::<Self>();
        let queue_size = self.size() * std::mem::size_of::<MetricsSnapshot>();
        
        base_size + queue_size
    }
}

/// Export statistics
#[derive(Debug, Clone)]
pub struct ExportStats {
    pub snapshots_queued: AtomicU64,
    pub exports_completed: AtomicU64,
    pub successful_exports: AtomicU64,
    pub failed_exports: AtomicU64,
    pub export_errors: AtomicU64,
    pub retries_attempted: AtomicU64,
    pub retry_exhausted: AtomicU64,
    pub total_export_time_ns: AtomicU64,
}

impl ExportStats {
    pub fn new() -> Self {
        Self {
            snapshots_queued: AtomicU64::new(0),
            exports_completed: AtomicU64::new(0),
            successful_exports: AtomicU64::new(0),
            failed_exports: AtomicU64::new(0),
            export_errors: AtomicU64::new(0),
            retries_attempted: AtomicU64::new(0),
            retry_exhausted: AtomicU64::new(0),
            total_export_time_ns: AtomicU64::new(0),
        }
    }
}

/// Export statistics snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportStatsSnapshot {
    pub snapshots_queued: u64,
    pub exports_completed: u64,
    pub successful_exports: u64,
    pub failed_exports: u64,
    pub success_rate: f64,
    pub export_errors: u64,
    pub retries_attempted: u64,
    pub retry_exhausted: u64,
    pub average_export_time_ns: u64,
    pub queue_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct MockRegistry;
    
    impl MetricsRegistry for MockRegistry {
        // Mock implementation for testing
    }

    #[tokio::test]
    async fn test_export_queue() {
        let queue = ExportQueue::new(10);
        assert_eq!(queue.size(), 0);
        assert_eq!(queue.capacity(), 10);

        let snapshot = MetricsSnapshot {
            counters: vec![],
            histograms: vec![],
            gauges: vec![],
            summaries: vec![],
            collection_time: Duration::from_millis(1),
            timestamp: SystemTime::now(),
            metadata: crate::telemetry::MetricsMetadata {
                total_operations: 100,
                average_operation_time_ns: 50,
                memory_usage_bytes: 1024,
                uptime: Duration::from_secs(60),
            },
        };

        queue.enqueue(snapshot).unwrap();
        assert_eq!(queue.size(), 1);

        let batch = queue.dequeue_batch(5);
        assert_eq!(batch.len(), 1);
        assert_eq!(queue.size(), 0);
    }

    #[tokio::test]
    async fn test_metrics_exporter_creation() {
        let config = ExportConfig::default();
        let registry = Arc::new(MockRegistry);
        
        let exporter = MetricsExporter::new(config, registry).await.unwrap();
        assert_eq!(exporter.success_rate(), 1.0); // No exports yet
    }

    #[test]
    fn test_prometheus_formatting() {
        let snapshot = MetricsSnapshot {
            counters: vec![
                crate::telemetry::CounterMetric {
                    key: crate::telemetry::MetricKey::MessagesProcessed,
                    value: 42,
                    timestamp: SystemTime::now(),
                }
            ],
            histograms: vec![],
            gauges: vec![
                crate::telemetry::GaugeMetric {
                    key: crate::telemetry::MetricKey::ActiveConnections,
                    value: 10.5,
                    timestamp: SystemTime::now(),
                }
            ],
            summaries: vec![],
            collection_time: Duration::from_millis(1),
            timestamp: SystemTime::now(),
            metadata: crate::telemetry::MetricsMetadata {
                total_operations: 100,
                average_operation_time_ns: 50,
                memory_usage_bytes: 1024,
                uptime: Duration::from_secs(60),
            },
        };

        let formatted = MetricsExporter::format_prometheus_data(&snapshot).unwrap();
        
        assert!(formatted.contains("messages_processed 42"));
        assert!(formatted.contains("network_active_connections 10.5"));
    }

    #[test]
    fn test_influxdb_formatting() {
        let snapshot = MetricsSnapshot {
            counters: vec![
                crate::telemetry::CounterMetric {
                    key: crate::telemetry::MetricKey::MessagesProcessed,
                    value: 42,
                    timestamp: SystemTime::now(),
                }
            ],
            histograms: vec![],
            gauges: vec![],
            summaries: vec![],
            collection_time: Duration::from_millis(1),
            timestamp: SystemTime::now(),
            metadata: crate::telemetry::MetricsMetadata {
                total_operations: 100,
                average_operation_time_ns: 50,
                memory_usage_bytes: 1024,
                uptime: Duration::from_secs(60),
            },
        };

        let formatted = MetricsExporter::format_influxdb_data(&snapshot).unwrap();
        
        assert!(formatted.contains("messages.processed value=42"));
    }

    #[test]
    fn test_export_stats() {
        let stats = ExportStats::new();
        
        stats.snapshots_queued.store(10, Ordering::Relaxed);
        stats.successful_exports.store(8, Ordering::Relaxed);
        stats.failed_exports.store(2, Ordering::Relaxed);

        assert_eq!(stats.snapshots_queued.load(Ordering::Relaxed), 10);
        assert_eq!(stats.successful_exports.load(Ordering::Relaxed), 8);
        assert_eq!(stats.failed_exports.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn test_success_rate_calculation() {
        let config = ExportConfig::default();
        let registry = Arc::new(MockRegistry);
        
        let rt = tokio::runtime::Runtime::new().unwrap();
        let exporter = rt.block_on(async {
            MetricsExporter::new(config, registry).await.unwrap()
        });

        // Initial success rate should be 100%
        assert_eq!(exporter.success_rate(), 1.0);

        // Simulate some exports
        exporter.stats.successful_exports.store(8, Ordering::Relaxed);
        exporter.stats.failed_exports.store(2, Ordering::Relaxed);

        // Success rate should be 80%
        assert_eq!(exporter.success_rate(), 0.8);
    }

    #[test]
    fn test_memory_usage_estimation() {
        let config = ExportConfig::default();
        let registry = Arc::new(MockRegistry);
        
        let rt = tokio::runtime::Runtime::new().unwrap();
        let exporter = rt.block_on(async {
            MetricsExporter::new(config, registry).await.unwrap()
        });

        let memory_usage = exporter.memory_usage();
        assert!(memory_usage > 0);
        assert!(memory_usage < 1_000_000); // Should be reasonable
    }
}