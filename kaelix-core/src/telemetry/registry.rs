//! # Metrics Registry and Management
//!
//! Centralized metrics registry for definition management and metadata storage.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use dashmap::DashMap;
use serde::{Serialize, Deserialize};

use crate::telemetry::{TelemetryError, Result, MetricKey, MetricsConfig, MetricsSnapshot};

/// Centralized metrics registry
pub struct MetricsRegistry {
    /// Registered metric definitions
    metrics: DashMap<MetricKey, MetricDefinition>,
    
    /// Metric families for grouping related metrics
    families: DashMap<String, MetricFamily>,
    
    /// Registry metadata
    metadata: RegistryMetadata,
    
    /// Configuration
    config: MetricsConfig,
    
    /// Creation time
    created_at: Instant,
}

impl MetricsRegistry {
    /// Creates a new metrics registry
    pub fn new(config: MetricsConfig) -> Result<Self> {
        Ok(Self {
            metrics: DashMap::new(),
            families: DashMap::new(),
            metadata: RegistryMetadata {
                registry_id: uuid::Uuid::new_v4().to_string(),
                created_at: std::time::SystemTime::now(),
                version: "1.0.0".to_string(),
            },
            config,
            created_at: Instant::now(),
        })
    }

    /// Registers a new metric definition
    pub fn register_metric(&mut self, definition: MetricDefinition) -> Result<()> {
        // Validate metric definition
        self.validate_metric_definition(&definition)?;
        
        // Check for conflicts
        if self.metrics.contains_key(&definition.key) {
            return Err(TelemetryError::registry_with_size(
                format!("Metric key {:?} already registered", definition.key),
                self.metrics.len(),
            ));
        }

        // Check cardinality limits
        if self.metrics.len() >= self.config.max_cardinality {
            return Err(TelemetryError::registry_with_size(
                format!("Registry at capacity: {}", self.config.max_cardinality),
                self.metrics.len(),
            ));
        }

        // Register metric in appropriate family
        let family_name = self.extract_family_name(&definition.name);
        self.ensure_family_exists(&family_name, &definition);

        // Store the definition
        self.metrics.insert(definition.key, definition);
        
        Ok(())
    }

    /// Gets a metric definition by key
    pub fn get_metric(&self, key: MetricKey) -> Option<&MetricDefinition> {
        self.metrics.get(&key).map(|entry| entry.value())
    }

    /// Lists all registered metric keys
    pub fn list_metrics(&self) -> Vec<MetricKey> {
        self.metrics.iter().map(|entry| *entry.key()).collect()
    }

    /// Gets metric definitions by family
    pub fn get_metrics_by_family(&self, family_name: &str) -> Vec<&MetricDefinition> {
        self.metrics
            .iter()
            .filter(|entry| {
                let metric_family = self.extract_family_name(&entry.value().name);
                metric_family == family_name
            })
            .map(|entry| entry.value())
            .collect()
    }

    /// Lists all metric families
    pub fn list_families(&self) -> Vec<String> {
        self.families.iter().map(|entry| entry.key().clone()).collect()
    }

    /// Gets a metric family by name
    pub fn get_family(&self, name: &str) -> Option<MetricFamily> {
        self.families.get(name).map(|entry| entry.value().clone())
    }

    /// Collects metrics with their definitions for export
    pub fn collect_with_metadata(&self) -> Vec<MetricWithDefinition> {
        self.metrics
            .iter()
            .map(|entry| MetricWithDefinition {
                key: *entry.key(),
                definition: entry.value().clone(),
            })
            .collect()
    }

    /// Exports metrics in various formats
    pub fn export_metrics(&self, format: ExportFormat) -> Result<Vec<u8>> {
        match format {
            ExportFormat::Prometheus => self.export_prometheus(),
            ExportFormat::OpenMetrics => self.export_openmetrics(),
            ExportFormat::Json => self.export_json(),
        }
    }

    /// Returns registry statistics
    pub fn registry_stats(&self) -> RegistryStats {
        RegistryStats {
            total_metrics: self.metrics.len(),
            total_families: self.families.len(),
            memory_usage_bytes: self.memory_usage(),
            uptime: self.created_at.elapsed(),
            created_at: self.metadata.created_at,
        }
    }

