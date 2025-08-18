//! # Metrics Collection and Aggregation
//!
//! High-performance metrics collection and aggregation with time-series storage.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use dashmap::DashMap;
use parking_lot::RwLock;

use crate::telemetry::{
    TelemetryError, Result, MetricKey, MetricsConfig, MetricsSnapshot,
    CounterMetric, HistogramMetric, GaugeMetric, SummaryMetric,
    MetricDefinition, MetricType,
};

/// High-performance metrics collector with time-series aggregation
pub struct MetricsCollector {
    /// Time-series data store
    time_series: DashMap<MetricKey, TimeSeriesData>,
    
    /// Aggregation windows
    aggregation_windows: Vec<AggregationWindow>,
    
    /// Collection statistics
    stats: CollectionStats,
    
    /// Configuration
    config: MetricsConfig,
    
    /// Creation timestamp
    created_at: Instant,
}

impl MetricsCollector {
    /// Creates a new metrics collector
    pub fn new(config: MetricsConfig) -> Result<Self> {
        let aggregation_windows = vec![
            AggregationWindow::new(Duration::from_secs(60)),   // 1 minute
            AggregationWindow::new(Duration::from_secs(300)),  // 5 minutes
            AggregationWindow::new(Duration::from_secs(3600)), // 1 hour
        ];

        Ok(Self {
            time_series: DashMap::new(),
            aggregation_windows,
            stats: CollectionStats::new(),
            config,
            created_at: Instant::now(),
        })
    }

    /// Collects metrics from a snapshot and stores them in time-series
    pub fn collect_snapshot(&self, snapshot: MetricsSnapshot) -> Result<()> {
        let collection_start = Instant::now();

        // Process counters
        for counter in &snapshot.counters {
            self.store_counter_data(counter)?;
        }

        // Process histograms
        for histogram in &snapshot.histograms {
            self.store_histogram_data(histogram)?;
        }

        // Process gauges
        for gauge in &snapshot.gauges {
            self.store_gauge_data(gauge)?;
        }

        // Process summaries
        for summary in &snapshot.summaries {
            self.store_summary_data(summary)?;
        }

        // Update aggregation windows
        self.update_aggregations(&snapshot)?;

        // Update collection statistics
        let collection_time = collection_start.elapsed();
        self.stats.collections_total.fetch_add(1, Ordering::Relaxed);
        self.stats.collection_time_total_ns.fetch_add(
            collection_time.as_nanos() as u64,
            Ordering::Relaxed,
        );

        Ok(())
    }

    /// Stores counter data in time-series
    fn store_counter_data(&self, counter: &CounterMetric) -> Result<()> {
        let data_point = DataPoint {
            timestamp: counter.timestamp,
            value: counter.value as f64,
            labels: HashMap::new(), // Would be populated with actual labels
        };

        self.get_or_create_time_series(counter.key, MetricType::Counter)
            .add_data_point(data_point);

        Ok(())
    }

    /// Stores histogram data in time-series
    fn store_histogram_data(&self, histogram: &HistogramMetric) -> Result<()> {
        let data_point = DataPoint {
            timestamp: histogram.timestamp,
            value: histogram.sum / histogram.count as f64, // Average
            labels: HashMap::new(),
        };

        self.get_or_create_time_series(histogram.key, MetricType::Histogram)
            .add_data_point(data_point);

        // Store histogram-specific data
        let histogram_data = HistogramDataPoint {
            timestamp: histogram.timestamp,
            count: histogram.count,
            sum: histogram.sum,
            buckets: histogram.buckets.clone(),
            percentiles: histogram.percentiles.clone(),
        };

        self.get_or_create_time_series(histogram.key, MetricType::Histogram)
            .add_histogram_data(histogram_data);

        Ok(())
    }

    /// Stores gauge data in time-series
    fn store_gauge_data(&self, gauge: &GaugeMetric) -> Result<()> {
        let data_point = DataPoint {
            timestamp: gauge.timestamp,
            value: gauge.value,
            labels: HashMap::new(),
        };

        self.get_or_create_time_series(gauge.key, MetricType::Gauge)
            .add_data_point(data_point);

        Ok(())
    }

