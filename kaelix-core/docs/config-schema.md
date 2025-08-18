# MemoryStreamer Configuration Schema

This document describes the complete configuration schema for MemoryStreamer ultra-high-performance distributed streaming system.

## Overview

MemoryStreamer uses a layered configuration system that supports multiple sources and hot-reload capabilities. Configuration is designed for <10μs P99 latency and 10M+ messages/second throughput.

## Configuration Structure

### Network Configuration (`network`)

Controls transport layer settings and connection management.

```toml
[network]
bind_address = "0.0.0.0"        # IP address to bind to
port = 7878                     # Port number (1024-65535)
max_connections = 10000         # Maximum concurrent connections
connection_timeout = 5000       # Connection timeout in milliseconds

[network.keep_alive]
enabled = true                  # Enable TCP keep-alive
idle_time = 300                # Time before sending probes (seconds)
probe_interval = 60            # Interval between probes (seconds)
probe_count = 3                # Failed probes before dropping connection

[network.buffers]
receive_buffer = 262144        # Socket receive buffer (bytes)
send_buffer = 262144           # Socket send buffer (bytes)
read_buffer = 32768            # Application read buffer (bytes)
write_buffer = 32768           # Application write buffer (bytes)
```

### Protocol Configuration (`protocol`)

Controls message framing and wire protocol behavior.

```toml
[protocol]
max_payload_size = 1048576     # Maximum message payload (bytes)
frame_timeout = 1000           # Frame processing timeout (milliseconds)
enable_compression = false     # Enable payload compression

[protocol.compression]
algorithm = "Lz4"              # Compression algorithm: None, Lz4, Zstd, Snappy
level = 1                      # Compression level (1-9)
min_size = 1024               # Minimum size to trigger compression

[protocol.batching]
enabled = true                 # Enable frame batching
max_batch_size = 100          # Maximum frames per batch
max_batch_delay_us = 10       # Maximum batch delay (microseconds)
```

### Performance Configuration (`performance`)

Controls performance optimization settings.

```toml
[performance]
target_latency_p99_us = 10     # Target P99 latency (microseconds)
max_throughput = 10000000      # Maximum throughput (messages/second)
worker_threads = 8             # Worker threads (null = auto-detect)
numa_awareness = true          # Enable NUMA-aware allocation

[performance.cpu_affinity]
enabled = false                # Enable CPU affinity pinning
worker_cores = [0, 1, 2, 3]   # CPU cores for worker threads
io_cores = [4, 5]             # CPU cores for I/O threads

[performance.memory]
initial_pool_size = 67108864   # Initial memory pool (64MB)
max_pool_size = 1073741824     # Maximum memory pool (1GB)
use_huge_pages = false         # Enable huge pages
growth_factor = 1.5            # Pool growth factor
```

### Observability Configuration (`observability`)

Controls monitoring, metrics, and health checks.

```toml
[observability.tracing]
enabled = true                 # Enable tracing
level = "Info"                # Tracing level: Error, Warn, Info, Debug, Trace
json_output = false           # Enable JSON output format
distributed_tracing = false   # Enable distributed tracing

[observability.metrics]
enabled = true                # Enable metrics collection
collection_interval = 60     # Collection interval (seconds)
prometheus_enabled = true     # Enable Prometheus export
prometheus_port = 9090        # Prometheus metrics port

[observability.health_check]
enabled = true                # Enable health check endpoint
port = 8080                   # Health check port
path = "/health"              # Health check endpoint path
```

### Security Configuration (`security`)

Controls encryption, authentication, and access control.

```toml
[security.tls]
enabled = false               # Enable TLS encryption
cert_path = "/path/to/cert.pem"  # TLS certificate file
key_path = "/path/to/key.pem"    # TLS private key file
ca_path = "/path/to/ca.pem"      # CA certificate for client verification
require_client_cert = false     # Require client certificates

[security.authentication]
enabled = false               # Enable authentication
method = "None"               # Method: None, Jwt, ApiKey, MutualTls

[security.authentication.token_validation]
validation_key = "secret-key" # JWT signing key or API validation endpoint
expiration_tolerance = 300    # Token expiration tolerance (seconds)

[security.access_control]
enabled = false               # Enable access control
default_policy = "Allow"      # Default policy: Allow, Deny

[[security.access_control.rules]]
name = "admin-topics"         # Rule name
pattern = "admin.*"           # Pattern to match
action = "Deny"              # Action: Allow, Deny
```

## Environment Variable Overrides

All configuration values can be overridden using environment variables with the `MEMORY_STREAMER_` prefix:

```bash
# Override network port
export MEMORY_STREAMER_NETWORK_PORT=9090

# Override performance settings
export MEMORY_STREAMER_PERFORMANCE_WORKER_THREADS=16
export MEMORY_STREAMER_PERFORMANCE_TARGET_LATENCY_P99_US=5

# Override security settings
export MEMORY_STREAMER_SECURITY_TLS_ENABLED=true
```

## Validation Rules

### Network Configuration
- `bind_address`: Must be a valid IP address
- `port`: Range 1024-65535 (privileged ports require special permissions)
- `max_connections`: Range 1-1,000,000
- `connection_timeout`: Range 100-30,000 milliseconds

### Protocol Configuration
- `max_payload_size`: Range 64 bytes - 96MB
- `frame_timeout`: Range 1-10,000 milliseconds
- `compression.level`: Range 1-9 (algorithm-specific)
- `compression.min_size`: Range 64-65536 bytes

### Performance Configuration
- `target_latency_p99_us`: Minimum 1 microsecond
- `max_throughput`: Range 1,000-100,000,000 messages/second
- `worker_threads`: Range 1-256 threads
- `memory.initial_pool_size`: Range 1MB-128GB
- `memory.max_pool_size`: Range 1MB-1TB
- `memory.growth_factor`: Range 1.0-5.0

## Hot-Reload Support

Configuration changes are automatically detected and applied without service restart:

- File system watching with configurable debounce delay
- Validation before applying changes
- Automatic rollback on validation failure
- Event notifications for configuration changes
- Reload statistics and monitoring

## Performance Considerations

- Configuration access is optimized for zero-allocation hot paths
- Validation occurs at load time, not access time
- Default values are tuned for ultra-low latency operation
- Hot-reload has minimal performance impact on running system

## Configuration Examples

### Development Setup
```toml
[network]
bind_address = "127.0.0.1"
port = 7878

[performance]
target_latency_p99_us = 100
worker_threads = 4

[observability.tracing]
level = "Debug"

[security]
# All security disabled for development
```

### Production Setup
```toml
[network]
bind_address = "0.0.0.0"
port = 7878
max_connections = 50000

[performance]
target_latency_p99_us = 10
numa_awareness = true

[security.tls]
enabled = true
cert_path = "/etc/ssl/memory-streamer.crt"
key_path = "/etc/ssl/memory-streamer.key"

[security.authentication]
enabled = true
method = "Jwt"
```

### High-Throughput Setup
```toml
[performance]
target_latency_p99_us = 5
max_throughput = 50000000
worker_threads = 32

[performance.cpu_affinity]
enabled = true
worker_cores = [0, 2, 4, 6, 8, 10, 12, 14]
io_cores = [1, 3, 5, 7]

[performance.memory]
use_huge_pages = true
initial_pool_size = 1073741824  # 1GB
max_pool_size = 17179869184     # 16GB

[protocol.batching]
max_batch_size = 1000
max_batch_delay_us = 5
```