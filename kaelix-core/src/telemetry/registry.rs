//! # Metrics Registry and Management
//!
//! Centralized metrics registry for definition management and metadata storage.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use crate::telemetry::{MetricsConfig, MetricsRegistryTrait, Result, TelemetryError};

// Re-export MetricKey from parent module for convenience
pub use crate::telemetry::MetricKey;

/// High-performance, thread-safe metrics registry
#[derive(Debug)]
pub struct MetricsRegistry {
    /// Metric definitions indexed by key
    metrics: DashMap<MetricKey, Arc<MetricDefinition>>,
    /// Metric families for organization
    families: DashMap<String, MetricFamily>,
    /// Registry configuration
    config: MetricsConfig,
    /// Registry statistics
    stats: RegistryStats,
}

/// Comprehensive metric definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricDefinition {
    pub key: MetricKey,
    pub metric_type: MetricType,
    pub description: String,
    pub unit: Option<String>,
    pub labels: Vec<String>,
    pub created_at: SystemTime,
    pub metadata: std::collections::HashMap<String, String>,
}

/// Metric type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetricType {
    Counter,
    Gauge,
    Histogram,
    Summary,
}

/// Metric family for grouping related metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricFamily {
    pub name: String,
    pub description: String,
    pub metric_type: MetricType,
    pub metrics: Vec<MetricKey>,
    pub created_at: SystemTime,
}

/// Registry operation statistics
#[derive(Debug, Clone, Default)]
pub struct RegistryStats {
    pub total_metrics: std::sync::atomic::AtomicU64,
    pub total_families: std::sync::atomic::AtomicU64,
    pub registration_count: std::sync::atomic::AtomicU64,
    pub lookup_count: std::sync::atomic::AtomicU64,
    pub memory_usage_bytes: std::sync::atomic::AtomicU64,
}

impl MetricsRegistry {
    /// Create a new metrics registry
    pub fn new(config: MetricsConfig) -> Self {
        Self {
            metrics: DashMap::new(),
            families: DashMap::new(),
            config,
            stats: RegistryStats::default(),
        }
    }

    /// Register a metric definition
    pub fn register_metric(&self, definition: MetricDefinition) -> Result<()> {
        let key = definition.key.clone();

        // Check if metric already exists
        if self.metrics.contains_key(&key) {
            return Err(TelemetryError::MetricAlreadyExists(key.to_string()));
        }

        // Insert metric definition
        self.metrics.insert(key.clone(), Arc::new(definition));

        // Update statistics
        self.stats.total_metrics.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.stats.registration_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        Ok(())
    }

    /// Get metric definition by key
    pub fn get_metric_definition(&self, key: &MetricKey) -> Option<Arc<MetricDefinition>> {
        self.stats.lookup_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.metrics.get(key).map(|entry| entry.clone())
    }

    /// List all registered metric keys
    pub fn list_metrics(&self) -> Vec<MetricKey> {
        self.metrics.iter().map(|entry| entry.key().clone()).collect()
    }

    /// Register a metric family
    pub fn register_family(&self, family: MetricFamily) -> Result<()> {
        let name = family.name.clone();

        if self.families.contains_key(&name) {
            return Err(TelemetryError::FamilyAlreadyExists(name));
        }

        self.families.insert(name, family);
        self.stats.total_families.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        Ok(())
    }

    /// Get metric family by name
    pub fn get_family(&self, name: &str) -> Option<MetricFamily> {
        self.families.get(name).map(|entry| entry.clone())
    }

    /// List all metric families
    pub fn list_families(&self) -> Vec<String> {
        self.families.iter().map(|entry| entry.key().clone()).collect()
    }

    /// Remove a metric definition
    pub fn unregister_metric(&self, key: &MetricKey) -> Result<()> {
        if self.metrics.remove(key).is_some() {
            self.stats.total_metrics.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        } else {
            Err(TelemetryError::MetricNotFound(key.to_string()))
        }
    }

    /// Remove a metric family
    pub fn unregister_family(&self, name: &str) -> Result<()> {
        if self.families.remove(name).is_some() {
            self.stats.total_families.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        } else {
            Err(TelemetryError::FamilyNotFound(name.to_string()))
        }
    }

    /// Get registry statistics
    pub fn stats(&self) -> RegistryStats {
        let mut stats = self.stats.clone();
        stats
            .memory_usage_bytes
            .store(self.calculate_memory_usage(), std::sync::atomic::Ordering::Relaxed);
        stats
    }

    /// Clear all metrics and families
    pub fn clear(&self) {
        self.metrics.clear();
        self.families.clear();
        self.stats.total_metrics.store(0, std::sync::atomic::Ordering::Relaxed);
        self.stats.total_families.store(0, std::sync::atomic::Ordering::Relaxed);
    }

