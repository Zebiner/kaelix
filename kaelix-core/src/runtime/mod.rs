//! Ultra-High-Performance Async Runtime for MemoryStreamer
//!
//! This module provides a custom async runtime optimized for MemoryStreamer's demanding
//! performance requirements of <10μs P99 latency with 10M+ messages/second throughput.
//!
//! # Key Features
//!
//! - **Zero-allocation task scheduling**: Pre-allocated task pools for hot paths
//! - **NUMA-aware worker placement**: Workers pinned to optimal NUMA nodes and CPU cores
//! - **Work-stealing optimization**: Efficient load balancing between worker threads
//! - **Lock-free task queues**: High-performance concurrent data structures
//! - **CPU affinity management**: Explicit CPU core binding for deterministic performance
//! - **Real-time metrics**: Comprehensive latency and throughput monitoring
//!
//! # Performance Targets
//!
//! - Task spawn latency: <1μs
//! - Context switch overhead: <100ns  
//! - End-to-end message latency: <10μs P99
//! - Throughput capability: 10M+ messages/second
//! - Memory allocation: Zero allocations in hot paths
//! - CPU utilization: >95% efficiency across all cores
//!
//! # Architecture Overview
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                 OptimizedRuntime                            │
//! ├─────────────────────────────────────────────────────────────┤
//! │  Task Scheduler (NUMA-aware)                               │
//! │  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐          │
//! │  │ Worker 0    │ │ Worker 1    │ │ Worker N    │          │
//! │  │ CPU 0-3     │ │ CPU 4-7     │ │ CPU N-M     │          │
//! │  │ NUMA Node 0 │ │ NUMA Node 1 │ │ NUMA Node X │          │
//! │  └─────────────┘ └─────────────┘ └─────────────┘          │
//! │                                                            │
//! │  Global Task Queue (Lock-free)                            │
//! │  ┌──────────────────────────────────────────────────────┐ │
//! │  │ Injector Queue → Work Stealing Deques               │ │
//! │  └──────────────────────────────────────────────────────┘ │
//! │                                                            │
//! │  Task Pool (Zero-allocation)                              │
//! │  ┌──────────────────────────────────────────────────────┐ │
//! │  │ Pre-allocated Task Slots + Free List                │ │
//! │  └──────────────────────────────────────────────────────┘ │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Usage Example
//!
//! ```rust
//! use kaelix_core::runtime::{OptimizedRuntime, RuntimeConfig};
//! use std::time::Duration;
//!
//! # tokio_test::block_on(async {
//! let config = RuntimeConfig::optimized();
//! let runtime = OptimizedRuntime::new(config).unwrap();
//!
//! // Spawn high-performance task
//! let handle = runtime.spawn(async {
//!     // Ultra-low latency message processing
//!     42
//! }).await;
//!
//! let result = handle.await;
//! # });
//! ```

pub mod affinity;
pub mod executor;
pub mod metrics;
pub mod scheduler;
pub mod worker;

// Re-export core types
pub use affinity::{CpuSet, NumaTopology};
pub use executor::{OptimizedRuntime, RuntimeConfig, RuntimeError};
pub use metrics::{LatencyHistogram, RuntimeMetrics, UtilizationReport};
pub use scheduler::{TaskScheduler, TaskQueues};
pub use worker::{WorkerThread, WorkerId};

/// Performance constants for the optimized runtime
pub mod performance {
    use std::time::Duration;

    /// Maximum allowed task spawn latency
    pub const MAX_SPAWN_LATENCY: Duration = Duration::from_nanos(1000); // 1μs

    /// Maximum allowed context switch overhead
    pub const MAX_CONTEXT_SWITCH_OVERHEAD: Duration = Duration::from_nanos(100); // 100ns

    /// Target end-to-end latency P99
    pub const TARGET_P99_LATENCY: Duration = Duration::from_micros(10);

    /// Target throughput (messages per second)
    pub const TARGET_THROUGHPUT_MPS: u64 = 10_000_000;

    /// Minimum CPU utilization efficiency target
    pub const MIN_CPU_EFFICIENCY: f64 = 0.95;

    /// Default task pool size (pre-allocated tasks)
    pub const DEFAULT_TASK_POOL_SIZE: usize = 1_000_000;

    /// Default queue depth per worker
    pub const DEFAULT_QUEUE_DEPTH: usize = 65536;

    /// Cache line size for memory alignment
    pub const CACHE_LINE_SIZE: usize = 64;

    /// Memory page size for NUMA allocation alignment
    pub const PAGE_SIZE: usize = 4096;
}

/// Common result type for runtime operations
pub type RuntimeResult<T> = Result<T, RuntimeError>;

#[cfg(test)]
mod tests {
    use super::*;
    use super::performance::*;

    #[test]
    fn test_performance_constants() {
        assert_eq!(MAX_SPAWN_LATENCY.as_nanos(), 1000);
        assert_eq!(MAX_CONTEXT_SWITCH_OVERHEAD.as_nanos(), 100);
        assert_eq!(TARGET_P99_LATENCY.as_micros(), 10);
        assert_eq!(TARGET_THROUGHPUT_MPS, 10_000_000);
        assert!((MIN_CPU_EFFICIENCY - 0.95).abs() < f64::EPSILON);
        assert_eq!(DEFAULT_TASK_POOL_SIZE, 1_000_000);
        assert!(DEFAULT_QUEUE_DEPTH.is_power_of_two());
        assert_eq!(CACHE_LINE_SIZE, 64);
        assert_eq!(PAGE_SIZE, 4096);
    }
}