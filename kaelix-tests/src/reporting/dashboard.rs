//! # Performance Dashboard
//!
//! Interactive dashboard for visualizing test metrics and performance data with real-time updates
//! and historical trend analysis.

use crate::metrics::MetricsSnapshot;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, SystemTime};
use tracing::debug;

/// Chart types supported by the dashboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChartType {
    /// Line chart for time series data
    Line,
    /// Bar chart for categorical data
    Bar,
    /// Histogram for distribution data
    Histogram,
    /// Heatmap for correlation data
    Heatmap,
    /// Gauge for single value displays
    Gauge,
    /// Area chart for cumulative data
    Area,
}

/// Data series for charts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSeries {
    /// Series name
    pub name: String,
    /// Data points (timestamp, value)
    pub data: Vec<(i64, f64)>,
    /// Series color
    pub color: Option<String>,
    /// Series unit
    pub unit: String,
    /// Series metadata
    pub metadata: HashMap<String, String>,
}

impl DataSeries {
    /// Creates a new data series
    pub fn new(name: String, unit: String) -> Self {
        Self {
            name,
            data: Vec::new(),
            color: None,
            unit,
            metadata: HashMap::new(),
        }
    }
    
    /// Adds a data point to the series
    pub fn add_point(&mut self, timestamp: SystemTime, value: f64) {
        let timestamp_ms = timestamp
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        self.data.push((timestamp_ms, value));
    }
    
    /// Sets the series color
    pub fn with_color(mut self, color: String) -> Self {
        self.color = Some(color);
        self
    }
    
    /// Adds metadata to the series
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
    
    /// Gets the latest value in the series
    pub fn latest_value(&self) -> Option<f64> {
        self.data.last().map(|(_, value)| *value)
    }
    
    /// Gets the average value over the last N points
    pub fn average_last_n(&self, n: usize) -> Option<f64> {
        if self.data.is_empty() {
            return None;
        }
        
        let start_idx = self.data.len().saturating_sub(n);
        let sum: f64 = self.data[start_idx..].iter().map(|(_, value)| value).sum();
        let count = self.data.len() - start_idx;
        
        if count > 0 {
            Some(sum / count as f64)
        } else {
            None
        }
    }
    
    /// Trims series data to maximum allowed points
    pub fn trim_data(&mut self, max_points: usize) {
        if self.data.len() > max_points {
            self.data.drain(0..self.data.len() - max_points);
        }
    }
}

/// Chart configuration and data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chart {
    /// Chart ID
    pub id: String,
    /// Chart title
    pub title: String,
    /// Chart type
    pub chart_type: ChartType,
    /// Data series in the chart
    pub data_series: Vec<DataSeries>,
    /// Chart configuration options
    pub options: ChartOptions,
    /// Chart position and size
    pub layout: ChartLayout,
}

/// Chart configuration options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartOptions {
    /// X-axis label
    pub x_axis_label: String,
    /// Y-axis label
    pub y_axis_label: String,
    /// Chart height in pixels
    pub height: u32,
    /// Chart width in pixels
    pub width: u32,
    /// Show legend
    pub show_legend: bool,
    /// Show grid
    pub show_grid: bool,
    /// Animate transitions
    pub animate: bool,
    /// Maximum number of data points to display
    pub max_data_points: usize,
    /// Y-axis minimum value
    pub y_min: Option<f64>,
    /// Y-axis maximum value
    pub y_max: Option<f64>,
}

impl Default for ChartOptions {
    fn default() -> Self {
        Self {
            x_axis_label: "Time".to_string(),
            y_axis_label: "Value".to_string(),
            height: 400,
            width: 800,
            show_legend: true,
            show_grid: true,
            animate: true,
            max_data_points: 1000,
            y_min: None,
            y_max: None,
        }
    }
}

/// Chart layout information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartLayout {
    /// Row position
    pub row: u32,
    /// Column position
    pub col: u32,
    /// Width in grid units
    pub width: u32,
    /// Height in grid units
    pub height: u32,
}

