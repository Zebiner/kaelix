//! # Monitoring Module
//!
//! Real-time monitoring and alerting system for test execution.

pub mod alerts;
pub mod realtime;

// Re-export common types from alerts
pub use alerts::{
    AlertManager, AlertRule, AlertCondition, ComparisonOperator,
    NotificationChannel, NotificationError, AlertStats,
    SlackNotification, EmailNotification, WebhookNotification,
    create_default_alert_rules,
};

// Re-export types from dashboard for alerts
pub use crate::reporting::dashboard::{AlertSeverity, AlertState, Alert};

// Re-export real-time monitoring types
pub use realtime::{
    RealtimeMonitor, MetricCollector, MonitoringConfig, MonitoringStats,
    SystemResourceCollector, NetworkCollector, ApplicationMetricsCollector,
    create_default_monitor,
};