    /// Search metrics by pattern
    pub fn search_metrics(&self, pattern: &str) -> Vec<MetricKey> {
        self.metrics
            .iter()
            .filter(|entry| entry.key().name.contains(pattern))
            .map(|entry| entry.key().clone())
            .collect()
    }

    /// Get metrics by type
    pub fn get_metrics_by_type(&self, metric_type: MetricType) -> Vec<MetricKey> {
        self.metrics
            .iter()
            .filter(|entry| entry.metric_type == metric_type)
            .map(|entry| entry.key().clone())
            .collect()
    }

    /// Validate metric definition
    pub fn validate_definition(&self, definition: &MetricDefinition) -> Result<()> {
        // Validate key
        if definition.key.name.is_empty() {
            return Err(TelemetryError::InvalidMetricName("Empty metric name".to_string()));
        }

        // Validate description
        if definition.description.is_empty() {
            return Err(TelemetryError::InvalidMetricName("Empty description".to_string()));
        }

        // Validate labels
        for label in &definition.labels {
            if label.is_empty() {
                return Err(TelemetryError::InvalidMetricName("Empty label".to_string()));
            }
        }

        Ok(())
    }

    /// Calculate approximate memory usage
    fn calculate_memory_usage(&self) -> u64 {
        let base_size = std::mem::size_of::<Self>() as u64;

        // Calculate metrics memory usage
        let metrics_size = self.metrics.len() as u64
            * (std::mem::size_of::<MetricKey>() as u64
                + std::mem::size_of::<Arc<MetricDefinition>>() as u64
                + std::mem::size_of::<MetricDefinition>() as u64);

        // Calculate families memory usage
        let families_size = self.families.len() as u64
            * (std::mem::size_of::<String>() as u64 + std::mem::size_of::<MetricFamily>() as u64);

        base_size + metrics_size + families_size
    }
}

impl MetricsRegistryTrait for MetricsRegistry {
    fn get_metric_definition(
        &self,
        key: crate::telemetry::MetricKey,
    ) -> Option<Arc<MetricDefinition>> {
        self.get_metric_definition(&key)
    }

    fn list_metrics(&self) -> Vec<crate::telemetry::MetricKey> {
        self.list_metrics()
    }

    fn search_metrics(&self, pattern: &str) -> Vec<crate::telemetry::MetricKey> {
        self.search_metrics(pattern)
    }

    fn get_metrics_by_type(&self, metric_type: MetricType) -> Vec<crate::telemetry::MetricKey> {
        self.get_metrics_by_type(metric_type)
    }
}

impl MetricType {
    /// Create metric type from metric key
    pub fn from_key(key: &MetricKey) -> Self {
        // Infer type from metric name patterns
        let name = &key.name;

        if name.ends_with("_total") || name.ends_with("_count") {
            MetricType::Counter
        } else if name.contains("_histogram") || name.contains("_bucket") {
            MetricType::Histogram
        } else if name.contains("_summary") || name.contains("_quantile") {
            MetricType::Summary
        } else {
            MetricType::Gauge
        }
    }

    /// Get default unit for metric type
    pub fn default_unit(&self) -> Option<&'static str> {
        match self {
            MetricType::Counter => Some("total"),
            MetricType::Gauge => None,
            MetricType::Histogram => Some("seconds"),
            MetricType::Summary => Some("seconds"),
        }
    }
}

impl MetricDefinition {
    /// Create a new metric definition
    pub fn new(key: MetricKey, metric_type: MetricType, description: String) -> Self {
        Self {
            key,
            metric_type,
            description,
            unit: metric_type.default_unit().map(|s| s.to_string()),
            labels: Vec::new(),
            created_at: SystemTime::now(),
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Add a label to the metric definition
    pub fn with_label(mut self, label: String) -> Self {
        self.labels.push(label);
        self
    }

    /// Set the unit for the metric
    pub fn with_unit(mut self, unit: String) -> Self {
        self.unit = Some(unit);
        self
    }

    /// Add metadata to the metric definition
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

impl MetricFamily {
    /// Create a new metric family
    pub fn new(name: String, description: String, metric_type: MetricType) -> Self {
        Self { name, description, metric_type, metrics: Vec::new(), created_at: SystemTime::now() }
    }

    /// Add a metric to the family
    pub fn add_metric(&mut self, key: MetricKey) {
        if !self.metrics.contains(&key) {
            self.metrics.push(key);
        }
    }

    /// Remove a metric from the family
    pub fn remove_metric(&mut self, key: &MetricKey) {
        self.metrics.retain(|k| k != key);
    }
}

impl RegistryStats {
    /// Get current metric count
    pub fn metric_count(&self) -> u64 {
        self.total_metrics.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get current family count
    pub fn family_count(&self) -> u64 {
        self.total_families.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get memory usage in bytes
    pub fn memory_usage(&self) -> u64 {
        self.memory_usage_bytes.load(std::sync::atomic::Ordering::Relaxed)
    }
}