impl Default for ChartLayout {
    fn default() -> Self {
        Self {
            row: 0,
            col: 0,
            width: 1,
            height: 1,
        }
    }
}

/// Alert configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    /// Alert ID
    pub id: String,
    /// Alert name
    pub name: String,
    /// Alert description
    pub description: String,
    /// Metric name to monitor
    pub metric_name: String,
    /// Alert condition
    pub condition: AlertCondition,
    /// Alert severity
    pub severity: AlertSeverity,
    /// Alert state
    pub state: AlertState,
    /// When the alert was triggered
    pub triggered_at: Option<SystemTime>,
    /// When the alert was resolved
    pub resolved_at: Option<SystemTime>,
    /// Alert message
    pub message: Option<String>,
}

/// Alert condition types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertCondition {
    /// Value exceeds threshold
    GreaterThan(f64),
    /// Value is below threshold
    LessThan(f64),
    /// Value equals specific value
    Equal(f64),
    /// Value is between two thresholds
    Between(f64, f64),
    /// Rate of change exceeds threshold
    RateOfChange(f64),
    /// No data received for duration
    NoData(Duration),
}

/// Alert severity levels
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertSeverity {
    /// Informational alert
    Info,
    /// Warning alert
    Warning,
    /// Critical alert
    Critical,
    /// Emergency alert
    Emergency,
}

/// Alert state
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertState {
    /// Alert is active
    Active,
    /// Alert is resolved
    Resolved,
    /// Alert is silenced
    Silenced,
}

/// Performance dashboard for real-time monitoring
#[derive(Debug)]
pub struct PerformanceDashboard {
    /// Dashboard title
    title: String,
    /// Historical metrics data
    metrics_history: VecDeque<MetricsSnapshot>,
    /// Charts in the dashboard
    charts: Vec<Chart>,
    /// Active alerts
    alerts: Vec<Alert>,
    /// Dashboard configuration
    config: DashboardConfig,
    /// Real-time data series cache
    data_cache: HashMap<String, DataSeries>,
    /// Dashboard creation time
    created_at: SystemTime,
}

/// Dashboard configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardConfig {
    /// Maximum history size
    pub max_history_size: usize,
    /// Auto-refresh interval in seconds
    pub refresh_interval_secs: u64,
    /// Enable real-time updates
    pub real_time_enabled: bool,
    /// Dashboard theme
    pub theme: DashboardTheme,
    /// Grid layout configuration
    pub grid_config: GridConfig,
}

/// Dashboard theme options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DashboardTheme {
    Light,
    Dark,
    Auto,
}

/// Grid layout configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridConfig {
    /// Number of columns
    pub columns: u32,
    /// Column gap in pixels
    pub column_gap: u32,
    /// Row gap in pixels
    pub row_gap: u32,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            max_history_size: 1000,
            refresh_interval_secs: 5,
            real_time_enabled: true,
            theme: DashboardTheme::Light,
            grid_config: GridConfig {
                columns: 2,
                column_gap: 20,
                row_gap: 20,
            },
        }
    }
}

impl PerformanceDashboard {
    /// Creates a new performance dashboard
    pub fn new(title: String) -> Self {
        let mut dashboard = Self {
            title,
            metrics_history: VecDeque::new(),
            charts: Vec::new(),
            alerts: Vec::new(),
            config: DashboardConfig::default(),
            data_cache: HashMap::new(),
            created_at: SystemTime::now(),
        };
        
        // Add default charts
        dashboard.add_default_charts();
        
        dashboard
    }
    
    /// Adds metrics snapshot to the dashboard
    pub fn add_metrics(&mut self, snapshot: MetricsSnapshot) {
        debug!("Adding metrics snapshot to dashboard");
        
        // Add to history
        self.metrics_history.push_back(snapshot.clone());
        if self.metrics_history.len() > self.config.max_history_size {
            self.metrics_history.pop_front();
        }
        
        // Update data series
        self.update_data_series(&snapshot);
        
        // Check alerts
        self.check_alerts(&snapshot);
    }
    