    /// Validates a metric definition
    fn validate_metric_definition(&self, definition: &MetricDefinition) -> Result<()> {
        // Validate metric name format (Prometheus-compatible)
        if !self.is_valid_metric_name(&definition.name) {
            return Err(TelemetryError::registry(
                format!("Invalid metric name: {}", definition.name)
            ));
        }

        // Validate label names
        for label in &definition.labels {
            if !self.is_valid_label_name(label) {
                return Err(TelemetryError::registry(
                    format!("Invalid label name: {}", label)
                ));
            }
        }

        // Validate unit if present
        if let Some(ref unit) = definition.unit {
            if !self.is_valid_unit(unit) {
                return Err(TelemetryError::registry(
                    format!("Invalid unit: {}", unit)
                ));
            }
        }

        // Validate metric type compatibility
        self.validate_metric_type(&definition.metric_type)?;

        Ok(())
    }

    /// Validates metric name format
    fn is_valid_metric_name(&self, name: &str) -> bool {
        // Prometheus metric name rules:
        // - Must match [a-zA-Z_:][a-zA-Z0-9_:]*
        // - Cannot start with reserved prefixes
        if name.is_empty() {
            return false;
        }

        let first_char = name.chars().next().unwrap();
        if !matches!(first_char, 'a'..='z' | 'A'..='Z' | '_' | ':') {
            return false;
        }

        name.chars().all(|c| matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | ':'))
    }

    /// Validates label name format
    fn is_valid_label_name(&self, name: &str) -> bool {
        // Label names must match [a-zA-Z_][a-zA-Z0-9_]*
        if name.is_empty() || name.starts_with("__") {
            return false; // Reserved prefix
        }

        let first_char = name.chars().next().unwrap();
        if !matches!(first_char, 'a'..='z' | 'A'..='Z' | '_') {
            return false;
        }

        name.chars().all(|c| matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_'))
    }

    /// Validates unit format
    fn is_valid_unit(&self, unit: &str) -> bool {
        // Basic unit validation - could be expanded
        !unit.is_empty() && unit.len() <= 20
    }

    /// Validates metric type
    fn validate_metric_type(&self, metric_type: &MetricType) -> Result<()> {
        match metric_type {
            MetricType::Counter | MetricType::Gauge => Ok(()),
            MetricType::Histogram | MetricType::Summary => {
                // These types require additional validation
                Ok(())
            }
        }
    }

    /// Extracts family name from metric name
    fn extract_family_name(&self, metric_name: &str) -> String {
        // Extract base name before any suffixes
        if let Some(pos) = metric_name.find('_') {
            let base = &metric_name[..pos];
            // Check if it's a known suffix pattern
            let suffix = &metric_name[pos..];
            if matches!(suffix, "_total" | "_seconds" | "_bytes" | "_count" | "_sum" | "_bucket") {
                return base.to_string();
            }
        }
        
        metric_name.to_string()
    }

    /// Ensures a metric family exists
    fn ensure_family_exists(&mut self, family_name: &str, definition: &MetricDefinition) {
        if !self.families.contains_key(family_name) {
            let family = MetricFamily {
                name: family_name.to_string(),
                help: definition.description.clone(),
                metric_type: definition.metric_type,
                unit: definition.unit.clone(),
                created_at: std::time::SystemTime::now(),
            };
            self.families.insert(family_name.to_string(), family);
        }
    }

    /// Exports metrics in Prometheus format
    fn export_prometheus(&self) -> Result<Vec<u8>> {
        let mut output = String::new();
        
        // Group metrics by family
        for family_entry in self.families.iter() {
            let family_name = family_entry.key();
            let family = family_entry.value();
            
            // Write family help
            output.push_str(&format!("# HELP {} {}\n", family_name, family.help));
            
            // Write family type
            let type_str = match family.metric_type {
                MetricType::Counter => "counter",
                MetricType::Gauge => "gauge",
                MetricType::Histogram => "histogram",
                MetricType::Summary => "summary",
            };
            output.push_str(&format!("# TYPE {} {}\n", family_name, type_str));
            
            // Write unit if present
            if let Some(ref unit) = family.unit {
                output.push_str(&format!("# UNIT {} {}\n", family_name, unit));
            }
            
            // Get metrics in this family
            let family_metrics = self.get_metrics_by_family(family_name);
            for metric in family_metrics {
                output.push_str(&format!("{}\n", self.format_prometheus_metric(metric)));
            }
            
            output.push('\n');
        }
        
        Ok(output.into_bytes())
    }

    /// Formats a metric in Prometheus format
    fn format_prometheus_metric(&self, definition: &MetricDefinition) -> String {
        // This is a simplified version - would need actual metric values
        // In a real implementation, this would fetch current metric values
        format!("{} 0", definition.name)
    }

    /// Exports metrics in OpenMetrics format
    fn export_openmetrics(&self) -> Result<Vec<u8>> {
        // OpenMetrics format is similar to Prometheus but with stricter rules
        // For simplicity, using Prometheus format here
        self.export_prometheus()
    }

    /// Exports metrics in JSON format
    fn export_json(&self) -> Result<Vec<u8>> {
        let export_data = JsonExport {
            metadata: self.metadata.clone(),
            metrics: self.collect_with_metadata(),
            families: self.families
                .iter()
                .map(|entry| entry.value().clone())
                .collect(),
            stats: self.registry_stats(),
        };
        
        serde_json::to_vec_pretty(&export_data)
            .map_err(|e| TelemetryError::serialization_with_format(
                format!("JSON export error: {}", e),
                "json"
            ))
    }

    /// Returns estimated memory usage
    pub fn memory_usage(&self) -> usize {
        let base_size = std::mem::size_of::<Self>();
        let metrics_size = self.metrics.len() * std::mem::size_of::<(MetricKey, MetricDefinition)>();
        let families_size = self.families.len() * std::mem::size_of::<(String, MetricFamily)>();
        
        base_size + metrics_size + families_size
    }
}

