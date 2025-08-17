//! # Alerting System
//!
//! Comprehensive alerting system for test monitoring with multiple notification channels
//! and configurable alert rules for performance and reliability monitoring.

use crate::metrics::MetricsSnapshot;
use crate::reporting::dashboard::{Alert, AlertCondition as DashboardAlertCondition, AlertSeverity, AlertState};
use async_trait::async_trait;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Comparison operators for alert conditions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComparisonOperator {
    /// Greater than
    GreaterThan,
    /// Less than
    LessThan,
    /// Equal to (with tolerance)
    Equal,
    /// Not equal to
    NotEqual,
    /// Greater than or equal to
    GreaterThanOrEqual,
    /// Less than or equal to
    LessThanOrEqual,
}

/// Enhanced alert condition with more flexibility
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertCondition {
    /// Throughput below threshold
    ThroughputBelow(u64),
    /// Latency above threshold  
    LatencyAbove(Duration),
    /// Error rate above percentage
    ErrorRateAbove(f64),
    /// Memory usage above threshold in bytes
    MemoryUsageAbove(u64),
    /// CPU usage above percentage
    CpuUsageAbove(f64),
    /// Custom metric condition
    CustomMetric {
        /// Metric name
        metric: String,
        /// Threshold value
        threshold: f64,
        /// Comparison operator
        operator: ComparisonOperator,
    },
    /// Rate of change condition
    RateOfChange {
        /// Metric name
        metric: String,
        /// Rate threshold per second
        rate_threshold: f64,
        /// Time window for rate calculation
        time_window: Duration,
    },
    /// Absence of data
    NoData {
        /// Maximum time without data
        max_silence: Duration,
    },
}

/// Alert rule configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    /// Rule name
    pub name: String,
    /// Rule description
    pub description: String,
    /// Alert condition
    pub condition: AlertCondition,
    /// Alert severity
    pub severity: AlertSeverity,
    /// Cooldown period to prevent spam
    pub cooldown: Duration,
    /// Whether the rule is enabled
    pub enabled: bool,
    /// Tags for categorization
    pub tags: HashMap<String, String>,
    /// Custom notification channels for this rule
    pub notification_channels: Vec<String>,
    /// Evaluation interval
    pub evaluation_interval: Duration,
    /// Number of consecutive evaluations that must be true
    pub consecutive_failures: u32,
}

impl AlertRule {
    /// Creates a new alert rule
    pub fn new(name: String, condition: AlertCondition, severity: AlertSeverity) -> Self {
        Self {
            name,
            description: String::new(),
            condition,
            severity,
            cooldown: Duration::from_secs(300), // 5 minutes
            enabled: true,
            tags: HashMap::new(),
            notification_channels: Vec::new(),
            evaluation_interval: Duration::from_secs(30),
            consecutive_failures: 1,
        }
    }
    
    /// Sets the rule description
    pub fn with_description(mut self, description: String) -> Self {
        self.description = description;
        self
    }
    
    /// Sets the cooldown period
    pub fn with_cooldown(mut self, cooldown: Duration) -> Self {
        self.cooldown = cooldown;
        self
    }
    
    /// Adds a tag to the rule
    pub fn with_tag(mut self, key: String, value: String) -> Self {
        self.tags.insert(key, value);
        self
    }
    
    /// Adds a notification channel
    pub fn with_notification_channel(mut self, channel: String) -> Self {
        self.notification_channels.push(channel);
        self
    }
    
    /// Sets the number of consecutive failures required
    pub fn with_consecutive_failures(mut self, count: u32) -> Self {
        self.consecutive_failures = count;
        self
    }
}

/// Notification channel trait for sending alerts
#[async_trait]
pub trait NotificationChannel: Send + Sync {
    /// Sends an alert notification
    async fn send_alert(&self, alert: &Alert) -> Result<(), NotificationError>;
    