    /// Generates HTML dashboard
    pub fn generate_html_dashboard(&self) -> String {
        let theme_class = match self.config.theme {
            DashboardTheme::Light => "theme-light",
            DashboardTheme::Dark => "theme-dark",
            DashboardTheme::Auto => "theme-auto",
        };
        
        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{}</title>
    <script src="https://cdn.plot.ly/plotly-latest.min.js"></script>
    <style>
        {}
    </style>
</head>
<body class="{}">
    <div class="dashboard-header">
        <h1>{}</h1>
        <div class="dashboard-stats">
            <div class="stat">
                <span class="stat-label">Total Metrics:</span>
                <span class="stat-value">{}</span>
            </div>
            <div class="stat">
                <span class="stat-label">Active Alerts:</span>
                <span class="stat-value alert-count">{}</span>
            </div>
            <div class="stat">
                <span class="stat-label">Last Update:</span>
                <span class="stat-value" id="last-update">Just now</span>
            </div>
        </div>
    </div>
    
    <div class="alerts-panel">
        {}
    </div>
    
    <div class="dashboard-grid">
        {}
    </div>
    
    <script>
        {}
    </script>
</body>
</html>"#,
            self.title,
            self.generate_dashboard_css(),
            theme_class,
            self.title,
            self.metrics_history.len(),
            self.alerts.iter().filter(|a| a.state == AlertState::Active).count(),
            self.generate_alerts_html(),
            self.generate_charts_html(),
            self.generate_dashboard_js()
        )
    }
    
    /// Checks alerts against current metrics
    pub fn check_alerts(&mut self, snapshot: &MetricsSnapshot) -> Vec<Alert> {
        let mut triggered_alerts = Vec::new();
        
        for alert in &mut self.alerts {
            if alert.state != AlertState::Active {
                continue;
            }
            
            let metric_value = match alert.metric_name.as_str() {
                "throughput" => Some(snapshot.throughput),
                "latency_p99" => Some(snapshot.latency_percentiles.p99),
                "memory_usage" => Some(snapshot.resource_usage.memory_bytes as f64),
                "cpu_usage" => Some(snapshot.resource_usage.cpu_percent),
                _ => snapshot.custom_metrics.get(&alert.metric_name).copied(),
            };
            
            if let Some(value) = metric_value {
                let should_trigger = match &alert.condition {
                    AlertCondition::GreaterThan(threshold) => value > *threshold,
                    AlertCondition::LessThan(threshold) => value < *threshold,
                    AlertCondition::Equal(target) => (value - target).abs() < 0.001,
                    AlertCondition::Between(min, max) => value >= *min && value <= *max,
                    AlertCondition::RateOfChange(_threshold) => {
                        // Would need historical data for rate calculation
                        false
                    }
                    AlertCondition::NoData(_duration) => false, // Would need timestamp tracking
                };
                
                if should_trigger && alert.state != AlertState::Active {
                    alert.state = AlertState::Active;
                    alert.triggered_at = Some(SystemTime::now());
                    alert.message = Some(format!(
                        "Alert '{}' triggered: {} = {:.2}",
                        alert.name, alert.metric_name, value
                    ));
                    
                    triggered_alerts.push(alert.clone());
                }
            }
        }
        
        triggered_alerts
    }
    
    /// Exports chart data in JSON format
    pub fn export_charts_data(&self) -> serde_json::Value {
        serde_json::json!({
            "dashboard_title": self.title,
            "created_at": self.created_at
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            "config": self.config,
            "charts": self.charts,
            "alerts": self.alerts,
            "metrics_count": self.metrics_history.len()
        })
    }
    
    /// Gets dashboard statistics
    pub fn get_dashboard_stats(&self) -> DashboardStats {
        let active_alerts = self.alerts.iter().filter(|a| a.state == AlertState::Active).count();
        let critical_alerts = self.alerts.iter()
            .filter(|a| a.state == AlertState::Active && a.severity == AlertSeverity::Critical)
            .count();
        
        DashboardStats {
            total_charts: self.charts.len(),
            total_metrics: self.metrics_history.len(),
            active_alerts,
            critical_alerts,
            uptime_seconds: self.created_at.elapsed().unwrap_or_default().as_secs(),
            memory_usage_mb: std::mem::size_of_val(&self.metrics_history) / (1024 * 1024),
        }
    }
    
    /// Adds default charts for common metrics
    fn add_default_charts(&mut self) {
        // Throughput chart
        let throughput_chart = Chart {
            id: "throughput".to_string(),
            title: "Throughput (msgs/sec)".to_string(),
            chart_type: ChartType::Line,
            data_series: vec![DataSeries::new("Throughput".to_string(), "msgs/sec".to_string())
                .with_color("#007bff".to_string())],
            options: ChartOptions {
                y_axis_label: "Messages per Second".to_string(),
                ..Default::default()
            },
            layout: ChartLayout {
                row: 0,
                col: 0,
                width: 1,
                height: 1,
            },
        };
        
        // Latency chart
        let latency_chart = Chart {
            id: "latency".to_string(),
            title: "Latency (P99)".to_string(),
            chart_type: ChartType::Line,
            data_series: vec![DataSeries::new("P99 Latency".to_string(), "ns".to_string())
                .with_color("#dc3545".to_string())],
            options: ChartOptions {
                y_axis_label: "Latency (nanoseconds)".to_string(),
                ..Default::default()
            },
            layout: ChartLayout {
                row: 0,
                col: 1,
                width: 1,
                height: 1,
            },
        };
        
        // Resource usage chart
        let resource_chart = Chart {
            id: "resources".to_string(),
            title: "Resource Usage".to_string(),
            chart_type: ChartType::Area,
            data_series: vec![
                DataSeries::new("Memory".to_string(), "MB".to_string())
                    .with_color("#28a745".to_string()),
                DataSeries::new("CPU".to_string(), "%".to_string())
                    .with_color("#ffc107".to_string()),
            ],
            options: ChartOptions {
                y_axis_label: "Usage".to_string(),
                ..Default::default()
            },
            layout: ChartLayout {
                row: 1,
                col: 0,
                width: 2,
                height: 1,
            },
        };
        
        self.charts.push(throughput_chart);
        self.charts.push(latency_chart);
        self.charts.push(resource_chart);
    }
    
    /// Updates data series with new metrics
    fn update_data_series(&mut self, snapshot: &MetricsSnapshot) {
        let timestamp = SystemTime::now();
        let max_points = 1000; // Max data points
        
        // Update throughput series
        if let Some(chart) = self.charts.iter_mut().find(|c| c.id == "throughput") {
            if let Some(series) = chart.data_series.get_mut(0) {
                series.add_point(timestamp, snapshot.throughput);
                series.trim_data(max_points);
            }
        }
        
        // Update latency series
        if let Some(chart) = self.charts.iter_mut().find(|c| c.id == "latency") {
            if let Some(series) = chart.data_series.get_mut(0) {
                series.add_point(timestamp, snapshot.latency_percentiles.p99);
                series.trim_data(max_points);
            }
        }
        
        // Update resource series
        if let Some(chart) = self.charts.iter_mut().find(|c| c.id == "resources") {
            if chart.data_series.len() >= 2 {
                // Memory usage (convert to MB)
                chart.data_series[0].add_point(
                    timestamp, 
                    snapshot.resource_usage.memory_bytes as f64 / (1024.0 * 1024.0)
                );
                chart.data_series[0].trim_data(max_points);
                
                // CPU usage
                chart.data_series[1].add_point(timestamp, snapshot.resource_usage.cpu_percent);
                chart.data_series[1].trim_data(max_points);
            }
        }
    }
    
    /// Generates CSS for the dashboard
    fn generate_dashboard_css(&self) -> String {
        r#"
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            margin: 0;
            padding: 20px;
            background-color: var(--bg-color);
            color: var(--text-color);
        }
        
        .theme-light {
            --bg-color: #ffffff;
            --text-color: #333333;
            --border-color: #dee2e6;
            --panel-bg: #f8f9fa;
        }
        
        .theme-dark {
            --bg-color: #1a1a1a;
            --text-color: #ffffff;
            --border-color: #444444;
            --panel-bg: #2d2d2d;
        }
        
        .dashboard-header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin-bottom: 30px;
            padding: 20px;
            background: var(--panel-bg);
            border-radius: 8px;
            border: 1px solid var(--border-color);
        }
        
        .dashboard-stats {
            display: flex;
            gap: 30px;
        }
        
        .stat {
            text-align: center;
        }
        
        .stat-label {
            display: block;
            font-size: 0.8em;
            color: #666;
            margin-bottom: 5px;
        }
        
        .stat-value {
            display: block;
            font-size: 1.5em;
            font-weight: bold;
        }
        
        .alert-count {
            color: #dc3545;
        }
        
        .alerts-panel {
            margin-bottom: 20px;
        }
        
        .alert {
            padding: 10px 15px;
            margin: 5px 0;
            border-radius: 4px;
            border-left: 4px solid;
        }
        
        .alert.critical {
            background-color: #f8d7da;
            border-color: #dc3545;
        }
        
        .alert.warning {
            background-color: #fff3cd;
            border-color: #ffc107;
        }
        
        .dashboard-grid {
            display: grid;
            grid-template-columns: repeat(2, 1fr);
            gap: 20px;
        }
        
        .chart-container {
            background: var(--panel-bg);
            border: 1px solid var(--border-color);
            border-radius: 8px;
            padding: 20px;
        }
        
        .chart-title {
            margin: 0 0 15px 0;
            font-size: 1.1em;
            font-weight: 600;
        }
        "#.to_string()
    }
    
    /// Generates HTML for alerts
    fn generate_alerts_html(&self) -> String {
        let mut html = String::new();
        
        let active_alerts: Vec<_> = self.alerts.iter()
            .filter(|a| a.state == AlertState::Active)
            .collect();
        
        if !active_alerts.is_empty() {
            html.push_str("<h2>Active Alerts</h2>");
            
            for alert in active_alerts {
                let severity_class = match alert.severity {
                    AlertSeverity::Critical | AlertSeverity::Emergency => "critical",
                    AlertSeverity::Warning => "warning",
                    AlertSeverity::Info => "info",
                };
                
                html.push_str(&format!(
                    r#"<div class="alert {}">
                        <strong>{}</strong>: {}
                    </div>"#,
                    severity_class,
                    alert.name,
                    alert.message.as_deref().unwrap_or("Alert triggered")
                ));
            }
        }
        
        html
    }
    
    /// Generates HTML for charts
    fn generate_charts_html(&self) -> String {
        let mut html = String::new();
        
        for chart in &self.charts {
            html.push_str(&format!(
                r#"<div class="chart-container">
                    <h3 class="chart-title">{}</h3>
                    <div id="chart-{}" style="width:100%;height:{}px;"></div>
                </div>"#,
                chart.title,
                chart.id,
                chart.options.height
            ));
        }
        
        html
    }
    
    /// Generates JavaScript for dashboard interactivity
    fn generate_dashboard_js(&self) -> String {
        let mut js = String::new();
        
        // Chart rendering code
        for chart in &self.charts {
            js.push_str(&format!(
                r#"
                // Chart: {}
                var chartData_{} = {};
                var chartLayout_{} = {{
                    title: '{}',
                    xaxis: {{ title: '{}' }},
                    yaxis: {{ title: '{}' }},
                    showlegend: {}
                }};
                Plotly.newPlot('chart-{}', chartData_{}, chartLayout_{});
                "#,
                chart.title,
                chart.id,
                self.generate_chart_data_js(chart),
                chart.id,
                chart.title,
                chart.options.x_axis_label,
                chart.options.y_axis_label,
                chart.options.show_legend,
                chart.id,
                chart.id,
                chart.id
            ));
        }
        
        // Auto-refresh code
        js.push_str(&format!(
            r#"
            // Auto-refresh functionality
            setInterval(function() {{
                document.getElementById('last-update').textContent = 'Just now';
                // In a real implementation, this would fetch new data
            }}, {});
            "#,
            self.config.refresh_interval_secs * 1000
        ));
        
        js
    }
    
    /// Generates chart data in JavaScript format
    fn generate_chart_data_js(&self, chart: &Chart) -> String {
        let mut traces = Vec::new();
        
        for series in &chart.data_series {
            let x_values: Vec<String> = series.data.iter()
                .map(|(timestamp, _)| {
                    let dt = SystemTime::UNIX_EPOCH + Duration::from_millis(*timestamp as u64);
                    format!("{:?}", dt) // Simplified timestamp formatting
                })
                .collect();
            
            let y_values: Vec<f64> = series.data.iter().map(|(_, value)| *value).collect();
            
            let trace = format!(
                r#"{{
                    x: {:?},
                    y: {:?},
                    type: '{}',
                    name: '{}',
                    line: {{ color: '{}' }}
                }}"#,
                x_values,
                y_values,
                match chart.chart_type {
                    ChartType::Line => "scatter",
                    ChartType::Bar => "bar",
                    ChartType::Area => "scatter",
                    _ => "scatter",
                },
                series.name,
                series.color.as_deref().unwrap_or("#007bff")
            );
            
            traces.push(trace);
        }
        
        format!("[{}]", traces.join(","))
    }
}

