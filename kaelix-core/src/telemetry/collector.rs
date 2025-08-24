//! High-performance metrics collection system
//!
//! Provides efficient collection, aggregation, and querying of telemetry data
//! with minimal overhead and built-in time-series analysis.

use dashmap::DashMap;
use parking_lot::RwLock;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use crate::telemetry::{
    metrics::{AggregatedData, CollectorStats},
    registry::MetricType,
    HistogramData, MetricKey, MetricsConfig, MetricsRegistryTrait, MetricsSnapshot, QuantileEntry,
    Result, SummaryData,
};

/// High-performance metrics collector with time-series aggregation
#[derive(Debug)]
pub struct MetricsCollector {
    metrics: DashMap<MetricKey, Arc<RwLock<TimeSeriesMetric>>>,
    config: MetricsConfig,
    last_cleanup: RwLock<Instant>,
    stats: CollectorStats,
}

/// Individual metric with time-series data
#[derive(Debug)]
pub struct TimeSeriesMetric {
    metric_type: MetricType,
    values: VecDeque<TimestampedValue>,
    aggregated: AggregatedData,
    last_updated: Instant,
}

/// Timestamped metric value
#[derive(Debug, Clone)]
pub struct TimestampedValue {
    pub value: f64,
    pub timestamp: SystemTime,
}

/// Aggregation window for time-series data
#[derive(Debug, Clone)]
pub struct AggregationWindow {
    pub start: SystemTime,
    pub end: SystemTime,
    pub values: Vec<f64>,
    pub count: u64,
    pub sum: f64,
    pub min: f64,
    pub max: f64,
}

/// Query parameters for metrics collection
#[derive(Debug, Clone)]
pub struct MetricsQuery {
    pub keys: Vec<MetricKey>,
    pub start_time: Option<SystemTime>,
    pub end_time: Option<SystemTime>,
    pub aggregation_window: Option<Duration>,
    pub include_raw_values: bool,
}

/// Query result with aggregated data
#[derive(Debug)]
pub struct QueryResult {
    pub metrics: HashMap<MetricKey, MetricData>,
    pub query_duration: Duration,
    pub total_points: usize,
}

/// Individual metric data in query results
#[derive(Debug)]
pub struct MetricData {
    pub aggregated: AggregatedData,
    pub windows: Vec<AggregationWindow>,
    pub raw_values: Option<Vec<TimestampedValue>>,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new(config: MetricsConfig) -> Self {
        Self {
            metrics: DashMap::new(),
            config,
            last_cleanup: RwLock::new(Instant::now()),
            stats: CollectorStats::default(),
        }
    }

    /// Record a metric value
    pub fn record(&self, key: &MetricKey, value: f64) -> Result<()> {
        let timestamp = SystemTime::now();
        let metric = self.get_or_create_metric(key.clone());

        let mut metric_guard = metric.write();
        metric_guard.add_value(value, timestamp, &self.config)?;

        // Update collector stats
        self.stats.total_records.fetch_add(1, Ordering::Relaxed);

        // Perform cleanup if needed
        self.cleanup_if_needed()?;

        Ok(())
    }

    /// Record multiple metric values in batch
    pub fn record_batch(&self, values: Vec<(MetricKey, f64)>) -> Result<()> {
        let timestamp = SystemTime::now();

        for (key, value) in values {
            let metric = self.get_or_create_metric(key);
            let mut metric_guard = metric.write();
            metric_guard.add_value(value, timestamp, &self.config)?;
        }

        self.stats.total_records.fetch_add(1, Ordering::Relaxed);
        self.cleanup_if_needed()?;

        Ok(())
    }

    /// Query metrics with flexible parameters
    pub fn query(&self, query: MetricsQuery) -> Result<QueryResult> {
        let start_time = Instant::now();
        let mut result_metrics = HashMap::new();
        let mut total_points = 0;

        for key in query.keys {
            if let Some(metric_ref) = self.metrics.get(&key) {
                let metric = metric_ref.read();
                let data = metric.query(&query)?;
                total_points += data.raw_values.as_ref().map(|v| v.len()).unwrap_or(0);
                result_metrics.insert(key, data);
            }
        }

        Ok(QueryResult {
            metrics: result_metrics,
            query_duration: start_time.elapsed(),
            total_points,
        })
    }

    /// Get current metrics snapshot
    pub fn snapshot(&self) -> Result<MetricsSnapshot> {
        let mut metrics = HashMap::new();

        for entry in self.metrics.iter() {
            let metric = entry.value().read();
            metrics.insert(entry.key().clone(), metric.aggregated.clone());
        }

        Ok(MetricsSnapshot { timestamp: SystemTime::now(), metrics })
    }

    /// Get collector statistics
    pub fn stats(&self) -> CollectorStats {
        self.stats.clone()
    }