    /// Returns the channel name
    fn name(&self) -> &str;
    
    /// Returns whether the channel is enabled
    fn is_enabled(&self) -> bool {
        true
    }
    
    /// Validates the channel configuration
    async fn validate_config(&self) -> Result<(), NotificationError> {
        Ok(())
    }
}

/// Notification error types
#[derive(Debug, thiserror::Error)]
pub enum NotificationError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("Authentication error: {0}")]
    Authentication(String),
    #[error("Configuration error: {0}")]
    Configuration(String),
    #[error("Rate limit exceeded")]
    RateLimit,
    #[error("Channel unavailable: {0}")]
    Unavailable(String),
}

/// Slack notification channel
#[derive(Debug)]
pub struct SlackNotification {
    name: String,
    webhook_url: String,
    channel: Option<String>,
    username: Option<String>,
    enabled: bool,
    #[cfg(feature = "monitoring")]
    client: reqwest::Client,
}

impl SlackNotification {
    /// Creates a new Slack notification channel
    pub fn new(webhook_url: String) -> Self {
        Self {
            name: "slack".to_string(),
            webhook_url,
            channel: None,
            username: Some("MemoryStreamer Tests".to_string()),
            enabled: true,
            #[cfg(feature = "monitoring")]
            client: reqwest::Client::new(),
        }
    }
    
    /// Sets the Slack channel
    pub fn with_channel(mut self, channel: String) -> Self {
        self.channel = Some(channel);
        self
    }
    
    /// Sets the username
    pub fn with_username(mut self, username: String) -> Self {
        self.username = Some(username);
        self
    }
}

#[async_trait]
impl NotificationChannel for SlackNotification {
    async fn send_alert(&self, alert: &Alert) -> Result<(), NotificationError> {
        if !self.enabled {
            return Ok(());
        }
        
        #[cfg(feature = "monitoring")]
        {
            let color = match alert.severity {
                AlertSeverity::Critical | AlertSeverity::Emergency => "danger",
                AlertSeverity::Warning => "warning",
                AlertSeverity::Info => "good",
            };
            
            let payload = serde_json::json!({
                "username": self.username,
                "channel": self.channel,
                "attachments": [
                    {
                        "color": color,
                        "title": format!("🚨 Alert: {}", alert.name),
                        "text": alert.description,
                        "fields": [
                            {
                                "title": "Severity",
                                "value": format!("{:?}", alert.severity),
                                "short": true
                            },
                            {
                                "title": "Metric",
                                "value": alert.metric_name,
                                "short": true
                            },
                            {
                                "title": "Message",
                                "value": alert.message.as_deref().unwrap_or("No details available"),
                                "short": false
                            }
                        ],
                        "footer": "MemoryStreamer Test Monitoring",
                        "ts": SystemTime::now()
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs()
                    }
                ]
            });
            
            let response = self.client
                .post(&self.webhook_url)
                .json(&payload)
                .send()
                .await
                .map_err(|e| NotificationError::Network(e.to_string()))?;
            
            if !response.status().is_success() {
                return Err(NotificationError::Network(format!(
                    "Slack API error: {}",
                    response.status()
                )));
            }
            
            info!("Sent Slack notification for alert: {}", alert.name);
        }
        
        #[cfg(not(feature = "monitoring"))]
        {
            info!("Would send Slack notification for alert: {} (monitoring feature disabled)", alert.name);
        }
        
        Ok(())
    }
    
    fn name(&self) -> &str {
        &self.name
    }
    
    fn is_enabled(&self) -> bool {
        self.enabled
    }
    
    async fn validate_config(&self) -> Result<(), NotificationError> {
        if self.webhook_url.is_empty() {
            return Err(NotificationError::Configuration(
                "Slack webhook URL is required".to_string()
            ));
        }
        
        if !self.webhook_url.starts_with("https://hooks.slack.com/") {
            return Err(NotificationError::Configuration(
                "Invalid Slack webhook URL format".to_string()
            ));
        }
        
        Ok(())
    }
}

