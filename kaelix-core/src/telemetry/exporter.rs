//! Metrics export system for external telemetry targets
//!
//! Provides high-performance, reliable export of metrics to various backends
//! with batching, retry logic, and configurable targets.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{interval, MissedTickBehavior};

use crate::telemetry::config::ExportTargetType;
use crate::telemetry::{
    registry::MetricsRegistry as ConcreteMetricsRegistry, BasicAuth, ExportConfig, ExportTarget,
    MetricsSnapshot, Result, TelemetryError,
};

/// High-performance metrics exporter with batching and retry logic
#[derive(Debug)]
pub struct MetricsExporter {
    /// Export targets configuration
    targets: Vec<ExportTarget>,

    /// Export statistics
    stats: Arc<ExportStats>,

    /// Control channel for shutdown
    shutdown_tx: Option<mpsc::UnboundedSender<()>>,

    /// Background export task handles
    export_handles: Vec<JoinHandle<()>>,

    /// Configuration
    config: ExportConfig,
}

/// Export performance statistics
#[derive(Debug)]
pub struct ExportStats {
    /// Total exports attempted
    pub total_exports: AtomicU64,

    /// Successful exports
    pub successful_exports: AtomicU64,

    /// Failed exports
    pub failed_exports: AtomicU64,

    /// Total metrics exported
    pub total_metrics: AtomicU64,

    /// Average export latency (microseconds)
    pub avg_export_latency: AtomicU64,

    /// Last export timestamp
    pub last_export: AtomicU64,

    /// Export backlog size
    pub backlog_size: AtomicU64,
}

impl ExportStats {
    pub fn new() -> Self {
        Self {
            total_exports: AtomicU64::new(0),
            successful_exports: AtomicU64::new(0),
            failed_exports: AtomicU64::new(0),
            total_metrics: AtomicU64::new(0),
            avg_export_latency: AtomicU64::new(0),
            last_export: AtomicU64::new(0),
            backlog_size: AtomicU64::new(0),
        }
    }

    pub fn record_export(&self, success: bool, latency_us: u64, metrics_count: u64) {
        self.total_exports.fetch_add(1, Ordering::Relaxed);
        if success {
            self.successful_exports.fetch_add(1, Ordering::Relaxed);
        } else {
            self.failed_exports.fetch_add(1, Ordering::Relaxed);
        }

        self.total_metrics.fetch_add(metrics_count, Ordering::Relaxed);

        // Update rolling average latency
        let total_exports = self.total_exports.load(Ordering::Relaxed);
        let current_avg = self.avg_export_latency.load(Ordering::Relaxed);
        let new_avg = if total_exports == 1 {
            latency_us
        } else {
            ((current_avg * (total_exports - 1)) + latency_us) / total_exports
        };
        self.avg_export_latency.store(new_avg, Ordering::Relaxed);

        self.last_export.store(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            Ordering::Relaxed,
        );
    }

    pub fn success_rate(&self) -> f64 {
        let total = self.total_exports.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            (self.successful_exports.load(Ordering::Relaxed) as f64) / (total as f64)
        }
    }
}

impl MetricsExporter {
    /// Create a new metrics exporter with the given configuration
    pub fn new(config: ExportConfig) -> Self {
        Self {
            targets: config.targets.clone(),
            stats: Arc::new(ExportStats::new()),
            shutdown_tx: None,
            export_handles: Vec::new(),
            config,
        }
    }

    /// Start the exporter with a metrics registry
    pub async fn start(&mut self, registry: Arc<ConcreteMetricsRegistry>) -> Result<()> {
        let (shutdown_tx, shutdown_rx) = mpsc::unbounded_channel();
        self.shutdown_tx = Some(shutdown_tx);

        // Start export tasks for each target
        for target in &self.targets {
            let export_handle = self
                .start_export_task(
                    target.clone(),
                    registry.clone(),
                    shutdown_rx.resubscribe().unwrap_or_else(|_| {
                        let (tx, rx) = mpsc::unbounded_channel();
                        let _ = tx.send(());
                        rx
                    }),
                )
                .await?;

            self.export_handles.push(export_handle);
        }

        Ok(())
    }