/// Metric definition with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricDefinition {
    pub key: MetricKey,
    pub name: String,
    pub description: String,
    pub metric_type: MetricType,
    pub labels: Vec<String>,
    pub unit: Option<String>,
}

/// Metric type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetricType {
    /// Monotonically increasing counter
    Counter,
    /// Current value that can go up or down
    Gauge,
    /// Distribution of values with buckets
    Histogram,
    /// Distribution of values with quantiles
    Summary,
}

/// Metric family grouping related metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricFamily {
    pub name: String,
    pub help: String,
    pub metric_type: MetricType,
    pub unit: Option<String>,
    pub created_at: std::time::SystemTime,
}

/// Registry metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryMetadata {
    pub registry_id: String,
    pub created_at: std::time::SystemTime,
    pub version: String,
}

/// Metric with its definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricWithDefinition {
    pub key: MetricKey,
    pub definition: MetricDefinition,
}

/// Registry statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryStats {
    pub total_metrics: usize,
    pub total_families: usize,
    pub memory_usage_bytes: usize,
    pub uptime: Duration,
    pub created_at: std::time::SystemTime,
}

/// Export format options
#[derive(Debug, Clone, Copy)]
pub enum ExportFormat {
    Prometheus,
    OpenMetrics,
    Json,
}

/// JSON export structure
#[derive(Debug, Serialize, Deserialize)]
struct JsonExport {
    metadata: RegistryMetadata,
    metrics: Vec<MetricWithDefinition>,
    families: Vec<MetricFamily>,
    stats: RegistryStats,
}

/// Registry error types specific to metric registration
#[derive(Debug, Clone)]
pub enum RegistryError {
    MetricAlreadyExists(MetricKey),
    InvalidMetricName(String),
    InvalidLabelName(String),
    CapacityExceeded(usize),
    ExportError(String),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MetricAlreadyExists(key) => write!(f, "Metric already exists: {:?}", key),
            Self::InvalidMetricName(name) => write!(f, "Invalid metric name: {}", name),
            Self::InvalidLabelName(name) => write!(f, "Invalid label name: {}", name),
            Self::CapacityExceeded(size) => write!(f, "Registry capacity exceeded: {}", size),
            Self::ExportError(msg) => write!(f, "Export error: {}", msg),
        }
    }
}

