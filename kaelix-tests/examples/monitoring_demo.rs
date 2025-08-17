//! # Monitoring and Observability Demo
//!
//! This example demonstrates the comprehensive monitoring and observability system
//! for MemoryStreamer testing, including real-time metrics, alerting, and reporting.

use kaelix_tests::prelude::*;
use std::path::Path;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize observability infrastructure
    let (tracer, _logger) = setup_test_observability()?;
    
    // Create metrics collection system
    let metrics = create_shared_test_metrics();
    
    // Create real-time monitoring
    let mut monitor = create_default_monitor(metrics.clone());
    
    // Create alert manager with default rules
    let mut alert_manager = AlertManager::new();
    for rule in create_default_alert_rules() {
        alert_manager.add_rule(rule);
    }
    
    // Add notification channels
    alert_manager.add_notification_channel(Box::new(
        SlackNotification::new("https://hooks.slack.com/example".to_string())
            .with_channel("#test-alerts".to_string())
    ));
    
    // Create performance dashboard
    let mut dashboard = create_performance_dashboard("MemoryStreamer Performance");
    
    // Create test report
    let mut report = create_test_report("monitoring_demo");
    
    // Start trace
    let trace_id = tracer.start_trace("monitoring_demo", Some("comprehensive_test"));
    println!("Started trace: {}", trace_id);
    
    // Start monitoring
    println!("Starting real-time monitoring...");
    // monitor.start().await?;
    
    // Start alert manager
    // alert_manager.start().await?;
    
    // Simulate test execution with metrics collection
    println!("Running simulated performance tests...");
    
    for i in 0..60 {
        // Simulate test span
        let span = tracer.start_span(&format!("test_iteration_{}", i));
        span.set_attribute("iteration", &i.to_string());
        span.set_attribute("test_type", "performance");
        
        // Record varying performance metrics
        let throughput = 1_000_000 + (i * 100_000) % 5_000_000;
        let latency_ns = 5000 + (i * 1000) % 20000;
        let memory_mb = 1000 + (i * 50) % 2000;
        let cpu_percent = 30.0 + (i as f64 * 2.0) % 70.0;
        
        metrics.record_throughput(throughput);
        metrics.record_latency(latency_ns);
        metrics.record_memory_usage((memory_mb * 1024 * 1024) as u64);
        metrics.record_custom_metric("cpu_usage_percent", cpu_percent);
        
        // Simulate occasional errors
        if i % 15 == 0 {
            metrics.record_error("timeout");
            span.error("Simulated timeout error");
        } else {
            span.success();
        }
        
        // Get current metrics snapshot
        let snapshot = metrics.get_snapshot();
        
        // Add to dashboard
        dashboard.add_metrics(snapshot.clone());
        
        // Check alerts
        let triggered_alerts = alert_manager.evaluate_metrics(&snapshot).await;
        if !triggered_alerts.is_empty() {
            println!("Triggered {} alerts at iteration {}", triggered_alerts.len(), i);
            for alert in triggered_alerts {
                println!("  - {}: {}", alert.name, alert.message.unwrap_or_default());
            }
        }
        
        // Simulate test result
        let test_result = if i % 10 == 9 {
            TestResult::new(format!("performance_test_{}", i))
                .failed("Simulated failure for demo".to_string())
                .with_metrics(snapshot)
        } else {
            TestResult::new(format!("performance_test_{}", i))
                .passed()
                .with_metrics(snapshot)
        };
        
        report.add_result(test_result);
        
        // Wait before next iteration
        sleep(Duration::from_millis(500)).await;
        
        if i % 10 == 0 {
            println!("  Completed {} test iterations", i + 1);
        }
    }
    
    // Finalize test report
    report.finalize();
    
    // Generate comprehensive reports
    println!("\nGenerating comprehensive reports...");
    
    // Create output directory
    std::fs::create_dir_all("target/monitoring_demo")?;
    let output_dir = Path::new("target/monitoring_demo");
    
    // Generate HTML report
    let html_report = report.generate_html();
    std::fs::write(output_dir.join("test_report.html"), html_report)?;
    println!("  ✓ HTML report: target/monitoring_demo/test_report.html");
    
    // Generate JSON report
    let json_report = serde_json::to_string_pretty(&report.generate_json())?;
    std::fs::write(output_dir.join("test_report.json"), json_report)?;
    println!("  ✓ JSON report: target/monitoring_demo/test_report.json");
    
    // Generate JUnit XML
    let junit_xml = report.generate_junit_xml();
    std::fs::write(output_dir.join("junit.xml"), junit_xml)?;
    println!("  ✓ JUnit XML: target/monitoring_demo/junit.xml");
    
    // Generate Prometheus metrics
    let prometheus_metrics = report.export_prometheus();
    std::fs::write(output_dir.join("metrics.txt"), prometheus_metrics)?;
    println!("  ✓ Prometheus metrics: target/monitoring_demo/metrics.txt");
    
    // Generate performance dashboard
    let dashboard_html = dashboard.generate_html_dashboard();
    std::fs::write(output_dir.join("dashboard.html"), dashboard_html)?;
    println!("  ✓ Performance dashboard: target/monitoring_demo/dashboard.html");
    
    // Export dashboard data
    let dashboard_data = serde_json::to_string_pretty(&dashboard.export_charts_data())?;
    std::fs::write(output_dir.join("dashboard_data.json"), dashboard_data)?;
    println!("  ✓ Dashboard data: target/monitoring_demo/dashboard_data.json");
    
    // Export traces
    let traces = tracer.export_traces();
    if !traces.is_empty() {
        let traces_json = serde_json::to_string_pretty(&traces)?;
        std::fs::write(output_dir.join("traces.json"), traces_json)?;
        println!("  ✓ Distributed traces: target/monitoring_demo/traces.json");
    }
    
    // Create CI artifacts
    let artifacts = create_test_artifacts(&report, output_dir);
    println!("  ✓ Created {} CI/CD artifacts", artifacts.len());
    
    // Display final statistics
    let stats = report.get_stats();
    let dashboard_stats = dashboard.get_dashboard_stats();
    let alert_stats = alert_manager.get_alert_stats();
    
    println!("\n=== Final Test Statistics ===");
    println!("Tests executed: {}", stats.total);
    println!("Tests passed: {}", stats.passed);
    println!("Tests failed: {}", stats.failed);
    println!("Success rate: {:.1}%", stats.success_rate);
    println!("Total duration: {:.2}s", stats.total_duration.as_secs_f64());
    
    if let Some(perf) = &report.performance_summary {
        println!("\n=== Performance Summary ===");
        println!("Average throughput: {:.0} msg/s", perf.avg_throughput);
        println!("Peak throughput: {:.0} msg/s", perf.peak_throughput);
        println!("P99 latency: {:.2} ms", perf.p99_latency_ns as f64 / 1_000_000.0);
        println!("Error rate: {:.3}%", perf.error_rate_percent);
    }
    
    println!("\n=== Monitoring Statistics ===");
    println!("Total charts: {}", dashboard_stats.total_charts);
    println!("Total metrics collected: {}", dashboard_stats.total_metrics);
    println!("Active alerts: {}", alert_stats.active_alerts);
    println!("Critical alerts: {}", alert_stats.critical_alerts);
    
    // Stop monitoring
    println!("\nStopping monitoring systems...");
    // monitor.stop().await?;
    // alert_manager.stop().await?;
    
    println!("\n🎉 Monitoring and observability demo completed successfully!");
    println!("📊 Open target/monitoring_demo/dashboard.html to view the performance dashboard");
    println!("📋 Open target/monitoring_demo/test_report.html to view the detailed test report");
    
    Ok(())
}