    /// Force cleanup of old metrics
    pub fn cleanup(&self) -> Result<()> {
        let cutoff = SystemTime::now() - self.config.retention_period;
        let mut removed_count = 0;

        // Remove old metrics
        self.metrics.retain(|_, metric| {
            let metric_guard = metric.read();
            if metric_guard.last_updated.elapsed() > self.config.retention_period {
                removed_count += 1;
                false
            } else {
                true
            }
        });

        // Cleanup individual metric time series
        for metric_ref in self.metrics.iter() {
            let mut metric = metric_ref.write();
            metric.cleanup_old_values(cutoff);
        }

        self.stats.cleanup_operations.fetch_add(1, Ordering::Relaxed);
        *self.last_cleanup.write() = Instant::now();

        Ok(())
    }

    fn get_or_create_metric(&self, key: MetricKey) -> Arc<RwLock<TimeSeriesMetric>> {
        self.metrics
            .entry(key.clone())
            .or_insert_with(|| {
                Arc::new(RwLock::new(TimeSeriesMetric::new(MetricType::from_key(&key))))
            })
            .clone()
    }

    fn cleanup_if_needed(&self) -> Result<()> {
        let last_cleanup = self.last_cleanup.read();
        if last_cleanup.elapsed() > self.config.cleanup_interval {
            drop(last_cleanup);
            self.cleanup()?;
        }
        Ok(())
    }
}

impl TimeSeriesMetric {
    fn new(metric_type: MetricType) -> Self {
        Self {
            metric_type,
            values: VecDeque::new(),
            aggregated: AggregatedData::default(),
            last_updated: Instant::now(),
        }
    }

    fn add_value(
        &mut self,
        value: f64,
        timestamp: SystemTime,
        config: &MetricsConfig,
    ) -> Result<()> {
        self.values.push_back(TimestampedValue { value, timestamp });
        self.update_aggregated(value);
        self.last_updated = Instant::now();

        // Limit the number of stored values
        while self.values.len() > config.max_values_per_metric {
            self.values.pop_front();
        }

        Ok(())
    }

    fn update_aggregated(&mut self, value: f64) {
        self.aggregated.count += 1;
        self.aggregated.sum += value;

        if self.aggregated.count == 1 {
            self.aggregated.min = value;
            self.aggregated.max = value;
        } else {
            self.aggregated.min = self.aggregated.min.min(value);
            self.aggregated.max = self.aggregated.max.max(value);
        }

        self.aggregated.average = self.aggregated.sum / self.aggregated.count as f64;
    }

    fn query(&self, query: &MetricsQuery) -> Result<MetricData> {
        let filtered_values: Vec<TimestampedValue> = self
            .values
            .iter()
            .filter(|v| {
                if let Some(start) = query.start_time {
                    if v.timestamp < start {
                        return false;
                    }
                }
                if let Some(end) = query.end_time {
                    if v.timestamp > end {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        let aggregated = if filtered_values.is_empty() {
            AggregatedData::default()
        } else {
            self.aggregate_values(&filtered_values)
        };

        let windows = if let Some(window_size) = query.aggregation_window {
            self.create_aggregation_windows(&filtered_values, window_size)?
        } else {
            vec![]
        };

        let raw_values = if query.include_raw_values {
            Some(filtered_values)
        } else {
            None
        };

        Ok(MetricData { aggregated, windows, raw_values })
    }

    fn aggregate_values(&self, values: &[TimestampedValue]) -> AggregatedData {
        if values.is_empty() {
            return AggregatedData::default();
        }

        let sum: f64 = values.iter().map(|v| v.value).sum();
        let count = values.len() as u64;
        let min = values.iter().map(|v| v.value).fold(f64::INFINITY, f64::min);
        let max = values.iter().map(|v| v.value).fold(f64::NEG_INFINITY, f64::max);
        let average = sum / count as f64;

        AggregatedData { count, sum, min, max, average }
    }

    fn create_aggregation_windows(
        &self,
        values: &[TimestampedValue],
        window_size: Duration,
    ) -> Result<Vec<AggregationWindow>> {
        if values.is_empty() {
            return Ok(vec![]);
        }

        let start_time = values.first().unwrap().timestamp;
        let end_time = values.last().unwrap().timestamp;
        let window_duration = window_size;

        let mut windows = Vec::new();
        let mut current_time = start_time;

        while current_time < end_time {
            let window_end = current_time + window_duration;

            let window_values: Vec<f64> = values
                .iter()
                .filter(|v| v.timestamp >= current_time && v.timestamp < window_end)
                .map(|v| v.value)
                .collect();

            if !window_values.is_empty() {
                let count = window_values.len() as u64;
                let sum: f64 = window_values.iter().sum();
                let min = window_values.iter().fold(f64::INFINITY, |a, &b| a.min(b));
                let max = window_values.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));

                windows.push(AggregationWindow {
                    start: current_time,
                    end: window_end,
                    values: window_values,
                    count,
                    sum,
                    min,
                    max,
                });
            }

            current_time = window_end;
        }

        Ok(windows)
    }

    fn cleanup_old_values(&mut self, cutoff: SystemTime) {
        self.values.retain(|v| v.timestamp >= cutoff);
    }
}

impl Default for AggregatedData {
    fn default() -> Self {
        Self { count: 0, sum: 0.0, min: 0.0, max: 0.0, average: 0.0 }
    }
}
