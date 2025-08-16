# CLAUDE.md - MemoryStreamer Project Context

## Project Overview

MemoryStreamer is an ultra-high-performance distributed streaming system designed to achieve 10x better throughput and 100x lower latency than Apache Kafka. This is not an incremental improvement but a fundamental reimagining of streaming infrastructure using cutting-edge technologies.

### Core Performance Targets
- **Throughput**: 10+ million messages/second per node
- **Latency**: <10 microseconds P99 end-to-end  
- **Consistency**: Linearizable consistency via Raft consensus
- **Security**: <100ns authorization decisions with full ACL support
- **Availability**: 99.99% uptime with automatic failover in <500ms

### Technology Stack
- **Language**: Rust (latest stable)
- **Async Runtime**: Tokio with all features
- **Consensus**: Raft protocol with Multi-Raft for partitions
- **Serialization**: Protocol Buffers with zero-copy deserialization
- **Networking**: TCP with optional DPDK/eBPF acceleration
- **Security**: JWT, OAuth 2.0, mTLS, SASL with hierarchical ACLs

## Architecture Principles

### 1. Security First, Performance Always
Every component must have security built-in from the start, not added later. Security overhead must be <0.1% of throughput. Use caching aggressively for authorization decisions.

### 2. Zero-Copy Everywhere
Avoid unnecessary memory allocations and copies. Use `bytes::Bytes` for message payloads. Implement reference counting instead of cloning. Use memory-mapped files where appropriate.

### 3. Lock-Free When Possible
Prefer lock-free data structures from `crossbeam`. Use `Arc<RwLock<T>>` only when necessary. Consider `parking_lot` for better performance than std locks.

### 4. Batch Operations
Always batch network I/O, disk writes, and processing. Single operations are the enemy of throughput. Design APIs to encourage batching.

## Rust Coding Guidelines

### Error Handling
```rust
// Use thiserror for library errors
#[derive(Debug, thiserror::Error)]
pub enum BrokerError {
    #[error("Topic not found: {0}")]
    TopicNotFound(String),
    
    #[error("Authorization failed: {reason}")]
    AuthorizationFailed { reason: String },
}

// Use anyhow for application errors
use anyhow::{Result, Context};

pub async fn start_broker() -> Result<()> {
    let config = load_config()
        .context("Failed to load broker configuration")?;
    // ...
}
```

### Async Patterns
```rust
// Always use async/await for I/O operations
pub async fn publish_message(&mut self, msg: Message) -> Result<()> {
    // Prefer tokio::select! for multiple async operations
    tokio::select! {
        result = self.send_to_broker(&msg) => result?,
        _ = tokio::time::sleep(Duration::from_secs(5)) => {
            return Err(BrokerError::Timeout.into());
        }
    }
    Ok(())
}

// Use Tokio's bounded channels for backpressure
let (tx, rx) = tokio::sync::mpsc::channel(1000);
```

### Performance Critical Code
```rust
// Use #[inline] for hot path functions
#[inline]
fn check_authorization(&self, principal: &Principal, resource: &Resource) -> bool {
    // Fast path: check cache first
    if let Some(cached) = self.auth_cache.get(&(principal.id(), resource.id())) {
        return *cached;
    }
    // Slow path: evaluate ACLs
    self.evaluate_acls(principal, resource)
}

// Use SIMD where beneficial
use std::simd::*;
fn compute_checksum_simd(data: &[u8]) -> u32 {
    // Process 16 bytes at a time using SIMD
    // ...
}

// Prefer stack allocation for small objects
#[derive(Debug, Clone, Copy)] // Copy for stack semantics
pub struct PartitionId(u32);
```

### Memory Management
```rust
// Use arena allocation for batch processing
use bumpalo::Bump;

pub fn process_batch(messages: &[Message]) -> Result<()> {
    let arena = Bump::new();
    
    for msg in messages {
        let processed = arena.alloc(ProcessedMessage {
            // ... process without heap allocation
        });
    }
    // Arena deallocates everything at once
}

// Use object pools for frequently allocated objects
use object_pool::Pool;

lazy_static! {
    static ref MESSAGE_POOL: Pool<Message> = Pool::new(
        1000,
        || Message::default()
    );
}
```

### Testing Patterns
```rust
// Use property-based testing for invariants
#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    
    proptest! {
        #[test]
        fn test_acl_evaluation_consistency(
            principal in any::<Principal>(),
            resource in any::<Resource>(),
        ) {
            let result1 = evaluate_acl(&principal, &resource);
            let result2 = evaluate_acl(&principal, &resource);
            prop_assert_eq!(result1, result2); // ACL evaluation is deterministic
        }
    }
    
    // Use deterministic testing for distributed systems
    #[test]
    fn test_raft_election_deterministic() {
        let mut sim = Simulation::new(123); // Seed for determinism
        // Test election scenarios...
    }
}
```

### Security Patterns
```rust
// Always validate input
pub fn create_topic(name: &str, config: TopicConfig) -> Result<Topic> {
    // Validate topic name
    if !is_valid_topic_name(name) {
        return Err(BrokerError::InvalidTopicName(name.to_string()).into());
    }
    
    // Validate configuration
    config.validate()?;
    
    // Check authorization
    let principal = current_principal()?;
    if !authorize(&principal, Resource::Cluster, Operation::CreateTopic) {
        return Err(BrokerError::AuthorizationFailed {
            reason: format!("Principal {} cannot create topics", principal.name),
        }.into());
    }
    
    // ... create topic
}

// Use constant-time comparison for secrets
use subtle::ConstantTimeEq;

fn verify_token(provided: &[u8], expected: &[u8]) -> bool {
    provided.ct_eq(expected).into()
}
```

