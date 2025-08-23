//! Runtime diagnostics and health monitoring.
//!
//! Provides comprehensive health checking and diagnostic capabilities for the runtime system.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Represents the health status of a component
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded(String),
    Unhealthy(String),
}

impl HealthStatus {
    pub fn is_healthy(&self) -> bool {
        matches!(self, HealthStatus::Healthy)
    }

    pub fn is_degraded(&self) -> bool {
        matches!(self, HealthStatus::Degraded(_))
    }

    pub fn is_unhealthy(&self) -> bool {
        matches!(self, HealthStatus::Unhealthy(_))
    }

    pub fn message(&self) -> Option<&str> {
        match self {
            HealthStatus::Healthy => None,
            HealthStatus::Degraded(msg) | HealthStatus::Unhealthy(msg) => Some(msg),
        }
    }
}

impl Default for HealthStatus {
    fn default() -> Self {
        HealthStatus::Healthy
    }
}

/// Health check indicator trait
pub trait HealthIndicator: Send + Sync {
    fn name(&self) -> &str;
    async fn check_health(&self) -> HealthStatus;
    fn dependencies(&self) -> Vec<String>;
    fn health_status(&self) -> HealthStatus;
}

/// Diagnostic information for a component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticInfo {
    pub component_name: String,
    pub status: HealthStatus,
    pub last_check: Option<Instant>,
    pub metrics: HashMap<String, DiagnosticValue>,
    pub dependencies: Vec<String>,
}

/// Diagnostic value types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiagnosticValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Duration(Duration),
}

impl DiagnosticInfo {
    pub fn new(component_name: String) -> Self {
        Self {
            component_name,
            status: HealthStatus::Healthy,
            last_check: None,
            metrics: HashMap::new(),
            dependencies: Vec::new(),
        }
    }

    pub fn with_status(mut self, status: HealthStatus) -> Self {
        self.status = status;
        self
    }

    pub fn add_metric<T: Into<DiagnosticValue>>(mut self, key: String, value: T) -> Self {
        self.metrics.insert(key, value.into());
        self
    }
}

impl From<String> for DiagnosticValue {
    fn from(value: String) -> Self {
        DiagnosticValue::String(value)
    }
}

impl From<i64> for DiagnosticValue {
    fn from(value: i64) -> Self {
        DiagnosticValue::Integer(value)
    }
}

impl From<f64> for DiagnosticValue {
    fn from(value: f64) -> Self {
        DiagnosticValue::Float(value)
    }
}

impl From<bool> for DiagnosticValue {
    fn from(value: bool) -> Self {
        DiagnosticValue::Boolean(value)
    }
}

impl From<Duration> for DiagnosticValue {
    fn from(value: Duration) -> Self {
        DiagnosticValue::Duration(value)
    }
}

/// System-wide diagnostics collector
#[derive(Debug)]
pub struct DiagnosticsCollector {
    indicators: Vec<Box<dyn HealthIndicator>>,
    last_collection: Option<Instant>,
    collection_interval: Duration,
}

impl DiagnosticsCollector {
    pub fn new() -> Self {
        Self {
            indicators: Vec::new(),
            last_collection: None,
            collection_interval: Duration::from_secs(30),
        }
    }

    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.collection_interval = interval;
        self
    }

    pub fn add_indicator(&mut self, indicator: Box<dyn HealthIndicator>) {
        self.indicators.push(indicator);
    }

    pub async fn collect_diagnostics(&mut self) -> HashMap<String, DiagnosticInfo> {
        let mut diagnostics = HashMap::new();

        for indicator in &self.indicators {
            let status = indicator.check_health().await;
            let info = DiagnosticInfo::new(indicator.name().to_string()).with_status(status);

            diagnostics.insert(indicator.name().to_string(), info);
        }

        self.last_collection = Some(Instant::now());
        diagnostics
    }

    pub fn should_collect(&self) -> bool {
        match self.last_collection {
            Some(last) => last.elapsed() >= self.collection_interval,
            None => true,
        }
    }
}

impl Default for DiagnosticsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestIndicator {
        name: String,
        status: HealthStatus,
    }

    impl TestIndicator {
        fn new(name: String, status: HealthStatus) -> Self {
            Self { name, status }
        }
    }

    impl HealthIndicator for TestIndicator {
        fn name(&self) -> &str {
            &self.name
        }

        async fn check_health(&self) -> HealthStatus {
            self.status.clone()
        }

        fn dependencies(&self) -> Vec<String> {
            vec![]
        }

        fn health_status(&self) -> HealthStatus {
            self.status.clone()
        }
    }

    #[tokio::test]
    async fn test_diagnostics_collector() {
        let mut collector = DiagnosticsCollector::new();

        let indicator = TestIndicator::new("test".to_string(), HealthStatus::Healthy);
        collector.add_indicator(Box::new(indicator));

        let diagnostics = collector.collect_diagnostics().await;
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics.contains_key("test"));
    }

    #[test]
    fn test_health_status_methods() {
        let healthy = HealthStatus::Healthy;
        assert!(healthy.is_healthy());
        assert!(!healthy.is_degraded());
        assert!(!healthy.is_unhealthy());

        let degraded = HealthStatus::Degraded("slow".to_string());
        assert!(!degraded.is_healthy());
        assert!(degraded.is_degraded());
        assert!(!degraded.is_unhealthy());

        let unhealthy = HealthStatus::Unhealthy("failed".to_string());
        assert!(!unhealthy.is_healthy());
        assert!(!unhealthy.is_degraded());
        assert!(unhealthy.is_unhealthy());
    }
}
