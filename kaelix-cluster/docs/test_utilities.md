# Kaelix Cluster Test Utilities Guide

## Overview

The Kaelix Cluster Test Utilities provide a comprehensive framework for simulating and testing distributed systems with unprecedented realism and control. Designed for complex distributed system testing, these utilities enable developers to create sophisticated test scenarios that mirror real-world network conditions.

## Architecture

```
Test Utilities Architecture
├── MockNetwork - Simulated Network Layer
│   ├── Realistic network condition simulation
│   ├── Network partition management
│   └── Comprehensive message routing
├── NetworkConditions - Configurable Network Behaviors
│   ├── Latency simulation
│   ├── Packet loss modeling
│   ├── Bandwidth throttling
│   └── Jitter and reordering
└── NetworkPartition - Topology Simulation
    ├── Multi-group network segmentation
    ├── Communication blocking
    └── Dynamic partition management
```

## Key Components

### MockNetwork

`MockNetwork` is the core simulation infrastructure for distributed system testing. It provides a realistic, configurable network environment for testing distributed algorithms and system behaviors.

#### Features
- Async message delivery simulation
- Configurable network conditions
- Network partition management
- Comprehensive metrics tracking

#### Basic Usage

```rust
use kaelix_cluster::test_utils::mock_network::{MockNetwork, NetworkConditions};

let network = MockNetwork::new();

// Register test nodes
let node1 = network.register_node("node1", "127.0.0.1:8000".parse()?).await?;
let node2 = network.register_node("node2", "127.0.0.1:8001".parse()?).await?;

// Apply realistic network conditions
let conditions = NetworkConditions::builder()
    .latency(Duration::from_millis(10), Duration::from_millis(50))
    .packet_loss(0.01)  // 1% packet loss
    .bandwidth_limit(1_000_000)  // 1 Mbps
    .build();

network.apply_conditions(conditions).await;

// Send messages with simulated network behavior
network.send_message(node1, node2, vec![1, 2, 3]).await?;
```

### NetworkConditions

`NetworkConditions` allows fine-grained control over network simulation parameters.

#### Configuration Options
- **Latency**: Minimum and maximum message delivery times
- **Packet Loss**: Probability of message dropping
- **Bandwidth Limit**: Maximum transfer rate
- **Jitter**: Variability in message delivery times
- **Message Duplication**: Probability of sending duplicate messages
- **Reordering**: Probability of message order alteration

#### Preset Network Conditions

```rust
// Perfect network (minimal latency, no loss)
let perfect = NetworkConditions::perfect();

// LAN-like conditions
let lan = NetworkConditions::lan_preset();

// WAN-like conditions
let wan = NetworkConditions::wan_preset();

// Unreliable network simulation
let unreliable = NetworkConditions::unreliable_preset();
```

### NetworkPartition

`NetworkPartition` enables sophisticated network topology simulation, including split-brain scenarios.

#### Features
- Multi-group network segmentation
- Dynamic partition creation
- Communication blocking between groups

#### Usage Examples

```rust
// Two-way partition
let partition = NetworkPartition::new()
    .split(&nodes[0..2], &nodes[2..4]);

// Multi-way partition
let partition = NetworkPartition::new()
    .add_partition(&nodes[0..2])   // Group 1
    .add_partition(&nodes[2..4])   // Group 2
    .add_partition(&nodes[4..6]);  // Group 3

network.apply_partition(partition).await;
```

## Advanced Testing Scenarios

### Split-Brain Detection

```rust
let harness = ClusterTestHarness::builder()
    .node_count(5)
    .build()
    .await?;

harness.start_cluster().await?;

// Create network partition
let partition = NetworkPartition::new()
    .split(&harness.node_ids()[0..2], &harness.node_ids()[2..5]);

harness.apply_network_partition(partition).await?;

// Detect split-brain condition
harness.wait_for_split_brain_detection().await?;
```

### Failure Scenario Testing

```rust
let harness = ClusterTestHarness::builder()
    .node_count(3)
    .build()
    .await?;

harness.start_cluster().await?;

// Simulate node failures
harness.fail_nodes(&[harness.node_ids()[0]], "test failure".to_string()).await?;

// Verify cluster resilience
assert_eq!(harness.active_node_count().await, 2);
```

## Best Practices

1. **Deterministic Testing**: Use fixed seeds for random number generation
2. **Incremental Complexity**: Start with simple network conditions, then increase complexity
3. **Comprehensive Coverage**: Test various scenarios (failures, partitions, load)
4. **Performance Monitoring**: Track metrics during simulations

## Performance Considerations

- **Low Overhead**: Test utilities designed for minimal performance impact
- **Async Architecture**: Non-blocking message processing
- **Configurable Precision**: Adjust simulation granularity as needed

## Metrics and Monitoring

```rust
let metrics = network.metrics_snapshot();
println!("Messages Sent: {}", metrics.messages_sent);
println!("Messages Delivered: {}", metrics.messages_delivered);
println!("Network Latency: {} µs", metrics.avg_latency_us);
```

## Common Pitfalls and Solutions

- **Over-Simulation**: Don't make tests too complex
- **Unrealistic Conditions**: Base network conditions on real-world scenarios
- **Insufficient Error Handling**: Always prepare for unexpected network behaviors

## Contributing

Help improve our test utilities! Report issues, submit PRs, and contribute to making distributed systems testing more robust.

## License

Part of the Kaelix Distributed Systems Framework