    /// Stores summary data in time-series
    fn store_summary_data(&self, summary: &SummaryMetric) -> Result<()> {
        let data_point = DataPoint {
            timestamp: summary.timestamp,
            value: summary.sum / summary.count as f64, // Average
            labels: HashMap::new(),
        };

        self.get_or_create_time_series(summary.key, MetricType::Summary)
            .add_data_point(data_point);

        // Store summary-specific data
        let summary_data = SummaryDataPoint {
            timestamp: summary.timestamp,
            count: summary.count,
            sum: summary.sum,
            quantiles: summary.quantiles.clone(),
        };

        self.get_or_create_time_series(summary.key, MetricType::Summary)
            .add_summary_data(summary_data);

        Ok(())
    }

    /// Gets or creates time-series data for a metric
    fn get_or_create_time_series(&self, key: MetricKey, metric_type: MetricType) -> Arc<TimeSeriesData> {
        self.time_series
            .entry(key)
            .or_insert_with(|| Arc::new(TimeSeriesData::new(key, metric_type, self.config.retention_period)))
            .clone()
    }

    /// Updates aggregation windows with new data
    fn update_aggregations(&self, snapshot: &MetricsSnapshot) -> Result<()> {
        for window in &self.aggregation_windows {
            window.update_with_snapshot(snapshot)?;
        }
        Ok(())
    }

    /// Queries time-series data for a specific metric
    pub fn query_time_series(
        &self,
        key: MetricKey,
        start_time: SystemTime,
        end_time: SystemTime,
    ) -> Option<TimeSeriesQuery> {
        let time_series = self.time_series.get(&key)?;
        Some(time_series.query_range(start_time, end_time))
    }

    /// Queries aggregated data for a specific window
    pub fn query_aggregated(
        &self,
        key: MetricKey,
        window_duration: Duration,
    ) -> Option<AggregatedData> {
        let window = self.aggregation_windows
            .iter()
            .find(|w| w.duration == window_duration)?;
        
        window.get_aggregated_data(key)
    }

    /// Returns collection statistics
    pub fn collection_stats(&self) -> CollectionStatsSnapshot {
        let collections_total = self.stats.collections_total.load(Ordering::Relaxed);
        let collection_time_total_ns = self.stats.collection_time_total_ns.load(Ordering::Relaxed);

        CollectionStatsSnapshot {
            collections_total,
            average_collection_time_ns: if collections_total > 0 {
                collection_time_total_ns / collections_total
            } else {
                0
            },
            time_series_count: self.time_series.len(),
            memory_usage_bytes: self.memory_usage(),
            retention_period: self.config.retention_period,
            uptime: self.created_at.elapsed(),
        }
    }

    /// Performs cleanup of expired data
    pub fn cleanup_expired_data(&self) -> Result<CleanupStats> {
        let cleanup_start = Instant::now();
        let mut removed_data_points = 0;
        let mut removed_time_series = 0;

        let cutoff_time = SystemTime::now() - self.config.retention_period;

        // Clean up each time series
        let mut keys_to_remove = Vec::new();
        for entry in self.time_series.iter() {
            let key = *entry.key();
            let time_series = entry.value();
            
            let removed_points = time_series.cleanup_expired_data(cutoff_time);
            removed_data_points += removed_points;

            // Remove empty time series
            if time_series.is_empty() {
                keys_to_remove.push(key);
            }
        }

        // Remove empty time series
        for key in keys_to_remove {
            self.time_series.remove(&key);
            removed_time_series += 1;
        }

        // Clean up aggregation windows
        for window in &self.aggregation_windows {
            window.cleanup_expired_data(cutoff_time);
        }

        Ok(CleanupStats {
            removed_data_points,
            removed_time_series,
            cleanup_duration: cleanup_start.elapsed(),
        })
    }

    /// Returns estimated memory usage
    pub fn memory_usage(&self) -> usize {
        let base_size = std::mem::size_of::<Self>();
        let time_series_size: usize = self.time_series
            .iter()
            .map(|entry| entry.value().memory_usage())
            .sum();
        let aggregation_size: usize = self.aggregation_windows
            .iter()
            .map(|window| window.memory_usage())
            .sum();

        base_size + time_series_size + aggregation_size
    }

    /// Returns the number of active time series
    pub fn active_time_series_count(&self) -> usize {
        self.time_series.len()
    }