/// Email notification channel
#[derive(Debug)]
pub struct EmailNotification {
    name: String,
    smtp_server: String,
    smtp_port: u16,
    username: String,
    password: String,
    from_address: String,
    to_addresses: Vec<String>,
    enabled: bool,
}

impl EmailNotification {
    /// Creates a new email notification channel
    pub fn new(
        smtp_server: String,
        smtp_port: u16,
        username: String,
        password: String,
        from_address: String,
        to_addresses: Vec<String>,
    ) -> Self {
        Self {
            name: "email".to_string(),
            smtp_server,
            smtp_port,
            username,
            password,
            from_address,
            to_addresses,
            enabled: true,
        }
    }
}

#[async_trait]
impl NotificationChannel for EmailNotification {
    async fn send_alert(&self, alert: &Alert) -> Result<(), NotificationError> {
        if !self.enabled {
            return Ok(());
        }
        
        // In a real implementation, this would use an SMTP library like lettre
        // For now, we'll just log the email that would be sent
        let subject = format!("[{:?}] Alert: {}", alert.severity, alert.name);
        let body = format!(
            r#"Alert Details:
            
Name: {}
Severity: {:?}
Metric: {}
Message: {}
Triggered At: {:?}

This is an automated alert from MemoryStreamer Test Monitoring.
"#,
            alert.name,
            alert.severity,
            alert.metric_name,
            alert.message.as_deref().unwrap_or("No details available"),
            alert.triggered_at
        );
        
        info!(
            "Would send email to {:?} with subject '{}' and body: {}",
            self.to_addresses, subject, body
        );
        
        // Simulate email sending delay
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        Ok(())
    }
    
    fn name(&self) -> &str {
        &self.name
    }
    
    fn is_enabled(&self) -> bool {
        self.enabled
    }
    
    async fn validate_config(&self) -> Result<(), NotificationError> {
        if self.smtp_server.is_empty() {
            return Err(NotificationError::Configuration(
                "SMTP server is required".to_string()
            ));
        }
        
        if self.to_addresses.is_empty() {
            return Err(NotificationError::Configuration(
                "At least one recipient email address is required".to_string()
            ));
        }
        
        Ok(())
    }
}

/// Webhook notification channel
#[derive(Debug)]
pub struct WebhookNotification {
    name: String,
    url: String,
    headers: HashMap<String, String>,
    enabled: bool,
    #[cfg(feature = "monitoring")]
    client: reqwest::Client,
}

impl WebhookNotification {
    /// Creates a new webhook notification channel
    pub fn new(url: String) -> Self {
        Self {
            name: "webhook".to_string(),
            url,
            headers: HashMap::new(),
            enabled: true,
            #[cfg(feature = "monitoring")]
            client: reqwest::Client::new(),
        }
    }
    
    /// Adds a header to the webhook request
    pub fn with_header(mut self, key: String, value: String) -> Self {
        self.headers.insert(key, value);
        self
    }
}

#[async_trait]
impl NotificationChannel for WebhookNotification {
    async fn send_alert(&self, alert: &Alert) -> Result<(), NotificationError> {
        if !self.enabled {
            return Ok(());
        }
        
        #[cfg(feature = "monitoring")]
        {
            let payload = serde_json::json!({
                "alert": {
                    "id": alert.id,
                    "name": alert.name,
                    "description": alert.description,
                    "severity": alert.severity,
                    "state": alert.state,
                    "metric_name": alert.metric_name,
                    "message": alert.message,
                    "triggered_at": alert.triggered_at
                        .map(|t| t.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs()),
                    "resolved_at": alert.resolved_at
                        .map(|t| t.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs())
                },
                "timestamp": SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                "source": "memorystreamer-tests"
            });
            
            let mut request = self.client.post(&self.url).json(&payload);
            
            for (key, value) in &self.headers {
                request = request.header(key, value);
            }
            
            let response = request
                .send()
                .await
                .map_err(|e| NotificationError::Network(e.to_string()))?;
            
            if !response.status().is_success() {
                return Err(NotificationError::Network(format!(
                    "Webhook error: {}",
                    response.status()
                )));
            }
            
            info!("Sent webhook notification for alert: {}", alert.name);
        }
        
        #[cfg(not(feature = "monitoring"))]
        {
            info!("Would send webhook notification for alert: {} (monitoring feature disabled)", alert.name);
        }
        
        Ok(())
    }
    
