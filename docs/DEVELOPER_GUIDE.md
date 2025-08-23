# Kaelix MemoryStreamer Developer Guide

## Table of Contents
- [Getting Started](#getting-started)
- [Building the Project](#building-the-project)
- [Core APIs](#core-apis)
- [Configuration Guide](#configuration-guide)
- [Plugin Development](#plugin-development)
- [Testing](#testing)
- [Performance Optimization](#performance-optimization)
- [Troubleshooting](#troubleshooting)
- [Contributing](#contributing)

## Getting Started

### Prerequisites
- Rust 1.70.0 or later
- Cargo (comes with Rust)
- Git

### Quick Start

```bash
# Clone the repository
git clone <repository-url>
cd kaelix

# Build the workspace
cargo build --workspace

# Run tests
cargo test --workspace

# Build with optimizations
cargo build --release --workspace
```

### Project Structure
```
kaelix/
├── kaelix-core/          # High-performance streaming foundation
├── kaelix-broker/        # Message broker with pub/sub
├── kaelix-cluster/       # Distributed coordination
├── kaelix-consumer/      # Streaming consumer client
├── kaelix-publisher/     # Streaming publisher client
├── docs/                 # Documentation
└── examples/             # Example applications
```

## Building the Project

### Development Build
```bash
# Build all crates
cargo build --workspace

# Build specific crate
cargo build -p kaelix-core

# Build with features
cargo build --features "experimental,hot-reload"
```

### Release Build
```bash
# Optimized build
cargo build --release --workspace

# Profile-guided optimization (if supported)
RUSTFLAGS="-C target-cpu=native" cargo build --release
```

### Feature Flags
- `experimental`: Enable experimental distributed features
- `hot-reload`: Enable configuration hot-reload capabilities
- `telemetry`: Enable full telemetry and metrics
- `cluster`: Enable clustering capabilities

## Core APIs

### Message System

#### Creating Messages
```rust
use kaelix_core::{Message, Topic};
use bytes::Bytes;

// Simple message creation
let topic = Topic::new("events.user.signup")?;
let payload = Bytes::from("user_data");
let message = Message::new(topic, payload)?;

// Using builder pattern
let message = Message::builder()
    .topic("events.user.signup")
    .payload(Bytes::from(r#"{"userId": "12345", "email": "user@example.com"}"#))
    .header("content-type", "application/json")
    .header("timestamp", "2024-01-15T10:30:00Z")
    .partition(PartitionId::new(1))
    .build()?;
```

#### Working with MessageBatch
```rust
use kaelix_core::MessageBatch;

// Create batch from messages
let messages = vec![message1, message2, message3];
let batch = MessageBatch::new(messages);

// Split large batch
let smaller_batches = batch.split(100); // 100 messages per batch

// Merge multiple batches
let merged = MessageBatch::merge(vec![batch1, batch2, batch3]);

// Process batch
for message in batch {
    // Process each message
    println!("Processing message: {}", message.id);
}
```

### Message Broker

#### Setting up Pub/Sub
```rust
use kaelix_broker::MessageBroker;
use kaelix_core::{Message, Topic};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create and start broker
    let broker = MessageBroker::new();
    broker.start().await?;

    // Create topic
    let topic = Topic::new("events.notifications")?;

    // Subscribe to topic
    let mut receiver = broker.subscribe(&topic).await?;

    // Publish messages
    let message = Message::new(topic.clone(), Bytes::from("Hello, World!"))?;
    broker.publish(&topic, message).await?;

    // Receive messages
    tokio::spawn(async move {
        while let Ok(message) = receiver.recv().await {
            println!("Received: {:?}", message);
        }
    });

    // Get broker statistics
    let stats = broker.stats();
    println!("Broker stats: {}", stats);

    Ok(())
}
```

### Runtime System

#### Configuring the Runtime
```rust
use kaelix_core::runtime::{OptimizedRuntime, RuntimeConfig};

// Default configuration
let runtime = OptimizedRuntime::new(RuntimeConfig::default())?;

// Custom configuration
let config = RuntimeConfig {
    worker_threads: Some(8),
    task_queue_depth: 8192,
    target_latency_us: 5, // 5μs target
    numa_aware: true,
    priority_levels: 4,
    ..Default::default()
};

let runtime = OptimizedRuntime::new(config)?;

// Start runtime
runtime.start().await?;

// Submit high-priority task
runtime.spawn_with_priority(TaskPriority::High, async {
    // Your high-priority work here
    println!("High priority task executed");
}).await?;
```

#### Health Monitoring
```rust
use kaelix_core::runtime::HealthMonitor;

let health_monitor = HealthMonitor::new();

// Check health status
match health_monitor.status() {
    HealthStatus::Healthy => println!("System is healthy"),
    HealthStatus::Degraded => println!("System is degraded"),
    HealthStatus::Unhealthy => println!("System is unhealthy"),
    HealthStatus::Unknown => println!("Health status unknown"),
}

// Get detailed metrics
let metrics = health_monitor.metrics();
println!("Error rate: {:.2}%", metrics.error_rate() * 100.0);
```

## Configuration Guide

### Configuration Sources

Kaelix loads configuration from multiple sources in order of precedence:
1. Command-line arguments (highest priority)
2. Environment variables
3. Configuration files
4. Default values (lowest priority)

### Configuration Files

#### Basic TOML Configuration
```toml
# kaelix.toml
[network]
host = "0.0.0.0"
port = 8080
max_connections = 10000

[runtime]
worker_threads = 8
task_queue_depth = 4096
target_latency_us = 10

[telemetry]
enabled = true

[telemetry.metrics]
enabled = true

[telemetry.metrics.prometheus]
enabled = true
port = 9090

[security]
[security.tls]
enabled = false
cert_path = ""
key_path = ""
```

### Environment Variables

Use the `KAELIX_` prefix for environment variables:

```bash
# Network configuration
export KAELIX_NETWORK_HOST="localhost"
export KAELIX_NETWORK_PORT="9090"

# Runtime configuration
export KAELIX_RUNTIME_WORKER_THREADS="16"
export KAELIX_RUNTIME_NUMA_AWARE="true"

# Telemetry configuration
export KAELIX_TELEMETRY_ENABLED="true"
export KAELIX_TELEMETRY_METRICS_ENABLED="true"
```

### Hot Reload Configuration

```rust
use kaelix_core::config::{HotReloadManager, MemoryStreamerConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load initial configuration
    let initial_config = MemoryStreamerConfig::default();
    
    // Create hot reload manager
    let hot_reload = HotReloadManager::new(
        "kaelix.toml",
        initial_config,
        None, // Use default settings
    ).await?;

    // Subscribe to configuration changes
    let mut config_changes = hot_reload.subscribe();

    // Handle configuration updates
    tokio::spawn(async move {
        while let Ok(change) = config_changes.recv().await {
            println!("Configuration updated at: {}", change.timestamp);
            println!("New config: {:?}", change.new_config);
            
            // Apply configuration changes to your components
            apply_config_changes(&change.new_config).await;
        }
    });

    // Force reload configuration
    hot_reload.force_reload().await?;

    // Get current configuration
    let current_config = hot_reload.current_config().await;
    
    Ok(())
}

async fn apply_config_changes(config: &MemoryStreamerConfig) {
    // Update runtime configuration
    // Update network settings
    // Reconfigure telemetry
}
```

### Configuration Validation

```rust
use kaelix_core::config::{ConfigValidator, MemoryStreamerConfig};

// Validate configuration
let config = MemoryStreamerConfig::default();
ConfigValidator::validate(&config)?;

// Environment compatibility check
let validator = ConfigValidator::new();
let warnings = validator.validate_environment_compatibility(&config)?;

for warning in warnings {
    println!("Warning: {}", warning);
}
```

## Plugin Development

### Creating a Plugin

#### Implement the Plugin Trait
```rust
use kaelix_core::plugin::{Plugin, PluginContext, StreamProcessor, ProcessingAction};
use kaelix_core::{Message, Result};

pub struct LoggingPlugin {
    name: String,
    log_level: String,
}

impl Plugin for LoggingPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn initialize(&mut self, context: &PluginContext) -> Result<()> {
        println!("Initializing logging plugin");
        self.log_level = context.get_config("log_level")
            .unwrap_or_else(|| "info".to_string());
        Ok(())
    }

    fn shutdown(&mut self) -> Result<()> {
        println!("Shutting down logging plugin");
        Ok(())
    }
}

impl StreamProcessor for LoggingPlugin {
    fn process_message(&mut self, message: &mut Message) -> Result<ProcessingAction> {
        println!("Processing message: {} on topic: {}", 
                 message.id, message.topic);
        
        // Add processing timestamp header
        message.set_header(
            "processed_at".to_string(),
            chrono::Utc::now().to_rfc3339()
        );
        
        Ok(ProcessingAction::Continue)
    }
}
```

#### Register and Use Plugin
```rust
use kaelix_core::plugin::PluginRegistry;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut registry = PluginRegistry::new();
    
    // Register plugin
    let logging_plugin = Box::new(LoggingPlugin {
        name: "logging".to_string(),
        log_level: "info".to_string(),
    });
    
    registry.register("logging", logging_plugin)?;
    
    // Initialize all plugins
    registry.initialize_all().await?;
    
    // Use plugin for message processing
    let mut message = Message::new(
        Topic::new("events.test")?,
        Bytes::from("test data")
    )?;
    
    registry.process_message("logging", &mut message)?;
    
    Ok(())
}
```

### Plugin Security

```rust
use kaelix_core::plugin::{SecurityIsolation, ResourceLimits, PermissionSet};

// Configure plugin security
let security = SecurityIsolation::new()
    .with_resource_limits(ResourceLimits {
        max_memory_mb: 100,
        max_cpu_percent: 10,
        max_file_descriptors: 50,
    })
    .with_permissions(PermissionSet {
        can_read_config: true,
        can_write_metrics: true,
        can_access_network: false,
    });

// Apply security to plugin
registry.set_security("logging", security)?;
```

## Testing

### Unit Tests
```bash
# Run all tests
cargo test --workspace

# Run tests for specific crate
cargo test -p kaelix-core

# Run tests with output
cargo test --workspace -- --nocapture

# Run specific test
cargo test test_message_creation
```

### Integration Tests
```bash
# Run integration tests
cargo test --test integration_tests

# Run with specific features
cargo test --features "experimental" --test cluster_tests
```

### Benchmark Tests
```bash
# Run benchmarks
cargo bench

# Run specific benchmark
cargo bench --bench message_processing
```

### Example Test
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use kaelix_core::{Message, Topic};
    use bytes::Bytes;

    #[tokio::test]
    async fn test_message_broker_pub_sub() {
        let broker = MessageBroker::new();
        broker.start().await.unwrap();

        let topic = Topic::new("test.topic").unwrap();
        let mut receiver = broker.subscribe(&topic).await.unwrap();

        let message = Message::new(
            topic.clone(),
            Bytes::from("test payload")
        ).unwrap();

        broker.publish(&topic, message.clone()).await.unwrap();

        let received = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            receiver.recv()
        ).await.unwrap().unwrap();

        assert_eq!(received.payload, message.payload);
    }
}
```

## Performance Optimization

### CPU Optimization
```rust
// Enable NUMA awareness
let config = RuntimeConfig {
    numa_aware: true,
    worker_threads: Some(num_cpus::get()),
    ..Default::default()
};

// Set CPU affinity for workers
runtime.set_worker_affinity(0, CpuSet::new(&[0, 1]))?;
```

### Memory Optimization
```rust
// Use object pools for frequent allocations
let message_pool = MessagePool::new(1000);

// Reuse message instances
let mut message = message_pool.acquire();
message.reset();
message.set_topic(topic);
message.set_payload(payload);

// Return to pool when done
message_pool.release(message);
```

### Zero-Copy Patterns
```rust
// Use Bytes for zero-copy payload handling
let payload = Bytes::from_static(b"static data");

// Memory-mapped files for large payloads
let mmap = MemoryMap::open("large_file.dat")?;
let payload = Bytes::from(mmap.as_slice());

// Shared payload across multiple messages
let shared_payload = Arc::new(Bytes::from("shared data"));
```

### Profiling

#### CPU Profiling
```bash
# Install perf tools
sudo apt-get install linux-tools-generic

# Profile application
sudo perf record --call-graph=dwarf target/release/kaelix-app
sudo perf report
```

#### Memory Profiling
```bash
# Using heaptrack
heaptrack target/release/kaelix-app
heaptrack_gui heaptrack.kaelix-app.*.gz
```

#### Flamegraphs
```bash
# Install flamegraph
cargo install flamegraph

# Generate flamegraph
cargo flamegraph --bin kaelix-app
```

## Troubleshooting

### Common Issues

#### Compilation Errors
```bash
# Clear cargo cache
cargo clean

# Update dependencies
cargo update

# Check for missing features
cargo check --all-features
```

#### Runtime Errors
```rust
// Enable debug logging
env_logger::init();

// Check system resources
let metrics = runtime.metrics();
if metrics.memory_usage() > 0.8 {
    println!("High memory usage detected");
}
```

#### Performance Issues
```rust
// Enable performance monitoring
let config = TelemetryConfig {
    performance: PerformanceConfig {
        enable_cpu_profiling: true,
        enable_memory_profiling: true,
        ..Default::default()
    },
    ..Default::default()
};
```

### Debug Configuration
```toml
[telemetry.logging]
level = "debug"
target = "console"

[runtime]
target_latency_us = 10000  # Increase for debugging

[security]
[security.tls]
enabled = false  # Disable for local testing
```

### Common Error Messages

| Error | Cause | Solution |
|-------|-------|----------|
| `Topic name contains invalid characters` | Invalid topic format | Use alphanumeric, hyphens, underscores, dots only |
| `Broker is not running` | Broker not started | Call `broker.start().await?` before publishing |
| `Configuration validation failed` | Invalid config values | Check configuration against schema |
| `Worker pool exhausted` | Too many concurrent tasks | Increase worker threads or queue depth |

## Contributing

### Code Style
- Follow Rust standard formatting (`cargo fmt`)
- No clippy warnings (`cargo clippy -- -D warnings`)
- Write comprehensive tests
- Document public APIs

### Pull Request Process
1. Fork the repository
2. Create feature branch
3. Write tests for new functionality
4. Ensure all tests pass
5. Run benchmarks for performance-critical changes
6. Submit pull request

### Development Workflow
```bash
# Pre-commit checks
cargo fmt
cargo clippy -- -D warnings
cargo test --workspace
cargo bench  # For performance changes

# Documentation updates
cargo doc --open
```

### Performance Requirements
- Maintain <10μs P99 latency target
- No performance regressions in benchmarks
- Memory usage should not increase significantly
- Zero-copy patterns preferred

This guide provides the foundation for developing with and extending the Kaelix MemoryStreamer system. For additional help, refer to the API documentation and architecture guide.