/// Dashboard statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardStats {
    /// Total number of charts
    pub total_charts: usize,
    /// Total metrics collected
    pub total_metrics: usize,
    /// Number of active alerts
    pub active_alerts: usize,
    /// Number of critical alerts
    pub critical_alerts: usize,
    /// Dashboard uptime in seconds
    pub uptime_seconds: u64,
    /// Memory usage in MB
    pub memory_usage_mb: usize,
}

/// Creates a default performance dashboard
pub fn create_performance_dashboard(title: &str) -> PerformanceDashboard {
    PerformanceDashboard::new(title.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{LatencyPercentiles, MetricsSnapshot, ResourceUsage};
    
    #[test]
    fn test_dashboard_creation() {
        let dashboard = PerformanceDashboard::new("Test Dashboard".to_string());
        assert_eq!(dashboard.title, "Test Dashboard");
        assert_eq!(dashboard.charts.len(), 3); // Default charts
    }
    
    #[test]
    fn test_data_series() {
        let mut series = DataSeries::new("test".to_string(), "count".to_string());
        series.add_point(SystemTime::now(), 42.0);
        
        assert_eq!(series.data.len(), 1);
        assert_eq!(series.latest_value(), Some(42.0));
    }
    
    #[test]
    fn test_alert_creation() {
        let alert = Alert {
            id: "test_alert".to_string(),
            name: "Test Alert".to_string(),
            description: "Test description".to_string(),
            metric_name: "throughput".to_string(),
            condition: AlertCondition::LessThan(1000.0),
            severity: AlertSeverity::Warning,
            state: AlertState::Active,
            triggered_at: None,
            resolved_at: None,
            message: None,
        };
        
        assert_eq!(alert.severity, AlertSeverity::Warning);
        assert_eq!(alert.state, AlertState::Active);
    }
    
    #[test]
    fn test_dashboard_stats() {
        let dashboard = PerformanceDashboard::new("Test".to_string());
        let stats = dashboard.get_dashboard_stats();
        
        assert_eq!(stats.total_charts, 3);
        assert_eq!(stats.active_alerts, 0);
    }
}