    fn name(&self) -> &str {
        &self.name
    }
    
    fn is_enabled(&self) -> bool {
        self.enabled
    }
    
    async fn validate_config(&self) -> Result<(), NotificationError> {
        if self.url.is_empty() {
            return Err(NotificationError::Configuration(
                "Webhook URL is required".to_string()
            ));
        }
        
        if !self.url.starts_with("http://") && !self.url.starts_with("https://") {
            return Err(NotificationError::Configuration(
                "Webhook URL must be a valid HTTP/HTTPS URL".to_string()
            ));
        }
        
        Ok(())
    }
}

/// Alert manager that coordinates alert evaluation and notifications
#[derive(Debug)]
pub struct AlertManager {
    /// Alert rules
    rules: Vec<AlertRule>,
    /// Notification channels
    notifications: HashMap<String, Box<dyn NotificationChannel>>,
    /// Active alerts
    active_alerts: HashMap<String, Alert>,
    /// Alert history for rate limiting and cooldowns
    alert_history: HashMap<String, Vec<SystemTime>>,
    /// Control channel for commands
    control_tx: Option<mpsc::UnboundedSender<AlertCommand>>,
    /// Failure counts for consecutive failure tracking
    failure_counts: HashMap<String, u32>,
}

#[derive(Debug)]
enum AlertCommand {
    Stop,
    AddRule(AlertRule),
    RemoveRule(String),
    EnableRule(String),
    DisableRule(String),
    TestRule(String),
}