    /// Lists all metric keys with active time series
    pub fn list_active_metrics(&self) -> Vec<MetricKey> {
        self.time_series.iter().map(|entry| *entry.key()).collect()
    }
}

/// Time-series data storage for a single metric
pub struct TimeSeriesData {
    /// Metric key
    key: MetricKey,
    
    /// Metric type
    metric_type: MetricType,
    
    /// Data points sorted by timestamp
    data_points: RwLock<Vec<DataPoint>>,
    
    /// Histogram-specific data
    histogram_data: RwLock<Vec<HistogramDataPoint>>,
    
    /// Summary-specific data
    summary_data: RwLock<Vec<SummaryDataPoint>>,
    
    /// Retention period
    retention_period: Duration,
    
    /// Creation timestamp
    created_at: Instant,
}

impl TimeSeriesData {
    /// Creates new time-series data
    pub fn new(key: MetricKey, metric_type: MetricType, retention_period: Duration) -> Self {
        Self {
            key,
            metric_type,
            data_points: RwLock::new(Vec::new()),
            histogram_data: RwLock::new(Vec::new()),
            summary_data: RwLock::new(Vec::new()),
            retention_period,
            created_at: Instant::now(),
        }
    }

    /// Adds a data point
    pub fn add_data_point(&self, data_point: DataPoint) {
        let mut data_points = self.data_points.write();
        
        // Insert in sorted order (binary search for performance)
        match data_points.binary_search_by(|probe| probe.timestamp.cmp(&data_point.timestamp)) {
            Ok(pos) => data_points[pos] = data_point, // Replace existing
            Err(pos) => data_points.insert(pos, data_point), // Insert new
        }
    }

    /// Adds histogram-specific data
    pub fn add_histogram_data(&self, histogram_data: HistogramDataPoint) {
        let mut data = self.histogram_data.write();
        
        match data.binary_search_by(|probe| probe.timestamp.cmp(&histogram_data.timestamp)) {
            Ok(pos) => data[pos] = histogram_data,
            Err(pos) => data.insert(pos, histogram_data),
        }
    }

    /// Adds summary-specific data
    pub fn add_summary_data(&self, summary_data: SummaryDataPoint) {
        let mut data = self.summary_data.write();
        
        match data.binary_search_by(|probe| probe.timestamp.cmp(&summary_data.timestamp)) {
            Ok(pos) => data[pos] = summary_data,
            Err(pos) => data.insert(pos, summary_data),
        }
    }

    /// Queries data within a time range
    pub fn query_range(&self, start_time: SystemTime, end_time: SystemTime) -> TimeSeriesQuery {
        let data_points = self.data_points.read();
        
        let filtered_points: Vec<DataPoint> = data_points
            .iter()
            .filter(|point| point.timestamp >= start_time && point.timestamp <= end_time)
            .cloned()
            .collect();

        TimeSeriesQuery {
            key: self.key,
            metric_type: self.metric_type,
            data_points: filtered_points,
            start_time,
            end_time,
        }
    }

    /// Cleans up expired data points
    pub fn cleanup_expired_data(&self, cutoff_time: SystemTime) -> usize {
        let mut removed_count = 0;

        // Clean up regular data points
        {
            let mut data_points = self.data_points.write();
            let original_len = data_points.len();
            data_points.retain(|point| point.timestamp >= cutoff_time);
            removed_count += original_len - data_points.len();
        }

        // Clean up histogram data
        {
            let mut histogram_data = self.histogram_data.write();
            let original_len = histogram_data.len();
            histogram_data.retain(|point| point.timestamp >= cutoff_time);
            removed_count += original_len - histogram_data.len();
        }

        // Clean up summary data
        {
            let mut summary_data = self.summary_data.write();
            let original_len = summary_data.len();
            summary_data.retain(|point| point.timestamp >= cutoff_time);
            removed_count += original_len - summary_data.len();
        }

        removed_count
    }

    /// Returns true if time series has no data
    pub fn is_empty(&self) -> bool {
        let data_points = self.data_points.read();
        let histogram_data = self.histogram_data.read();
        let summary_data = self.summary_data.read();
        
        data_points.is_empty() && histogram_data.is_empty() && summary_data.is_empty()
    }

