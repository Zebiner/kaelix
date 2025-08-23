
use std::time::Duration;
use telemetry::config::*;

fn main() {
    let config = TelemetryConfig::default();
    let result = config.validate_config();
    println!("Config validation: {:?}", result);
}