impl AlertManager {
    /// Creates a new alert manager
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            notifications: HashMap::new(),
            active_alerts: HashMap::new(),
            alert_history: HashMap::new(),
            control_tx: None,
            failure_counts: HashMap::new(),
        }
    }
    
    /// Adds an alert rule
    pub fn add_rule(&mut self, rule: AlertRule) {
        info!("Adding alert rule: {}", rule.name);
        self.rules.push(rule);
    }
    
    /// Adds a notification channel
    pub fn add_notification_channel(&mut self, channel: Box<dyn NotificationChannel>) {
        let name = channel.name().to_string();
        info!("Adding notification channel: {}", name);
        self.notifications.insert(name, channel);
    }
    
    /// Evaluates metrics against all alert rules
    pub async fn evaluate_metrics(&mut self, metrics: &MetricsSnapshot) -> Vec<Alert> {
        let mut triggered_alerts = Vec::new();
        
        for rule in &self.rules {
            if !rule.enabled {
                continue;
            }
            
            let is_triggered = self.evaluate_condition(&rule.condition, metrics).await;
            let rule_key = rule.name.clone();
            
            if is_triggered {
                // Increment failure count
                let failure_count = self.failure_counts.entry(rule_key.clone()).or_insert(0);
                *failure_count += 1;
                
                // Check if we've reached consecutive failure threshold
                if *failure_count >= rule.consecutive_failures {
                    // Check cooldown
                    if self.is_in_cooldown(&rule_key, rule.cooldown) {
                        debug!("Alert {} is in cooldown period", rule.name);
                        continue;
                    }
                    
                    let alert = self.create_alert_from_rule(rule, metrics);
                    
                    // Send notifications
                    self.send_notifications(&alert, rule).await;
                    
                    // Track alert
                    self.active_alerts.insert(rule_key.clone(), alert.clone());
                    self.record_alert_trigger(&rule_key);
                    
                    triggered_alerts.push(alert);
                    
                    // Reset failure count after triggering
                    self.failure_counts.insert(rule_key, 0);
                }
            } else {
                // Reset failure count on success
                self.failure_counts.insert(rule_key, 0);
                
                // Resolve alert if it was active
                if let Some(mut alert) = self.active_alerts.remove(&rule_key) {
                    alert.state = AlertState::Resolved;
                    alert.resolved_at = Some(SystemTime::now());
                    
                    info!("Alert resolved: {}", alert.name);
                    // Could send resolution notifications here
                }
            }
        }
        
        triggered_alerts
    }
    
    /// Starts the alert manager background task
    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let (control_tx, mut control_rx) = mpsc::unbounded_channel();
        self.control_tx = Some(control_tx);
        
        info!("Starting alert manager with {} rules", self.rules.len());
        
        // Validate all notification channels
        for channel in self.notifications.values() {
            if let Err(e) = channel.validate_config().await {
                warn!("Notification channel {} validation failed: {}", channel.name(), e);
            }
        }
        
        // Background task for handling commands
        tokio::spawn(async move {
            while let Some(command) = control_rx.recv().await {
                match command {
                    AlertCommand::Stop => {
                        info!("Stopping alert manager");
                        break;
                    }
                    AlertCommand::AddRule(rule) => {
                        info!("Adding rule via command: {}", rule.name);
                        // Would need to modify rules in a thread-safe way
                    }
                    AlertCommand::RemoveRule(name) => {
                        info!("Removing rule via command: {}", name);
                    }
                    AlertCommand::EnableRule(name) => {
                        info!("Enabling rule via command: {}", name);
                    }
                    AlertCommand::DisableRule(name) => {
                        info!("Disabling rule via command: {}", name);
                    }
                    AlertCommand::TestRule(name) => {
                        info!("Testing rule via command: {}", name);
                    }
                }
            }
        });
        
        Ok(())
    }
    
    /// Stops the alert manager
    pub async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(control_tx) = &self.control_tx {
            control_tx.send(AlertCommand::Stop)?;
        }
        Ok(())
    }
    
    /// Gets current alert statistics
    pub fn get_alert_stats(&self) -> AlertStats {
        let total_rules = self.rules.len();
        let enabled_rules = self.rules.iter().filter(|r| r.enabled).count();
        let active_alerts = self.active_alerts.len();
        let critical_alerts = self.active_alerts.values()
            .filter(|a| matches!(a.severity, AlertSeverity::Critical | AlertSeverity::Emergency))
            .count();
        
        AlertStats {
            total_rules,
            enabled_rules,
            active_alerts,
            critical_alerts,
            notification_channels: self.notifications.len(),
        }
    }
    
    /// Evaluates a specific alert condition (boxed to avoid recursion)
    fn evaluate_condition<'a>(&'a self, condition: &'a AlertCondition, metrics: &'a MetricsSnapshot) -> BoxFuture<'a, bool> {
        Box::pin(async move {
            match condition {
                AlertCondition::ThroughputBelow(threshold) => {
                    metrics.throughput < *threshold as f64
                }
                AlertCondition::LatencyAbove(threshold) => {
                    Duration::from_nanos(metrics.latency_percentiles.p99 as u64) > *threshold
                }
                AlertCondition::ErrorRateAbove(threshold) => {
                    let total_errors: u64 = metrics.error_counts.values().sum();
                    let error_rate = if total_errors > 0 {
                        // Simplified error rate calculation
                        (total_errors as f64 / (total_errors + 1000) as f64) * 100.0
                    } else {
                        0.0
                    };
                    error_rate > *threshold
                }
                AlertCondition::MemoryUsageAbove(threshold) => {
                    metrics.resource_usage.memory_bytes > *threshold
                }
                AlertCondition::CpuUsageAbove(threshold) => {
                    metrics.resource_usage.cpu_percent > *threshold
                }
                AlertCondition::CustomMetric { metric, threshold, operator } => {
                    if let Some(value) = metrics.custom_metrics.get(metric) {
                        match operator {
                            ComparisonOperator::GreaterThan => *value > *threshold,
                            ComparisonOperator::LessThan => *value < *threshold,
                            ComparisonOperator::Equal => (*value - *threshold).abs() < 0.001,
                            ComparisonOperator::NotEqual => (*value - *threshold).abs() >= 0.001,
                            ComparisonOperator::GreaterThanOrEqual => *value >= *threshold,
                            ComparisonOperator::LessThanOrEqual => *value <= *threshold,
                        }
                    } else {
                        false
                    }
                }
                AlertCondition::RateOfChange { .. } => {
                    // Would need historical data for rate calculation
                    false
                }
                AlertCondition::NoData { .. } => {
                    // Would need timestamp tracking
                    false
                }
            }
        })
    }
    
    /// Creates an alert from a rule and metrics
    fn create_alert_from_rule(&self, rule: &AlertRule, _metrics: &MetricsSnapshot) -> Alert {
        Alert {
            id: format!("{}_{}", rule.name, SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs()),
            name: rule.name.clone(),
            description: rule.description.clone(),
            metric_name: "multiple".to_string(), // Would extract from condition
            condition: DashboardAlertCondition::GreaterThan(0.0), // Simplified
            severity: rule.severity.clone(),
            state: AlertState::Active,
            triggered_at: Some(SystemTime::now()),
            resolved_at: None,
            message: Some(format!("Alert triggered for rule: {}", rule.name)),
        }
    }
    
    /// Sends notifications for an alert
    async fn send_notifications(&self, alert: &Alert, rule: &AlertRule) {
        let channels_to_notify = if rule.notification_channels.is_empty() {
            // Send to all channels if none specified
            self.notifications.keys().cloned().collect()
        } else {
            rule.notification_channels.clone()
        };
        
        for channel_name in channels_to_notify {
            if let Some(channel) = self.notifications.get(&channel_name) {
                if let Err(e) = channel.send_alert(alert).await {
                    error!("Failed to send notification via {}: {}", channel_name, e);
                } else {
                    debug!("Sent notification via {}", channel_name);
                }
            }
        }
    }
    
    /// Checks if an alert is in cooldown period
    fn is_in_cooldown(&self, rule_name: &str, cooldown: Duration) -> bool {
        if let Some(history) = self.alert_history.get(rule_name) {
            if let Some(last_trigger) = history.last() {
                let elapsed = SystemTime::now().duration_since(*last_trigger).unwrap_or_default();
                return elapsed < cooldown;
            }
        }
        false
    }
    
    /// Records an alert trigger time
    fn record_alert_trigger(&mut self, rule_name: &str) {
        let history = self.alert_history.entry(rule_name.to_string()).or_insert_with(Vec::new);
        history.push(SystemTime::now());
        
        // Keep only recent history (last 100 triggers)
        if history.len() > 100 {
            history.drain(0..history.len() - 100);
        }
    }
}

