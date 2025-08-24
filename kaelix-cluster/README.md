# Kaelix Cluster

## Overview

Kaelix Cluster provides advanced distributed systems infrastructure with sophisticated clustering capabilities.

## Test Utilities

Comprehensive test utilities for realistic distributed systems testing are available. See our [Test Utilities Documentation](/docs/test_utilities.md) for detailed information.

### Key Features

- Realistic network condition simulation
- Network partition management
- Advanced failure scenario testing
- Performance-aware testing infrastructure

## Getting Started

### Installation

```toml
[dependencies]
kaelix-cluster = { version = "0.1.0" }
```

### Basic Usage

```rust
use kaelix_cluster::test_utils::MockNetwork;

async fn example_test() {
    let network = MockNetwork::new();
    // Configure and use network simulation
}
```

## Documentation

- [Test Utilities Guide](/docs/test_utilities.md)

## License

Apache 2.0