    /// Start an export task for a specific target
    async fn start_export_task(
        &self,
        target: ExportTarget,
        registry: Arc<ConcreteMetricsRegistry>,
        mut shutdown_rx: mpsc::UnboundedReceiver<()>,
    ) -> Result<JoinHandle<()>> {
        let stats = self.stats.clone();
        let export_interval = self.config.export_interval;

        let handle = tokio::spawn(async move {
            let mut interval_timer = interval(export_interval);
            interval_timer.set_missed_tick_behavior(MissedTickBehavior::Skip);

            let mut export_buffer = Vec::new();
            let mut retry_backoff = Duration::from_millis(100);

            loop {
                tokio::select! {
                    _ = interval_timer.tick() => {
                        let start = Instant::now();

                        // Get metrics snapshot
                        let snapshot = registry.snapshot();
                        let metrics_count = snapshot.metrics_count() as u64;

                        // Add to export buffer
                        export_buffer.push(snapshot);

                        // Check if we should export (batch size or time-based)
                        let should_export = export_buffer.len() >= target.batch_size
                            || export_buffer.first()
                                .map(|s| s.timestamp.elapsed() >= target.max_batch_delay)
                                .unwrap_or(false);

                        if should_export {
                            let export_result = Self::export_batch(&target, &export_buffer).await;
                            let latency_us = start.elapsed().as_micros() as u64;

                            match export_result {
                                Ok(_) => {
                                    stats.record_export(true, latency_us, metrics_count);
                                    export_buffer.clear();
                                    retry_backoff = Duration::from_millis(100); // Reset backoff
                                }
                                Err(e) => {
                                    stats.record_export(false, latency_us, metrics_count);
                                    eprintln!("Export failed for target {}: {:?}", target.name, e);

                                    // Implement exponential backoff
                                    tokio::time::sleep(retry_backoff).await;
                                    retry_backoff = std::cmp::min(retry_backoff * 2, Duration::from_secs(60));

                                    // Keep buffer for retry, but limit size to prevent memory growth
                                    if export_buffer.len() > target.max_buffer_size() {
                                        let excess = export_buffer.len() - target.max_buffer_size();
                                        export_buffer.drain(0..excess);
                                    }
                                }
                            }
                        }

                        stats.backlog_size.store(export_buffer.len() as u64, Ordering::Relaxed);
                    }
                    _ = shutdown_rx.recv() => {
                        // Shutdown signal received, export remaining buffer and exit
                        if !export_buffer.is_empty() {
                            let _ = Self::export_batch(&target, &export_buffer).await;
                        }
                        break;
                    }
                }
            }
        });

        Ok(handle)
    }

    /// Export a batch of metrics to a target
    async fn export_batch(target: &ExportTarget, snapshots: &[MetricsSnapshot]) -> Result<()> {
        match &target.target_type {
            ExportTargetType::Http => {
                Self::export_http(
                    &target.endpoint,
                    &target.headers,
                    target.auth.as_ref(),
                    snapshots,
                )
                .await
            },
            ExportTargetType::Prometheus => {
                Self::export_prometheus(
                    &target.endpoint,
                    &target.headers,
                    target.auth.as_ref(),
                    snapshots,
                )
                .await
            },
            ExportTargetType::Otlp => {
                Self::export_otlp(
                    &target.endpoint,
                    &target.headers,
                    target.auth.as_ref(),
                    snapshots,
                )
                .await
            },
            ExportTargetType::StatsD => Self::export_statsd(&target.endpoint, snapshots).await,
            ExportTargetType::InfluxDb => {
                Self::export_influxdb(
                    &target.endpoint,
                    &target.headers,
                    target.auth.as_ref(),
                    snapshots,
                )
                .await
            },
        }
    }

    /// Export to HTTP endpoint
    async fn export_http(
        url: &str,
        headers: &HashMap<String, String>,
        auth: Option<&BasicAuth>,
        snapshots: &[MetricsSnapshot],
    ) -> Result<()> {
        let client = reqwest::Client::new();
        let mut request = client.post(url);

        // Add headers
        for (key, value) in headers {
            request = request.header(key, value);
        }

        // Add authentication
        if let Some(auth) = auth {
            request = request.basic_auth(&auth.username, Some(&auth.password));
        }

        // Serialize metrics data
        let payload = serde_json::to_string(snapshots).map_err(|e| {
            TelemetryError::serialization(format!("JSON serialization failed: {}", e))
        })?;

        // Send request
        let response = request
            .header("Content-Type", "application/json")
            .body(payload)
            .send()
            .await
            .map_err(|e| TelemetryError::export(format!("HTTP request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(TelemetryError::export(format!(
                "HTTP export failed with status: {}",
                response.status()
            )));
        }