impl Default for AlertManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Alert statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertStats {
    /// Total number of alert rules
    pub total_rules: usize,
    /// Number of enabled rules
    pub enabled_rules: usize,
    /// Number of active alerts
    pub active_alerts: usize,
    /// Number of critical alerts
    pub critical_alerts: usize,
    /// Number of notification channels
    pub notification_channels: usize,
}

/// Creates default alert rules for common monitoring scenarios
pub fn create_default_alert_rules() -> Vec<AlertRule> {
    vec![
        AlertRule::new(
            "Low Throughput".to_string(),
            AlertCondition::ThroughputBelow(1000),
            AlertSeverity::Warning,
        )
        .with_description("Throughput has dropped below 1000 msgs/sec".to_string())
        .with_cooldown(Duration::from_secs(120)),
        
        AlertRule::new(
            "High Latency".to_string(),
            AlertCondition::LatencyAbove(Duration::from_micros(10000)), // 10ms
            AlertSeverity::Critical,
        )
        .with_description("P99 latency has exceeded 10ms".to_string())
        .with_consecutive_failures(3),
        
        AlertRule::new(
            "High Memory Usage".to_string(),
            AlertCondition::MemoryUsageAbove(8 * 1024 * 1024 * 1024), // 8GB
            AlertSeverity::Warning,
        )
        .with_description("Memory usage has exceeded 8GB".to_string()),
        
        AlertRule::new(
            "High CPU Usage".to_string(),
            AlertCondition::CpuUsageAbove(80.0),
            AlertSeverity::Warning,
        )
        .with_description("CPU usage has exceeded 80%".to_string())
        .with_consecutive_failures(2),
        
        AlertRule::new(
            "High Error Rate".to_string(),
            AlertCondition::ErrorRateAbove(5.0),
            AlertSeverity::Critical,
        )
        .with_description("Error rate has exceeded 5%".to_string()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{LatencyPercentiles, ResourceUsage};
    
    #[test]
    fn test_alert_rule_creation() {
        let rule = AlertRule::new(
            "Test Rule".to_string(),
            AlertCondition::ThroughputBelow(1000),
            AlertSeverity::Warning,
        )
        .with_description("Test description".to_string())
        .with_tag("team".to_string(), "platform".to_string());
        
        assert_eq!(rule.name, "Test Rule");
        assert_eq!(rule.severity, AlertSeverity::Warning);
        assert_eq!(rule.tags.get("team"), Some(&"platform".to_string()));
    }
    
    #[test]
    fn test_slack_notification_creation() {
        let slack = SlackNotification::new("https://hooks.slack.com/test".to_string())
            .with_channel("#alerts".to_string())
            .with_username("TestBot".to_string());
        
        assert_eq!(slack.name(), "slack");
        assert!(slack.is_enabled());
    }
    
    #[tokio::test]
    async fn test_alert_manager() {
        let mut manager = AlertManager::new();
        
        let rule = AlertRule::new(
            "Test Rule".to_string(),
            AlertCondition::ThroughputBelow(1000),
            AlertSeverity::Warning,
        );
        
        manager.add_rule(rule);
        
        let stats = manager.get_alert_stats();
        assert_eq!(stats.total_rules, 1);
        assert_eq!(stats.enabled_rules, 1);
    }
    
    #[tokio::test]
    async fn test_condition_evaluation() {
        let manager = AlertManager::new();
        
        let metrics = MetricsSnapshot {
            timestamp: chrono::Utc::now(),
            throughput: 500.0,
            latency_percentiles: LatencyPercentiles {
                p50: 1000.0,
                p90: 2000.0,
                p95: 3000.0,
                p99: 5000.0,
                p999: 10000.0,
            },
            error_counts: HashMap::new(),
            resource_usage: ResourceUsage {
                memory_bytes: 1024 * 1024 * 1024, // 1GB
                cpu_percent: 50.0,
                network_io_bps: 0,
                disk_io_bps: 0,
                open_fds: 100,
                thread_count: 10,
            },
            custom_metrics: HashMap::new(),
        };
        
        // Test throughput condition
        let condition = AlertCondition::ThroughputBelow(1000);
        assert!(manager.evaluate_condition(&condition, &metrics).await);
        
        // Test latency condition
        let condition = AlertCondition::LatencyAbove(Duration::from_micros(1000));
        assert!(manager.evaluate_condition(&condition, &metrics).await);
        
        // Test memory condition
        let condition = AlertCondition::MemoryUsageAbove(512 * 1024 * 1024); // 512MB
        assert!(manager.evaluate_condition(&condition, &metrics).await);
    }
}