    /// Returns estimated memory usage
    pub fn memory_usage(&self) -> usize {
        let base_size = std::mem::size_of::<Self>();
        let data_points_size = {
            let data_points = self.data_points.read();
            data_points.len() * std::mem::size_of::<DataPoint>()
        };
        let histogram_size = {
            let histogram_data = self.histogram_data.read();
            histogram_data.len() * std::mem::size_of::<HistogramDataPoint>()
        };
        let summary_size = {
            let summary_data = self.summary_data.read();
            summary_data.len() * std::mem::size_of::<SummaryDataPoint>()
        };

        base_size + data_points_size + histogram_size + summary_size
    }
}

/// Individual data point in time-series
#[derive(Debug, Clone)]
pub struct DataPoint {
    pub timestamp: SystemTime,
    pub value: f64,
    pub labels: HashMap<String, String>,
}

/// Histogram-specific data point
#[derive(Debug, Clone)]
pub struct HistogramDataPoint {
    pub timestamp: SystemTime,
    pub count: u64,
    pub sum: f64,
    pub buckets: Vec<crate::telemetry::HistogramBucket>,
    pub percentiles: HashMap<f64, f64>,
}

/// Summary-specific data point
#[derive(Debug, Clone)]
pub struct SummaryDataPoint {
    pub timestamp: SystemTime,
    pub count: u64,
    pub sum: f64,
    pub quantiles: HashMap<f64, f64>,
}

/// Aggregation window for time-series data
pub struct AggregationWindow {
    /// Window duration
    duration: Duration,
    
    /// Aggregated data by metric key
    aggregated_data: DashMap<MetricKey, AggregatedData>,
    
    /// Last update timestamp
    last_update: RwLock<Instant>,
}

impl AggregationWindow {
    /// Creates a new aggregation window
    pub fn new(duration: Duration) -> Self {
        Self {
            duration,
            aggregated_data: DashMap::new(),
            last_update: RwLock::new(Instant::now()),
        }
    }

    /// Updates the window with a metrics snapshot
    pub fn update_with_snapshot(&self, snapshot: &MetricsSnapshot) -> Result<()> {
        *self.last_update.write() = Instant::now();

        // Update aggregations for each metric type
        for counter in &snapshot.counters {
            self.update_counter_aggregation(counter);
        }

        for histogram in &snapshot.histograms {
            self.update_histogram_aggregation(histogram);
        }

        for gauge in &snapshot.gauges {
            self.update_gauge_aggregation(gauge);
        }

        for summary in &snapshot.summaries {
            self.update_summary_aggregation(summary);
        }

        Ok(())
    }

    /// Updates counter aggregation
    fn update_counter_aggregation(&self, counter: &CounterMetric) {
        let mut aggregated = self.aggregated_data
            .entry(counter.key)
            .or_insert_with(|| AggregatedData::new(counter.key, MetricType::Counter));

        aggregated.update_with_value(counter.value as f64, counter.timestamp);
    }

    /// Updates histogram aggregation
    fn update_histogram_aggregation(&self, histogram: &HistogramMetric) {
        let mut aggregated = self.aggregated_data
            .entry(histogram.key)
            .or_insert_with(|| AggregatedData::new(histogram.key, MetricType::Histogram));

        aggregated.update_with_histogram(histogram);
    }

    /// Updates gauge aggregation
    fn update_gauge_aggregation(&self, gauge: &GaugeMetric) {
        let mut aggregated = self.aggregated_data
            .entry(gauge.key)
            .or_insert_with(|| AggregatedData::new(gauge.key, MetricType::Gauge));

        aggregated.update_with_value(gauge.value, gauge.timestamp);
    }

    /// Updates summary aggregation
    fn update_summary_aggregation(&self, summary: &SummaryMetric) {
        let mut aggregated = self.aggregated_data
            .entry(summary.key)
            .or_insert_with(|| AggregatedData::new(summary.key, MetricType::Summary));

        aggregated.update_with_summary(summary);
    }

    /// Gets aggregated data for a metric
    pub fn get_aggregated_data(&self, key: MetricKey) -> Option<AggregatedData> {
        self.aggregated_data.get(&key).map(|entry| entry.value().clone())
    }

