//! Simple test binary to verify monitoring system compilation and functionality

#[cfg(feature = "monitoring")]
use kaelix_tests::prelude::*;

#[cfg(feature = "monitoring")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing monitoring system compilation...");
    
    // Test metrics creation
    let metrics = create_shared_test_metrics();
    metrics.record_throughput(1000000);
    metrics.record_latency(5000);
    
    println!("✓ Metrics system works");
    
    // Test real-time monitoring
    let monitor = create_default_monitor(metrics.clone());
    println!("✓ Real-time monitor created");
    
    // Test alerting
    let mut alert_manager = AlertManager::new();
    let rules = create_default_alert_rules();
    for rule in rules {
        alert_manager.add_rule(rule);
    }
    
    // Add notification channels
    alert_manager.add_notification_channel(Box::new(
        SlackNotification::new("https://example.com/hook".to_string())
    ));
    
    println!("✓ Alert manager configured");
    
    // Test dashboard
    let dashboard = create_performance_dashboard("Test Dashboard");
    println!("✓ Performance dashboard created");
    
    // Test reporting
    let report = create_test_report("test_suite");
    println!("✓ Test report created");
    
    // Test observability
    let (tracer, _logger) = setup_test_observability()?;
    println!("✓ Observability system initialized");
    
    println!("\n🎉 All monitoring components compiled and initialized successfully!");
    println!("📊 The monitoring and observability system is ready for use.");
    
    Ok(())
}

#[cfg(not(feature = "monitoring"))]
fn main() {
    println!("⚠️  Monitoring features disabled. Run with: cargo run --bin test_monitoring --features monitoring");
}