impl std::error::Error for RegistryError {}

/// Implementation of the MetricsRegistry trait for use by MetricsCollector
impl crate::telemetry::metrics::MetricsRegistry for MetricsRegistry {
    fn get_metric_definition(&self, key: MetricKey) -> Option<&MetricDefinition> {
        self.get_metric(key)
    }
    
    fn list_metrics(&self) -> Vec<MetricKey> {
        self.list_metrics()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let config = MetricsConfig::default();
        let registry = MetricsRegistry::new(config).unwrap();
        
        assert_eq!(registry.metrics.len(), 0);
        assert_eq!(registry.families.len(), 0);
        assert!(!registry.metadata.registry_id.is_empty());
    }

    #[test]
    fn test_metric_registration() {
        let config = MetricsConfig::default();
        let mut registry = MetricsRegistry::new(config).unwrap();
        
        let definition = MetricDefinition {
            key: MetricKey::MessagesProcessed,
            name: "messages_processed_total".to_string(),
            description: "Total messages processed".to_string(),
            metric_type: MetricType::Counter,
            labels: vec!["component".to_string()],
            unit: None,
        };
        
        registry.register_metric(definition.clone()).unwrap();
        
        let retrieved = registry.get_metric(MetricKey::MessagesProcessed).unwrap();
        assert_eq!(retrieved.name, definition.name);
        assert_eq!(retrieved.metric_type, MetricType::Counter);
        assert_eq!(retrieved.labels, vec!["component".to_string()]);
    }

    #[test]
    fn test_duplicate_metric_registration() {
        let config = MetricsConfig::default();
        let mut registry = MetricsRegistry::new(config).unwrap();
        
        let definition = MetricDefinition {
            key: MetricKey::MessagesProcessed,
            name: "test_metric".to_string(),
            description: "Test metric".to_string(),
            metric_type: MetricType::Counter,
            labels: vec![],
            unit: None,
        };
        
        registry.register_metric(definition.clone()).unwrap();
        
        // Second registration should fail
        assert!(registry.register_metric(definition).is_err());
    }

    #[test]
    fn test_metric_name_validation() {
        let config = MetricsConfig::default();
        let registry = MetricsRegistry::new(config).unwrap();
        
        // Valid names
        assert!(registry.is_valid_metric_name("test_metric"));
        assert!(registry.is_valid_metric_name("TestMetric"));
        assert!(registry.is_valid_metric_name("_internal_metric"));
        assert!(registry.is_valid_metric_name("http:requests:total"));
        
        // Invalid names
        assert!(!registry.is_valid_metric_name(""));
        assert!(!registry.is_valid_metric_name("9metric")); // Starts with number
        assert!(!registry.is_valid_metric_name("metric-name")); // Contains dash
        assert!(!registry.is_valid_metric_name("metric name")); // Contains space
    }

    #[test]
    fn test_label_name_validation() {
        let config = MetricsConfig::default();
        let registry = MetricsRegistry::new(config).unwrap();
        
        // Valid label names
        assert!(registry.is_valid_label_name("label"));
        assert!(registry.is_valid_label_name("test_label"));
        assert!(registry.is_valid_label_name("_internal"));
        
        // Invalid label names
        assert!(!registry.is_valid_label_name(""));
        assert!(!registry.is_valid_label_name("9label")); // Starts with number
        assert!(!registry.is_valid_label_name("__reserved")); // Reserved prefix
        assert!(!registry.is_valid_label_name("label-name")); // Contains dash
    }

    #[test]
    fn test_family_extraction() {
        let config = MetricsConfig::default();
        let registry = MetricsRegistry::new(config).unwrap();
        
        assert_eq!(registry.extract_family_name("http_requests_total"), "http_requests");
        assert_eq!(registry.extract_family_name("duration_seconds"), "duration");
        assert_eq!(registry.extract_family_name("memory_bytes"), "memory");
        assert_eq!(registry.extract_family_name("custom_metric"), "custom_metric");
    }

