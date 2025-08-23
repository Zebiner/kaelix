# Kaelix MemoryStreamer Architecture

## Table of Contents
- [Overview](#overview)
- [Core Runtime System](#core-runtime-system)
- [Message System](#message-system)
- [Configuration Management](#configuration-management)
- [Multiplexing Infrastructure](#multiplexing-infrastructure)
- [Telemetry Framework](#telemetry-framework)
- [Plugin System](#plugin-system)
- [Distributed Components](#distributed-components)
- [Performance Characteristics](#performance-characteristics)

## Overview

Kaelix MemoryStreamer is an ultra-high-performance distributed streaming system designed to achieve sub-10μs P99 latency while supporting millions of concurrent streams. The architecture is built around a NUMA-aware async runtime with zero-copy message handling and comprehensive observability.

### Design Philosophy
- **Performance First**: Every component optimized for ultra-low latency
- **Zero-Copy Patterns**: Minimize memory allocations and copies
- **NUMA Awareness**: Optimize for modern multi-core architectures
- **Type Safety**: Leverage Rust's type system for correctness
- **Modular Design**: Clean separation of concerns with clear interfaces

## Core Runtime System

**Location**: `kaelix-core/src/runtime/`

The runtime system provides the foundation for all high-performance operations in Kaelix.

### High-Performance Async Runtime

#### TaskScheduler (`runtime/executor.rs`)
```rust
// Priority-based task scheduling with microsecond precision
pub struct TaskScheduler {
    pools: Vec<TaskPool>,
    metrics: Arc<RuntimeMetrics>,
    config: RuntimeConfig,
}
```

**Key Features**:
- 4-tier priority system (Critical, High, Normal, Background)
- Dynamic worker pool allocation based on CPU topology
- Load balancing across multiple pools
- **Performance Target**: <10μs task scheduling latency

**Implementation Highlights** (`executor.rs:45-89`):
- Priority-based pool selection algorithm
- Lock-free task queuing using atomic operations
- NUMA-aware thread placement

#### Worker Management (`runtime/worker.rs`)
```rust
pub struct Worker {
    id: WorkerId,
    pool_id: PoolId,
    metrics: Arc<WorkerMetrics>,
    affinity: Option<CpuSet>,
}
```

**Features**:
- CPU affinity management for optimal cache locality
- Individual worker metrics tracking
- Graceful shutdown with task completion
- Configurable stack sizes (default: 2MB)

#### Health Monitoring (`runtime/metrics.rs`)
```rust
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}
```

**Monitoring Capabilities**:
- Real-time health status tracking
- Automatic degradation detection (>10% error rate triggers degraded status)
- Performance metrics collection with atomic counters
- Resource utilization tracking

### Configuration (`runtime/mod.rs`)
```rust
pub struct RuntimeConfig {
    pub worker_threads: Option<usize>,
    pub task_queue_depth: usize,
    pub target_latency_us: u64,
    pub numa_aware: bool,
    pub priority_levels: u8,
}
```

## Message System

**Location**: `kaelix-core/src/message.rs`

The message system implements zero-copy patterns for maximum performance.

### Core Message Types

#### Message Structure
```rust
pub struct Message {
    pub id: MessageId,
    pub topic: Topic,
    pub payload: Bytes,           // Zero-copy payload
    pub timestamp: Timestamp,
    pub partition: Option<PartitionId>,
    pub offset: Option<Offset>,
    pub headers: Option<HashMap<String, String>>,
}
```

**Design Highlights**:
- **Zero-Copy Payload**: Uses `bytes::Bytes` for reference-counted buffers
- **UUID-based MessageId**: Globally unique message identification
- **Topic Validation**: Enforces naming conventions and length limits
- **Optional Metadata**: Partition assignment and offset tracking

#### Builder Pattern (`message.rs:291-370`)
```rust
let message = Message::builder()
    .topic("events.user.signup")
    .payload(Bytes::from("user_data"))
    .partition(PartitionId::new(1))
    .header("content-type", "application/json")
    .build()?;
```

### Performance Optimizations

#### CompactMessage (`message.rs:247-289`)
```rust
pub struct CompactMessage {
    pub id: MessageId,
    pub topic: Topic,
    pub payload: Bytes,
    pub timestamp: Timestamp,
    pub partition: Option<PartitionId>,
    pub offset: Option<Offset>,
    // No headers for maximum performance
}
```

**Benefits**:
- Header-free representation for high-performance scenarios
- Reduced memory footprint
- Faster serialization/deserialization

#### MessageBatch (`message.rs:372-476`)
```rust
pub struct MessageBatch {
    pub messages: Vec<Message>,
    pub batch_id: MessageId,
    pub created_at: Timestamp,
}
```

**Bulk Operations**:
- Efficient batch processing
- Size estimation for memory planning
- Splitting and merging capabilities
- Iterator support for stream processing

## Configuration Management

**Location**: `kaelix-core/src/config/`

### Multi-Source Configuration Loading

#### ConfigLoader (`config/loader.rs`)
```rust
pub struct ConfigLoader {
    search_paths: Vec<PathBuf>,
    env_prefix: String,
}
```

**Loading Strategy** (`loader.rs:60-95`):
1. Start with default configuration
2. Merge configuration file if found
3. Apply environment variables (KAELIX_ prefix)
4. Validate final configuration

**Example Usage**:
```rust
let config = ConfigLoader::new()
    .with_path("/etc/kaelix")
    .with_env_prefix("KAELIX")
    .load()?;
```

### Hot Reload System

#### HotReloadManager (`config/hot_reload.rs`)
```rust
pub struct HotReloadManager {
    current_config: Arc<RwLock<MemoryStreamerConfig>>,
    config_path: PathBuf,
    change_tx: broadcast::Sender<ConfigChange>,
    stats: Arc<Mutex<HotReloadStats>>,
}
```

**Features** (`hot_reload.rs:101-212`):
- File watching with debounce mechanism (1-second default)
- Thread-safe configuration updates using `Arc<RwLock<>>`
- Broadcast notifications for configuration changes
- Optional validation before applying changes
- Automatic backup creation

**Implementation Pattern**:
```rust
let manager = HotReloadManager::new("config.toml", initial_config, None).await?;
let mut config_changes = manager.subscribe();

// Listen for configuration changes
tokio::spawn(async move {
    while let Ok(change) = config_changes.recv().await {
        println!("Config updated: {:?}", change.timestamp);
    }
});
```

### Comprehensive Validation

#### ConfigValidator (`config/validator.rs`)
```rust
pub struct ConfigValidator {
    context: ValidationContext,
}
```

**Validation Categories** (`validator.rs:38-55`):
- Network configuration validation
- Performance configuration validation
- Security configuration validation
- Telemetry configuration validation
- Storage configuration validation
- Integration constraints validation

**System-Aware Validation** (`validator.rs:336-366`):
- CPU core count validation
- Memory availability checking
- Disk space estimation
- Network interface validation

## Multiplexing Infrastructure

**Location**: `kaelix-core/src/multiplexing/`

### Stream Routing and Management

#### StreamRouter (`multiplexing/router.rs`)
```rust
pub struct StreamRouter {
    routing_table: Arc<DashMap<TopicPattern, Vec<StreamId>>>,
    load_balancer: Arc<LoadBalancer>,
    metrics: Arc<RoutingMetrics>,
}
```

**Routing Capabilities**:
- Topic-based routing with pattern matching
- Load balancing across multiple streams
- Consistent hashing for partition assignment
- Dead letter queue support

#### StreamRegistry (`multiplexing/registry.rs`)
```rust
pub struct StreamRegistry {
    streams: Arc<DashMap<StreamId, StreamInfo>>,
    by_topic: Arc<DashMap<Topic, Vec<StreamId>>>,
    metrics: StreamMetrics,
}
```

**Stream Management**:
- Active stream tracking
- Topic-based stream lookup
- Health monitoring and cleanup
- Capacity management

### Backpressure Management

#### BackpressureManager (`multiplexing/backpressure.rs`)
```rust
pub struct BackpressureManager {
    thresholds: BackpressureThresholds,
    current_pressure: Arc<AtomicU64>,
    strategies: Vec<Box<dyn BackpressureStrategy>>,
}
```

**Backpressure Strategies**:
- Drop oldest messages
- Block new publishers
- Increase buffer sizes
- Shed low-priority traffic

## Telemetry Framework

**Location**: `kaelix-core/src/telemetry/`

### OpenTelemetry Integration

#### Metrics Collection (`telemetry/metrics.rs`)
```rust
pub struct MetricsCollector {
    registry: Arc<MetricsRegistry>,
    exporters: Vec<Box<dyn MetricsExporter>>,
    collection_interval: Duration,
}
```

**Metrics Types**:
- Counters for event tracking
- Gauges for current state
- Histograms for latency distribution
- Summaries for percentile calculations

#### Distributed Tracing (`telemetry/tracing.rs`)
```rust
pub struct TracingConfig {
    pub enabled: bool,
    pub service_name: String,
    pub jaeger: JaegerConfig,
    pub sampling_ratio: f64,
}
```

**Tracing Capabilities**:
- Jaeger/Zipkin export
- Span correlation across services
- Custom span attributes
- Performance impact <1% overhead

### Performance Monitoring (`telemetry/performance.rs`)

```rust
pub struct PerformanceMonitor {
    pub worker_threads: Option<usize>,
    pub buffer_size: usize,
    pub batch_size: u32,
    pub enable_cpu_profiling: bool,
    pub enable_memory_profiling: bool,
}
```

**Monitoring Features**:
- CPU utilization tracking
- Memory allocation profiling
- I/O operation metrics
- Cache hit/miss ratios
- NUMA topology awareness

## Plugin System

**Location**: `kaelix-core/src/plugin/`

### Extensible Architecture

#### Plugin Traits (`plugin/traits.rs`)
```rust
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn initialize(&mut self, context: &PluginContext) -> Result<()>;
    fn shutdown(&mut self) -> Result<()>;
}

pub trait StreamProcessor: Plugin {
    fn process_message(&mut self, message: &mut Message) -> Result<ProcessingAction>;
}
```

#### Plugin Registry (`plugin/registry.rs`)
```rust
pub struct PluginRegistry {
    plugins: HashMap<String, Box<dyn Plugin>>,
    dependencies: DependencyGraph,
    lifecycle: PluginLifecycle,
}
```

**Lifecycle Management**:
- Dependency resolution
- Graceful initialization/shutdown
- Error isolation
- Hot plugin reloading

### Security Isolation (`plugin/security.rs`)

```rust
pub struct SecurityIsolation {
    sandbox: PluginSandbox,
    permissions: PermissionSet,
    resource_limits: ResourceLimits,
}
```

**Security Features**:
- Sandboxed execution environment
- Resource usage limits
- Permission-based access control
- API surface restriction

## Distributed Components

### Message Broker (`kaelix-broker/src/broker.rs`)

```rust
pub struct MessageBroker {
    state: Arc<RwLock<BrokerState>>,
    subscriptions: Arc<DashMap<Topic, Vec<broadcast::Sender<Message>>>>,
    message_counter: AtomicU64,
}
```

**Pub/Sub Capabilities**:
- Topic-based message routing
- Subscription management with cleanup
- State tracking and metrics
- Graceful shutdown handling

**Key Methods** (`broker.rs:88-130`):
- `publish()`: Deliver message to all subscribers
- `subscribe()`: Create new subscription for topic
- `stats()`: Get broker statistics
- Thread-safe operation with atomic counters

### Cluster Management (`kaelix-cluster/`)

#### Membership Management (`cluster/src/membership/mod.rs`)
```rust
pub struct MembershipManager {
    local_node: Box<NodeInfo>,
    swim_state: SwimState,
    event_tx: broadcast::Sender<MembershipEvent>,
}
```

**SWIM Protocol Implementation**:
- Gossip-based membership
- Failure detection with phi accrual
- Event broadcasting for membership changes
- Network partition resilience

#### Consensus Foundation (`cluster/src/consensus/mod.rs`)
```rust
pub struct ConsensusModule {
    state: ConsensusState,
    log: ReplicatedLog,
    peers: Vec<PeerId>,
}
```

**Raft Consensus Preparation**:
- Basic consensus state machine
- Log replication infrastructure
- Leader election foundation
- Snapshot management support

## Performance Characteristics

### Latency Targets
- **Task Scheduling**: <10μs P99
- **Message Creation**: <1μs average
- **Configuration Hot Reload**: <1ms
- **Plugin Invocation**: <100ns overhead

### Throughput Capabilities
- **Message Processing**: 10M+ messages/second
- **Concurrent Streams**: 1M+ active streams
- **Configuration Updates**: 1000+ updates/second
- **Plugin Calls**: 100M+ calls/second

### Memory Efficiency
- **Per-Stream Overhead**: <1KB for inactive streams
- **Message Batching**: Linear scaling with batch size
- **Configuration Storage**: <10KB per configuration instance
- **Plugin Isolation**: <100KB per plugin sandbox

### NUMA Optimizations
- **Thread Placement**: CPU affinity management
- **Memory Allocation**: Local node allocation
- **Cache Optimization**: Data structure locality
- **Cross-Node Communication**: Minimized overhead

## Design Patterns and Best Practices

### Zero-Copy Patterns
- `bytes::Bytes` for payload management
- Reference counting for shared data
- Memory mapping for large files
- Splice operations for kernel-level copies

### Lock-Free Programming
- Atomic operations for counters
- Compare-and-swap for state updates
- Memory ordering guarantees
- Wait-free data structures where possible

### Error Handling
- Comprehensive error taxonomy
- Context preservation in error chains
- Graceful degradation strategies
- Recovery mechanisms

### Resource Management
- RAII patterns for automatic cleanup
- Pool-based allocation strategies
- Background garbage collection
- Resource limit enforcement

This architecture provides the foundation for ultra-high-performance distributed streaming while maintaining type safety, modularity, and observability throughout the system.