# MemoryStreamer Development Guide
## Project Overview
MemoryStreamer is an ultra-high-performance distributed streaming system designed to revolutionize message streaming infrastructure with unprecedented speed and reliability.

## TODO.md: Master Development Plan
[... existing content ...]

## Rust Code Quality Guidelines

### 1. Architecture & Performance Guidelines

#### Zero-Copy and Memory Management
- Prioritize zero-copy designs using `bytes::Bytes` and reference counting
- Minimize memory allocations using `SmallVec` and stack-based storage
- Use memory-mapped files for large data sets
- Prefer borrowing over cloning

```rust
// Zero-copy message handling
struct Message {
    payload: Bytes,  // Cheap to clone, no deep copy
}

// Efficient memory usage with SmallVec
use smallvec::SmallVec;

struct BatchProcessor {
    batch: SmallVec<[Message; 16]>,  // Stack-based storage for small batches
}
```

#### Lock-Free Programming
- Prefer lock-free data structures from `crossbeam`
- Use atomic operations for synchronization
- Minimize lock contention

```rust
use crossbeam::queue::ArrayQueue;
use std::sync::atomic::{AtomicUsize, Ordering};

struct LockFreeCounter {
    value: AtomicUsize,
}

impl LockFreeCounter {
    fn increment(&self) -> usize {
        self.value.fetch_add(1, Ordering::SeqCst)
    }
}
```

#### Async/Await Optimization
- Use `tokio` runtime with careful task spawning
- Implement timeout and cancellation mechanisms
- Batch async operations when possible

```rust
async fn process_messages(messages: Vec<Message>) -> Result<()> {
    let tasks: Vec<_> = messages.into_iter()
        .map(|msg| tokio::spawn(process_message(msg)))
        .collect();

    // Wait with timeout
    tokio::select! {
        _ = futures::future::try_join_all(tasks) => Ok(()),
        _ = tokio::time::sleep(Duration::from_secs(10)) => {
            Err(ProcessingError::Timeout)
        }
    }
}
```

#### SIMD Optimization
- Use `std::simd` for vectorized computations
- Profile and benchmark SIMD implementations
- Apply SIMD only where significant performance gains are proven

```rust
use std::simd::*;

fn compute_checksum_simd(data: &[u8]) -> u32 {
    let mut checksum = u32x4::splat(0);
    
    for chunk in data.chunks(16) {
        let vec = u8x16::from_slice(chunk);
        // Vectorized computation
        checksum += vec.cast();
    }
    
    checksum.reduce_sum()
}
```

### 2. Safety & Correctness

#### Memory Safety Patterns
- Leverage Rust's ownership and borrowing system
- Use `Rc` and `Arc` for shared ownership
- Implement `Send` and `Sync` traits carefully

```rust
use std::sync::Arc;
use parking_lot::RwLock;

struct ThreadSafeCache<K, V> {
    inner: Arc<RwLock<HashMap<K, V>>>,
}

impl<K, V> ThreadSafeCache<K, V> 
where 
    K: Eq + std::hash::Hash,
    V: Clone,
{
    fn get(&self, key: &K) -> Option<V> {
        self.inner.read().get(key).cloned()
    }
}
```

#### Concurrency Safety
- Use `parking_lot` for more efficient locks
- Implement explicit `Send` and `Sync` bounds
- Use `#[derive(Send, Sync)]` when safe

```rust
use parking_lot::{Mutex, RwLock};

struct ConcurrentResource<T> {
    data: Mutex<T>,
    metadata: RwLock<ResourceMetadata>,
}

// Explicit Send/Sync implementation
unsafe impl<T: Send> Send for ConcurrentResource<T> {}
unsafe impl<T: Sync> Sync for ConcurrentResource<T> {}
```

#### Input Validation
- Validate all inputs at entry points
- Use `thiserror` for rich error types
- Implement invariant checks

```rust
#[derive(Debug, thiserror::Error)]
enum ValidationError {
    #[error("Invalid message size: {0} bytes")]
    MessageTooLarge(usize),
    #[error("Empty topic name not allowed")]
    EmptyTopicName,
}

fn validate_message(msg: &Message) -> Result<(), ValidationError> {
    if msg.payload.len() > MAX_MESSAGE_SIZE {
        return Err(ValidationError::MessageTooLarge(msg.payload.len()));
    }
    
    if msg.topic.is_empty() {
        return Err(ValidationError::EmptyTopicName);
    }
    
    Ok(())
}
```

### 3. Testing Standards

#### Unit Testing
- 95% minimum code coverage
- Test both happy and failure paths
- Use `proptest` for property-based testing

```rust
#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    
    proptest! {
        #[test]
        fn test_message_validation(
            payload in any::<Vec<u8>>(),
            topic in any::<String>(),
        ) {
            let msg = Message::new(payload, topic);
            prop_assert!(validate_message(&msg).is_ok());
        }
    }
}
```

#### Benchmarking
- Use `criterion` for microbenchmarks
- Compare against baseline performance
- Track performance regressions

```rust
#[cfg(test)]
mod benchmarks {
    use criterion::{black_box, criterion_group, criterion_main, Criterion};
    
    fn bench_message_processing(c: &mut Criterion) {
        let messages = generate_test_messages();
        
        c.bench_function("process_batch", |b| {
            b.iter(|| {
                black_box(process_messages(&messages))
            })
        });
    }
}
```

### 4. Security Guidelines

#### Cryptographic Practices
- Use constant-time comparison for secrets
- Implement secure random generation
- Sanitize and validate all inputs

```rust
use rand::rngs::OsRng;
use subtle::ConstantTimeEq;
use zeroize::Zeroize;

struct SecureToken {
    token: Vec<u8>,
}

impl SecureToken {
    fn verify(&self, other: &[u8]) -> bool {
        self.token.ct_eq(other).into()
    }
    
    fn generate() -> Self {
        let mut token = vec![0u8; 32];
        OsRng.fill_bytes(&mut token);
        
        Self { token }
    }
}

impl Drop for SecureToken {
    fn drop(&mut self) {
        self.token.zeroize();
    }
}
```

### 5. Code Style & Documentation

#### Documentation Standards
- Document public APIs thoroughly
- Include performance characteristics
- Use doc comments with examples

```rust
/// High-performance message broker with batched processing
///
/// # Performance
/// - Throughput: 10M msgs/sec
/// - Latency: <10μs P99
///
/// # Example
/// ```
/// let broker = MessageBroker::new();
/// broker.publish_batch(&messages)?;
/// ```
pub struct MessageBroker {
    // Implementation details
}
```

### 6. Quality Enforcement

#### Static Analysis
- Enable all Clippy lints
- Configure CI/CD for automatic checks
- Block merges with quality issues

```toml
# Cargo.toml
[workspace.lints.clippy]
pedantic = "deny"
nursery = "deny"
```

By following these guidelines, we ensure our Rust code is performant, safe, and maintainable. Always prioritize correctness, then security, and finally performance.