### Concurrency Patterns
```rust
// Use RwLock for read-heavy workloads
use parking_lot::RwLock;
use std::sync::Arc;

pub struct TopicManager {
    topics: Arc<RwLock<HashMap<String, Topic>>>,
}

impl TopicManager {
    pub fn get_topic(&self, name: &str) -> Option<Topic> {
        self.topics.read().get(name).cloned()
    }
    
    pub fn create_topic(&self, name: String, topic: Topic) -> Result<()> {
        let mut topics = self.topics.write();
        topics.insert(name, topic);
        Ok(())
    }
}

// Use crossbeam for lock-free data structures
use crossbeam::queue::ArrayQueue;

pub struct MessageQueue {
    queue: ArrayQueue<Message>,
}
```

## Project Structure

```
memorystreamer/
├── Cargo.toml                 # Workspace definition
├── CLAUDE.md                   # This file
├── TODO.md                     # Task tracking
├── benchmarks/                 # Performance benchmarks
│   └── throughput.rs
├── proto/                      # Protocol Buffer definitions
│   └── messages.proto
├── src/
│   ├── broker/                # Broker implementation
│   │   ├── mod.rs
│   │   ├── topic_manager.rs
│   │   └── partition.rs
│   ├── client/                # Client libraries
│   │   ├── publisher.rs
│   │   └── consumer.rs
│   ├── common/                # Shared code
│   │   ├── config.rs
│   │   └── errors.rs
│   ├── consensus/             # Raft implementation
│   │   ├── mod.rs
│   │   ├── node.rs
│   │   └── log.rs
│   ├── security/              # Security framework
│   │   ├── auth.rs
│   │   ├── acl.rs
│   │   └── encryption.rs
│   ├── protocol/              # Wire protocol
│   │   ├── frame.rs
│   │   └── codec.rs
│   └── lib.rs
├── tests/                     # Integration tests
│   ├── integration/
│   └── chaos/
└── ebpf/                      # eBPF programs
    └── src/
```

## Common Pitfalls to Avoid

### 1. Premature Optimization
Don't optimize without benchmarking first. Write clean, correct code, then profile and optimize hot paths.

### 2. Ignoring Backpressure
Always implement backpressure in async systems. Use bounded channels and respect capacity limits.

### 3. Blocking in Async Context
Never use blocking I/O in async functions. Use `tokio::task::spawn_blocking` for CPU-intensive work.

### 4. Unnecessary Allocations
Avoid `String` when `&str` suffices. Use `Cow<'_, str>` for conditional ownership. Prefer `SmallVec` for small collections.

### 5. Security as Afterthought
Every new feature must consider security implications. Add authorization checks before implementing functionality.

## Testing Requirements

### Unit Tests
- Minimum 95% code coverage
- Test both success and failure paths
- Use property-based testing for invariants

### Integration Tests
- Test component interactions
- Simulate network failures
- Test security boundaries

### Performance Tests
- Benchmark every commit
- Detect regressions > 1%
- Compare against Kafka baseline

### Security Tests
- Test authentication bypass attempts
- Verify authorization boundaries
- Validate encryption correctness

## Performance Optimization Checklist

Before optimizing, always:
1. ✓ Benchmark current performance
2. ✓ Profile to identify bottlenecks
3. ✓ Set measurable improvement target
4. ✓ Consider algorithmic improvements first
5. ✓ Implement optimization
6. ✓ Benchmark again to verify improvement
7. ✓ Add regression test

## Security Checklist

For every feature:
1. ✓ Identify principals and resources
2. ✓ Define required permissions
3. ✓ Implement authorization checks
4. ✓ Add audit logging
5. ✓ Validate all inputs
6. ✓ Handle errors securely (no information leakage)
7. ✓ Write security tests
8. ✓ Document security model

## Git Commit Guidelines

```bash
# Format: <type>(<scope>): <subject>
# Types: feat, fix, docs, test, perf, refactor, security
# Example:
git commit -m "feat(broker): implement topic-level ACLs"
git commit -m "perf(raft): optimize log replication with batching"
git commit -m "security(auth): add JWT token validation"
```

## Questions to Ask Before Implementation

1. **Does this maintain our performance targets?**
   - Will it handle 10M msgs/sec?
   - Will it add >1μs latency?

2. **Is it secure by default?**
   - Are all operations authorized?
   - Is data encrypted appropriately?

3. **Will it scale?**
   - Can it handle 10,000 topics?
   - Can it support 1,000 nodes?

4. **Is it testable?**
   - Can we unit test it?
   - Can we chaos test it?

5. **Is it maintainable?**
   - Is the code self-documenting?
   - Are errors handled gracefully?

## Getting Started with Tasks

When implementing a task from TODO.md:

1. **Read the task description** and understand its context in the weekly plan
2. **Check dependencies** - ensure prerequisite tasks are complete
3. **Write tests first** - TDD approach for better design
4. **Implement the feature** following the guidelines above
5. **Benchmark if performance-critical** - ensure no regression
6. **Update documentation** - keep docs in sync with code
7. **Submit focused PR** - one feature per pull request

## Resources

- [Raft Consensus Paper](https://raft.github.io/raft.pdf)
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)
- [eBPF Documentation](https://ebpf.io/)
- [Protocol Buffers](https://protobuf.dev/)
- [Rust Performance Book](https://nnethercote.github.io/perf-book/)

## Contact for Claude Code

When you encounter complex architectural decisions or need clarification:
- Reference the development plan in `/docs/development-plan.md`
- Check existing patterns in similar components
- Maintain consistency with established patterns
- Prioritize correctness, then security, then performance

Remember: We're building a production system that organizations will trust with critical data. Every line of code matters.