        Ok(())
    }

    /// Export to Prometheus endpoint
    async fn export_prometheus(
        url: &str,
        headers: &HashMap<String, String>,
        auth: Option<&BasicAuth>,
        snapshots: &[MetricsSnapshot],
    ) -> Result<()> {
        // Convert snapshots to Prometheus format
        let prometheus_data = Self::serialize_to_prometheus(snapshots)?;

        let client = reqwest::Client::new();
        let mut request = client.post(url);

        // Add headers
        for (key, value) in headers {
            request = request.header(key, value);
        }

        // Add authentication
        if let Some(auth) = auth {
            request = request.basic_auth(&auth.username, Some(&auth.password));
        }

        // Send request
        let response = request
            .header("Content-Type", "text/plain; version=0.0.4")
            .body(prometheus_data)
            .send()
            .await
            .map_err(|e| TelemetryError::export(format!("Prometheus export failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(TelemetryError::export(format!(
                "Prometheus export failed with status: {}",
                response.status()
            )));
        }

        Ok(())
    }

    /// Export to OTLP endpoint
    async fn export_otlp(
        url: &str,
        headers: &HashMap<String, String>,
        auth: Option<&BasicAuth>,
        snapshots: &[MetricsSnapshot],
    ) -> Result<()> {
        // Convert snapshots to OTLP format
        let otlp_data = Self::serialize_to_otlp(snapshots)?;

        let client = reqwest::Client::new();
        let mut request = client.post(url);

        // Add headers
        for (key, value) in headers {
            request = request.header(key, value);
        }

        // Add authentication
        if let Some(auth) = auth {
            request = request.basic_auth(&auth.username, Some(&auth.password));
        }

        // Send request
        let response = request
            .header("Content-Type", "application/x-protobuf")
            .body(otlp_data)
            .send()
            .await
            .map_err(|e| TelemetryError::export(format!("OTLP export failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(TelemetryError::export(format!(
                "OTLP export failed with status: {}",
                response.status()
            )));
        }

        Ok(())
    }

    /// Export to StatsD
    async fn export_statsd(endpoint: &str, snapshots: &[MetricsSnapshot]) -> Result<()> {
        // Convert snapshots to StatsD format and send via UDP
        let statsd_data = Self::serialize_to_statsd(snapshots)?;

        // Parse endpoint to get host and port
        let (host, port) = Self::parse_statsd_endpoint(endpoint)?;

        use tokio::net::UdpSocket;

        let socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .map_err(|e| TelemetryError::export(format!("UDP socket creation failed: {}", e)))?;

        socket
            .send_to(statsd_data.as_bytes(), format!("{}:{}", host, port))
            .await
            .map_err(|e| TelemetryError::export(format!("StatsD export failed: {}", e)))?;

        Ok(())
    }

    /// Export to InfluxDB
    async fn export_influxdb(
        url: &str,
        headers: &HashMap<String, String>,
        auth: Option<&BasicAuth>,
        snapshots: &[MetricsSnapshot],
    ) -> Result<()> {
        // Convert snapshots to InfluxDB line protocol
        let influx_data = Self::serialize_to_influxdb(snapshots)?;

        let client = reqwest::Client::new();
        let mut request = client.post(url);

        // Add headers
        for (key, value) in headers {
            request = request.header(key, value);
        }

        // Add authentication
        if let Some(auth) = auth {
            request = request.basic_auth(&auth.username, Some(&auth.password));
        }

        // Send request
        let response = request
            .header("Content-Type", "text/plain")
            .body(influx_data)
            .send()
            .await
            .map_err(|e| TelemetryError::export(format!("InfluxDB export failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(TelemetryError::export(format!(
                "InfluxDB export failed with status: {}",
                response.status()
            )));
        }

        Ok(())
    }

    /// Serialize snapshots to Prometheus format
    fn serialize_to_prometheus(snapshots: &[MetricsSnapshot]) -> Result<String> {
        let mut output = String::new();

        for snapshot in snapshots {
            for metric in snapshot.metrics() {
                output.push_str(&format!("{} {}\n", metric.name(), metric.value()));
            }
        }

        Ok(output)
    }

    /// Serialize snapshots to OTLP format
    fn serialize_to_otlp(snapshots: &[MetricsSnapshot]) -> Result<Vec<u8>> {
        // Simplified OTLP serialization - in practice would use protobuf
        let json_data = serde_json::to_string(snapshots).map_err(|e| {
            TelemetryError::serialization(format!("OTLP serialization failed: {}", e))
        })?;

        Ok(json_data.into_bytes())
    }

    /// Serialize snapshots to StatsD format
    fn serialize_to_statsd(snapshots: &[MetricsSnapshot]) -> Result<String> {
        let mut output = String::new();

        for snapshot in snapshots {
            for metric in snapshot.metrics() {
                output.push_str(&format!("{}:{}|g\n", metric.name(), metric.value()));
            }
        }

        Ok(output)
    }

    /// Serialize snapshots to InfluxDB line protocol
    fn serialize_to_influxdb(snapshots: &[MetricsSnapshot]) -> Result<String> {
        let mut output = String::new();

        for snapshot in snapshots {
            for metric in snapshot.metrics() {
                output.push_str(&format!("{} value={}\n", metric.name(), metric.value()));
            }
        }

        Ok(output)
    }

    /// Parse StatsD endpoint into host and port
    fn parse_statsd_endpoint(endpoint: &str) -> Result<(String, u16)> {
        if let Some((host, port_str)) = endpoint.split_once(':') {
            let port = port_str.parse::<u16>().map_err(|_| {
                TelemetryError::config(format!("Invalid port in StatsD endpoint: {}", endpoint))
            })?;
            Ok((host.to_string(), port))
        } else {
            Err(TelemetryError::config(format!("Invalid StatsD endpoint format: {}", endpoint)))
        }
    }

    /// Get export statistics
    pub fn stats(&self) -> &ExportStats {
        &self.stats
    }

    /// Shutdown the exporter gracefully
    pub async fn shutdown(&mut self) -> Result<()> {
        // Send shutdown signal
        if let Some(shutdown_tx) = &self.shutdown_tx {
            let _ = shutdown_tx.send(());
        }

        // Wait for all export tasks to complete
        for handle in self.export_handles.drain(..) {
            let _ = handle.await;
        }

        self.shutdown_tx = None;
        Ok(())
    }
}