    #[test]
    fn test_metric_family_creation() {
        let config = MetricsConfig::default();
        let mut registry = MetricsRegistry::new(config).unwrap();
        
        let definition = MetricDefinition {
            key: MetricKey::MessagesProcessed,
            name: "messages_processed_total".to_string(),
            description: "Total messages processed".to_string(),
            metric_type: MetricType::Counter,
            labels: vec![],
            unit: Some("messages".to_string()),
        };
        
        registry.register_metric(definition).unwrap();
        
        let families = registry.list_families();
        assert_eq!(families.len(), 1);
        assert_eq!(families[0], "messages_processed");
        
        let family = registry.get_family("messages_processed").unwrap();
        assert_eq!(family.name, "messages_processed");
        assert_eq!(family.metric_type, MetricType::Counter);
        assert_eq!(family.unit, Some("messages".to_string()));
    }

    #[test]
    fn test_prometheus_export() {
        let config = MetricsConfig::default();
        let mut registry = MetricsRegistry::new(config).unwrap();
        
        let definition = MetricDefinition {
            key: MetricKey::MessagesProcessed,
            name: "messages_processed_total".to_string(),
            description: "Total messages processed".to_string(),
            metric_type: MetricType::Counter,
            labels: vec![],
            unit: Some("messages".to_string()),
        };
        
        registry.register_metric(definition).unwrap();
        
        let export_data = registry.export_metrics(ExportFormat::Prometheus).unwrap();
        let export_string = String::from_utf8(export_data).unwrap();
        
        assert!(export_string.contains("# HELP messages_processed Total messages processed"));
        assert!(export_string.contains("# TYPE messages_processed counter"));
        assert!(export_string.contains("# UNIT messages_processed messages"));
    }

    #[test]
    fn test_json_export() {
        let config = MetricsConfig::default();
        let mut registry = MetricsRegistry::new(config).unwrap();
        
        let definition = MetricDefinition {
            key: MetricKey::MessagesProcessed,
            name: "test_metric".to_string(),
            description: "Test metric".to_string(),
            metric_type: MetricType::Gauge,
            labels: vec!["label1".to_string()],
            unit: None,
        };
        
        registry.register_metric(definition).unwrap();
        
        let export_data = registry.export_metrics(ExportFormat::Json).unwrap();
        let export_json: serde_json::Value = serde_json::from_slice(&export_data).unwrap();
        
        assert!(export_json["metadata"].is_object());
        assert!(export_json["metrics"].is_array());
        assert!(export_json["families"].is_array());
        assert!(export_json["stats"].is_object());
    }

    #[test]
    fn test_registry_stats() {
        let config = MetricsConfig::default();
        let mut registry = MetricsRegistry::new(config).unwrap();
        
        // Add some metrics
        for i in 0..5 {
            let definition = MetricDefinition {
                key: unsafe { std::mem::transmute(i as u32) }, // Unsafe but for testing
                name: format!("metric_{}", i),
                description: format!("Test metric {}", i),
                metric_type: MetricType::Counter,
                labels: vec![],
                unit: None,
            };
            registry.register_metric(definition).unwrap();
        }
        
        let stats = registry.registry_stats();
        assert_eq!(stats.total_metrics, 5);
        assert_eq!(stats.total_families, 5);
        assert!(stats.memory_usage_bytes > 0);
        assert!(stats.uptime.as_nanos() > 0);
    }

    #[test]
    fn test_capacity_limits() {
        let mut config = MetricsConfig::default();
        config.max_cardinality = 2; // Very small limit for testing
        
        let mut registry = MetricsRegistry::new(config).unwrap();
        
        // Register up to limit
        for i in 0..2 {
            let definition = MetricDefinition {
                key: unsafe { std::mem::transmute(i as u32) },
                name: format!("metric_{}", i),
                description: format!("Test metric {}", i),
                metric_type: MetricType::Counter,
                labels: vec![],
                unit: None,
            };
            registry.register_metric(definition).unwrap();
        }
        
        // Next registration should fail
        let definition = MetricDefinition {
            key: unsafe { std::mem::transmute(2u32) },
            name: "metric_2".to_string(),
            description: "Test metric 2".to_string(),
            metric_type: MetricType::Counter,
            labels: vec![],
            unit: None,
        };
        
        assert!(registry.register_metric(definition).is_err());
    }
}