    /// Cleans up expired aggregated data
    pub fn cleanup_expired_data(&self, cutoff_time: SystemTime) {
        let mut keys_to_remove = Vec::new();
        
        for entry in self.aggregated_data.iter() {
            if entry.value().last_update_time < cutoff_time {
                keys_to_remove.push(*entry.key());
            }
        }

        for key in keys_to_remove {
            self.aggregated_data.remove(&key);
        }
    }

    /// Returns estimated memory usage
    pub fn memory_usage(&self) -> usize {
        let base_size = std::mem::size_of::<Self>();
        let aggregated_size: usize = self.aggregated_data
            .iter()
            .map(|entry| std::mem::size_of::<AggregatedData>())
            .sum();

        base_size + aggregated_size
    }
}

/// Aggregated metric data
#[derive(Debug, Clone)]
pub struct AggregatedData {
    /// Metric key
    pub key: MetricKey,
    
    /// Metric type
    pub metric_type: MetricType,
    
    /// Sample count
    pub sample_count: u64,
    
    /// Minimum value
    pub min_value: f64,
    
    /// Maximum value
    pub max_value: f64,
    
    /// Average value
    pub avg_value: f64,
    
    /// Sum of all values
    pub sum_value: f64,
    
    /// Last update timestamp
    pub last_update_time: SystemTime,
    
    /// First sample timestamp
    pub first_sample_time: SystemTime,
}

impl AggregatedData {
    /// Creates new aggregated data
    pub fn new(key: MetricKey, metric_type: MetricType) -> Self {
        let now = SystemTime::now();
        Self {
            key,
            metric_type,
            sample_count: 0,
            min_value: f64::INFINITY,
            max_value: f64::NEG_INFINITY,
            avg_value: 0.0,
            sum_value: 0.0,
            last_update_time: now,
            first_sample_time: now,
        }
    }

    /// Updates aggregation with a new value
    pub fn update_with_value(&mut self, value: f64, timestamp: SystemTime) {
        if self.sample_count == 0 {
            self.first_sample_time = timestamp;
            self.min_value = value;
            self.max_value = value;
            self.sum_value = value;
            self.avg_value = value;
        } else {
            self.min_value = self.min_value.min(value);
            self.max_value = self.max_value.max(value);
            self.sum_value += value;
            self.avg_value = self.sum_value / (self.sample_count + 1) as f64;
        }

        self.sample_count += 1;
        self.last_update_time = timestamp;
    }

    /// Updates aggregation with histogram data
    pub fn update_with_histogram(&mut self, histogram: &HistogramMetric) {
        let avg_value = if histogram.count > 0 {
            histogram.sum / histogram.count as f64
        } else {
            0.0
        };

        self.update_with_value(avg_value, histogram.timestamp);
    }

    /// Updates aggregation with summary data
    pub fn update_with_summary(&mut self, summary: &SummaryMetric) {
        let avg_value = if summary.count > 0 {
            summary.sum / summary.count as f64
        } else {
            0.0
        };

        self.update_with_value(avg_value, summary.timestamp);
    }
}

/// Time-series query result
#[derive(Debug, Clone)]
pub struct TimeSeriesQuery {
    pub key: MetricKey,
    pub metric_type: MetricType,
    pub data_points: Vec<DataPoint>,
    pub start_time: SystemTime,
    pub end_time: SystemTime,
}

/// Collection statistics
#[derive(Debug)]
pub struct CollectionStats {
    pub collections_total: AtomicU64,
    pub collection_time_total_ns: AtomicU64,
}

impl CollectionStats {
    pub fn new() -> Self {
        Self {
            collections_total: AtomicU64::new(0),
            collection_time_total_ns: AtomicU64::new(0),
        }
    }
}

/// Collection statistics snapshot
#[derive(Debug, Clone)]
pub struct CollectionStatsSnapshot {
    pub collections_total: u64,
    pub average_collection_time_ns: u64,
    pub time_series_count: usize,
    pub memory_usage_bytes: usize,
    pub retention_period: Duration,
    pub uptime: Duration,
}