/// Extension methods for MetricsSnapshot
impl MetricsSnapshot {
    /// Get the number of metrics in this snapshot
    pub fn metrics_count(&self) -> usize {
        // This would depend on the actual implementation of MetricsSnapshot
        // For now, return a placeholder
        0
    }

    /// Get an iterator over the metrics in this snapshot
    pub fn metrics(&self) -> impl Iterator<Item = &dyn Metric> {
        // This would depend on the actual implementation of MetricsSnapshot
        // For now, return an empty iterator
        std::iter::empty()
    }
}

/// Trait for metric values in snapshots
pub trait Metric {
    fn name(&self) -> &str;
    fn value(&self) -> f64;
}

/// Display implementation for ExportStats
impl std::fmt::Display for ExportStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ExportStats {{ total: {}, success: {}, failed: {}, success_rate: {:.2}%, metrics: {}, latency: {}μs, backlog: {} }}",
            self.total_exports.load(Ordering::Relaxed),
            self.successful_exports.load(Ordering::Relaxed),
            self.failed_exports.load(Ordering::Relaxed),
            self.success_rate() * 100.0,
            self.total_metrics.load(Ordering::Relaxed),
            self.avg_export_latency.load(Ordering::Relaxed),
            self.backlog_size.load(Ordering::Relaxed)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_export_stats_creation() {
        let stats = ExportStats::new();
        assert_eq!(stats.total_exports.load(Ordering::Relaxed), 0);
        assert_eq!(stats.successful_exports.load(Ordering::Relaxed), 0);
        assert_eq!(stats.failed_exports.load(Ordering::Relaxed), 0);
        assert_eq!(stats.success_rate(), 0.0);
    }

    #[test]
    fn test_export_stats_recording() {
        let stats = ExportStats::new();

        stats.record_export(true, 1000, 10);
        assert_eq!(stats.total_exports.load(Ordering::Relaxed), 1);
        assert_eq!(stats.successful_exports.load(Ordering::Relaxed), 1);
        assert_eq!(stats.failed_exports.load(Ordering::Relaxed), 0);
        assert_eq!(stats.success_rate(), 1.0);

        stats.record_export(false, 2000, 5);
        assert_eq!(stats.total_exports.load(Ordering::Relaxed), 2);
        assert_eq!(stats.successful_exports.load(Ordering::Relaxed), 1);
        assert_eq!(stats.failed_exports.load(Ordering::Relaxed), 1);
        assert_eq!(stats.success_rate(), 0.5);
    }

    #[test]
    fn test_prometheus_serialization() {
        let snapshots = vec![];
        let result = MetricsExporter::serialize_to_prometheus(&snapshots);
        assert!(result.is_ok());
    }

    #[test]
    fn test_exporter_creation() {
        let config = ExportConfig::default();
        let exporter = MetricsExporter::new(config);

        assert_eq!(exporter.targets.len(), 0);
        assert_eq!(exporter.stats.total_exports.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_statsd_endpoint_parsing() {
        let result = MetricsExporter::parse_statsd_endpoint("localhost:8125");
        assert!(result.is_ok());
        let (host, port) = result.unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, 8125);

        let invalid_result = MetricsExporter::parse_statsd_endpoint("invalid_endpoint");
        assert!(invalid_result.is_err());
    }
}