/// Cleanup statistics
#[derive(Debug, Clone)]
pub struct CleanupStats {
    pub removed_data_points: usize,
    pub removed_time_series: usize,
    pub cleanup_duration: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn test_metrics_collector_creation() {
        let config = MetricsConfig::default();
        let collector = MetricsCollector::new(config).unwrap();
        
        assert_eq!(collector.active_time_series_count(), 0);
        assert_eq!(collector.aggregation_windows.len(), 3);
    }

    #[test]
    fn test_time_series_data_points() {
        let retention_period = Duration::from_secs(3600);
        let time_series = TimeSeriesData::new(
            MetricKey::MessagesProcessed,
            MetricType::Counter,
            retention_period,
        );

        let now = SystemTime::now();
        let data_point = DataPoint {
            timestamp: now,
            value: 42.0,
            labels: HashMap::new(),
        };

        time_series.add_data_point(data_point.clone());
        
        let query = time_series.query_range(
            now - Duration::from_secs(60),
            now + Duration::from_secs(60),
        );
        
        assert_eq!(query.data_points.len(), 1);
        assert_eq!(query.data_points[0].value, 42.0);
    }

    #[test]
    fn test_aggregation_window() {
        let window = AggregationWindow::new(Duration::from_secs(300));
        
        let counter = CounterMetric {
            key: MetricKey::MessagesProcessed,
            value: 100,
            timestamp: SystemTime::now(),
        };

        window.update_counter_aggregation(&counter);
        
        let aggregated = window.get_aggregated_data(MetricKey::MessagesProcessed).unwrap();
        assert_eq!(aggregated.sample_count, 1);
        assert_eq!(aggregated.sum_value, 100.0);
        assert_eq!(aggregated.avg_value, 100.0);
    }

    #[test]
    fn test_aggregated_data_updates() {
        let mut aggregated = AggregatedData::new(MetricKey::MessagesProcessed, MetricType::Counter);
        
        let now = SystemTime::now();
        aggregated.update_with_value(10.0, now);
        aggregated.update_with_value(20.0, now);
        aggregated.update_with_value(30.0, now);
        
        assert_eq!(aggregated.sample_count, 3);
        assert_eq!(aggregated.min_value, 10.0);
        assert_eq!(aggregated.max_value, 30.0);
        assert_eq!(aggregated.sum_value, 60.0);
        assert_eq!(aggregated.avg_value, 20.0);
    }

    #[test]
    fn test_time_series_cleanup() {
        let retention_period = Duration::from_secs(60);
        let time_series = TimeSeriesData::new(
            MetricKey::MessagesProcessed,
            MetricType::Counter,
            retention_period,
        );

        let now = SystemTime::now();
        
        // Add recent data point
        let recent_point = DataPoint {
            timestamp: now,
            value: 42.0,
            labels: HashMap::new(),
        };
        time_series.add_data_point(recent_point);

        // Add old data point
        let old_point = DataPoint {
            timestamp: now - Duration::from_secs(120), // 2 minutes ago
            value: 24.0,
            labels: HashMap::new(),
        };
        time_series.add_data_point(old_point);

        // Cleanup with cutoff 90 seconds ago
        let cutoff = now - Duration::from_secs(90);
        let removed_count = time_series.cleanup_expired_data(cutoff);
        
        assert_eq!(removed_count, 1); // Only old point should be removed
        
        let query = time_series.query_range(
            now - Duration::from_secs(180),
            now + Duration::from_secs(60),
        );
        
        assert_eq!(query.data_points.len(), 1);
        assert_eq!(query.data_points[0].value, 42.0);
    }

    #[test]
    fn test_collection_stats() {
        let config = MetricsConfig::default();
        let collector = MetricsCollector::new(config).unwrap();
        
        let stats = collector.collection_stats();
        assert_eq!(stats.collections_total, 0);
        assert_eq!(stats.time_series_count, 0);
        assert!(stats.memory_usage_bytes > 0);
        assert!(stats.uptime.as_nanos() > 0);
    }

    #[test]
    fn test_memory_usage_estimation() {
        let config = MetricsConfig::default();
        let collector = MetricsCollector::new(config).unwrap();
        
        let initial_usage = collector.memory_usage();
        assert!(initial_usage > 0);
        
        // Memory usage should be reasonable
        assert!(initial_usage < 1_000_000); // Less than 1MB for empty collector
    }
}