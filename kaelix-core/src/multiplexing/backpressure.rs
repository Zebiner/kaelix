//! Sophisticated Backpressure Management
//!
//! Credit-based flow control system with pressure propagation and global system monitoring.
//! Designed for preventing cascade failures and maintaining system stability under load.

use crate::multiplexing::error::{MultiplexerError, StreamId};
use crossbeam::atomic::AtomicCell;
use dashmap::DashMap;
use smallvec::SmallVec;
use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicI32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Default initial credits per stream
const DEFAULT_INITIAL_CREDITS: i32 = 1000;

/// Minimum credits before backpressure activates
const BACKPRESSURE_THRESHOLD: i32 = 100;

/// Maximum credit refill rate per second
const MAX_REFILL_RATE: u64 = 10000;

/// Backpressure levels for graduated response
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum BackpressureLevel {
    /// Normal operation - no backpressure
    None = 0,
    /// Light pressure - minor delays
    Light = 1,
    /// Moderate pressure - noticeable delays
    Moderate = 2,
    /// Heavy pressure - significant delays
    Heavy = 3,
    /// Critical pressure - system at risk
    Critical = 4,
}

impl BackpressureLevel {
    /// Returns the delay multiplier for this backpressure level
    pub fn delay_multiplier(self) -> f64 {
        match self {
            BackpressureLevel::None => 1.0,
            BackpressureLevel::Light => 1.2,
            BackpressureLevel::Moderate => 1.5,
            BackpressureLevel::Heavy => 2.0,
            BackpressureLevel::Critical => 3.0,
        }
    }

    /// Returns the credit consumption multiplier for this level
    pub fn credit_multiplier(self) -> f64 {
        match self {
            BackpressureLevel::None => 1.0,
            BackpressureLevel::Light => 1.1,
            BackpressureLevel::Moderate => 1.3,
            BackpressureLevel::Heavy => 1.6,
            BackpressureLevel::Critical => 2.0,
        }
    }
}

/// Backpressure configuration
#[derive(Debug, Clone)]
pub struct BackpressureConfig {
    /// Initial credits per stream
    pub initial_credits: i32,
    /// Minimum credits before backpressure
    pub backpressure_threshold: i32,
    /// Maximum credit refill rate per second
    pub max_refill_rate: u64,
    /// Enable adaptive backpressure
    pub adaptive_enabled: bool,
    /// Pressure propagation enabled
    pub propagation_enabled: bool,
    /// Global monitoring enabled
    pub global_monitoring: bool,
    /// Credit refill interval in milliseconds
    pub refill_interval_ms: u64,
}

impl BackpressureConfig {
    /// Adaptive backpressure configuration
    pub fn adaptive() -> Self {
        Self {
            initial_credits: 2000,
            backpressure_threshold: 200,
            max_refill_rate: 20000,
            adaptive_enabled: true,
            propagation_enabled: true,
            global_monitoring: true,
            refill_interval_ms: 10,
        }
    }

    /// Conservative backpressure configuration
    pub fn conservative() -> Self {
        Self {
            initial_credits: 500,
            backpressure_threshold: 50,
            max_refill_rate: 5000,
            adaptive_enabled: false,
            propagation_enabled: false,
            global_monitoring: false,
            refill_interval_ms: 100,
        }
    }
}

impl Default for BackpressureConfig {
    fn default() -> Self {
        Self::adaptive()
    }
}

/// Flow control metrics for monitoring
#[derive(Debug, Default)]
pub struct BackpressureMetrics {
    /// Total backpressure events
    pub backpressure_events: AtomicU64,
    /// Credits consumed
    pub credits_consumed: AtomicU64,
    /// Credits refilled
    pub credits_refilled: AtomicU64,
    /// Streams under backpressure
    pub streams_under_pressure: AtomicUsize,
    /// Pressure propagation events
    pub propagation_events: AtomicU64,
    /// Critical pressure events
    pub critical_pressure_events: AtomicU64,
}

impl BackpressureMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Gets the current backpressure ratio
    pub fn backpressure_ratio(&self) -> f64 {
        let total_events = self.backpressure_events.load(Ordering::Relaxed);
        let critical_events = self.critical_pressure_events.load(Ordering::Relaxed);

        if total_events > 0 {
            critical_events as f64 / total_events as f64
        } else {
            0.0
        }
    }

    /// Gets credit efficiency (refilled / consumed)
    pub fn credit_efficiency(&self) -> f64 {
        let consumed = self.credits_consumed.load(Ordering::Relaxed);
        let refilled = self.credits_refilled.load(Ordering::Relaxed);

        if consumed > 0 {
            refilled as f64 / consumed as f64
        } else {
            1.0
        }
    }
}

/// Per-stream credit tracking
#[derive(Debug)]
struct StreamCreditState {
    /// Available credits
    credits: AtomicI32,
    /// Last refill timestamp
    last_refill: AtomicCell<Instant>,
    /// Backpressure level
    pressure_level: AtomicCell<BackpressureLevel>,
    /// Total credits consumed
    total_consumed: AtomicU64,
    /// Total credits refilled
    total_refilled: AtomicU64,
}

impl StreamCreditState {
    fn new(initial_credits: i32) -> Self {
        Self {
            credits: AtomicI32::new(initial_credits),
            last_refill: AtomicCell::new(Instant::now()),
            pressure_level: AtomicCell::new(BackpressureLevel::None),
            total_consumed: AtomicU64::new(0),
            total_refilled: AtomicU64::new(0),
        }
    }

    /// Attempts to consume credits
    fn try_consume(&self, amount: i32) -> Result<i32, MultiplexerError> {
        let current = self.credits.fetch_sub(amount, Ordering::AcqRel);
        if current < amount {
            // Insufficient credits - restore
            self.credits.fetch_add(amount, Ordering::Relaxed);
            Err(MultiplexerError::Backpressure(format!(
                "Insufficient credits: {} available, {} required",
                current, amount
            )))
        } else {
            self.total_consumed.fetch_add(amount as u64, Ordering::Relaxed);
            Ok(current - amount)
        }
    }

    /// Refills credits based on elapsed time
    fn refill_credits(&self, max_refill_rate: u64) {
        let now = Instant::now();
        let last_refill = self.last_refill.swap(now);
        let elapsed = now.duration_since(last_refill);

        // Calculate refill amount based on elapsed time
        let refill_amount = ((elapsed.as_millis() as u64 * max_refill_rate) / 1000) as i32;

        if refill_amount > 0 {
            // Add credits but don't exceed the initial amount
            let current = self.credits.load(Ordering::Relaxed);
            let new_credits = std::cmp::min(current + refill_amount, DEFAULT_INITIAL_CREDITS);
            let actual_refill = new_credits - current;

            if actual_refill > 0 {
                self.credits.store(new_credits, Ordering::Release);
                self.total_refilled.fetch_add(actual_refill as u64, Ordering::Relaxed);
            }
        }
    }

    /// Updates pressure level based on current credits
    fn update_pressure_level(&self, threshold: i32) -> BackpressureLevel {
        let credits = self.credits.load(Ordering::Relaxed);
        let level = if credits <= 0 {
            BackpressureLevel::Critical
        } else if credits < threshold / 4 {
            BackpressureLevel::Heavy
        } else if credits < threshold / 2 {
            BackpressureLevel::Moderate
        } else if credits < threshold {
            BackpressureLevel::Light
        } else {
            BackpressureLevel::None
        };

        self.pressure_level.store(level);
        level
    }

    /// Gets current pressure level
    fn pressure_level(&self) -> BackpressureLevel {
        self.pressure_level.load()
    }
}

/// Backpressure propagation graph
#[derive(Debug, Default)]
struct PropagationGraph {
    /// Upstream dependencies for each stream
    upstream: DashMap<StreamId, HashSet<StreamId>>,
    /// Downstream dependents for each stream
    downstream: DashMap<StreamId, HashSet<StreamId>>,
}

impl PropagationGraph {
    /// Adds a dependency relationship
    fn add_dependency(&self, downstream_id: StreamId, upstream_id: StreamId) {
        // Add to upstream map
        self.upstream
            .entry(downstream_id)
            .or_insert_with(HashSet::new)
            .insert(upstream_id);

        // Add to downstream map
        self.downstream
            .entry(upstream_id)
            .or_insert_with(HashSet::new)
            .insert(downstream_id);
    }

    /// Gets all streams that should be affected by backpressure from this stream
    fn get_propagation_targets(&self, stream_id: StreamId) -> SmallVec<[StreamId; 8]> {
        let mut targets = SmallVec::new();

        // Propagate to upstream (producers should slow down)
        if let Some(upstream_streams) = self.upstream.get(&stream_id) {
            targets.extend(upstream_streams.iter().copied());
        }

        targets
    }

    /// Removes a stream from the propagation graph
    fn remove_stream(&self, stream_id: StreamId) {
        // Remove from upstream relationships
        if let Some((_, upstream_streams)) = self.upstream.remove(&stream_id) {
            for upstream_id in upstream_streams {
                if let Some(mut downstream_set) = self.downstream.get_mut(&upstream_id) {
                    downstream_set.remove(&stream_id);
                }
            }
        }

        // Remove from downstream relationships
        if let Some((_, downstream_streams)) = self.downstream.remove(&stream_id) {
            for downstream_id in downstream_streams {
                if let Some(mut upstream_set) = self.upstream.get_mut(&downstream_id) {
                    upstream_set.remove(&stream_id);
                }
            }
        }
    }
}

/// Sophisticated backpressure management system
pub struct BackpressureManager {
    /// Per-stream credit state
    stream_states: DashMap<StreamId, Arc<StreamCreditState>>,

    /// Pressure propagation graph
    propagation_graph: PropagationGraph,

    /// Configuration
    config: BackpressureConfig,

    /// Performance metrics
    metrics: BackpressureMetrics,

    /// Global backpressure level
    global_pressure: AtomicCell<BackpressureLevel>,

    /// Pending refill queue
    refill_queue: Arc<parking_lot::Mutex<VecDeque<StreamId>>>,

    /// Creation timestamp
    created_at: Instant,
}

impl std::fmt::Debug for BackpressureManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackpressureManager")
            .field("config", &self.config)
            .field("metrics", &self.metrics)
            .field("global_pressure", &self.global_pressure.load())
            .field("stream_count", &self.stream_states.len())
            .field("created_at", &self.created_at)
            .finish()
    }
}

impl BackpressureManager {
    /// Creates a new backpressure manager
    pub fn new(config: BackpressureConfig) -> Self {
        Self {
            stream_states: DashMap::new(),
            propagation_graph: PropagationGraph::default(),
            config,
            metrics: BackpressureMetrics::new(),
            global_pressure: AtomicCell::new(BackpressureLevel::None),
            refill_queue: Arc::new(parking_lot::Mutex::new(VecDeque::new())),
            created_at: Instant::now(),
        }
    }

    /// Registers a stream for backpressure management
    pub fn register_stream(&self, stream_id: StreamId) -> Result<(), MultiplexerError> {
        if self.stream_states.contains_key(&stream_id) {
            return Err(MultiplexerError::Stream(format!(
                "Stream {} already registered for backpressure management",
                stream_id
            )));
        }

        let state = Arc::new(StreamCreditState::new(self.config.initial_credits));
        self.stream_states.insert(stream_id, state);

        Ok(())
    }

    /// Unregisters a stream
    pub fn unregister_stream(&self, stream_id: StreamId) -> Result<(), MultiplexerError> {
        self.stream_states.remove(&stream_id);
        self.propagation_graph.remove_stream(stream_id);

        Ok(())
    }

    /// Attempts to consume credits for a stream
    pub fn try_consume_credits(
        &self,
        stream_id: StreamId,
        amount: i32,
    ) -> Result<(), MultiplexerError> {
        // Get or create stream state
        let state = match self.stream_states.get(&stream_id) {
            Some(state) => state.clone(),
            None => {
                // Auto-register stream
                let state = Arc::new(StreamCreditState::new(self.config.initial_credits));
                self.stream_states.insert(stream_id, state.clone());
                state
            },
        };

        // Apply backpressure multiplier
        let pressure_level = state.pressure_level();
        let effective_amount = (amount as f64 * pressure_level.credit_multiplier()) as i32;

        // Try to consume credits
        match state.try_consume(effective_amount) {
            Ok(_) => {
                self.metrics
                    .credits_consumed
                    .fetch_add(effective_amount as u64, Ordering::Relaxed);
                Ok(())
            },
            Err(e) => {
                // Update pressure level
                let new_level = state.update_pressure_level(self.config.backpressure_threshold);

                // Increment backpressure metrics
                self.metrics.backpressure_events.fetch_add(1, Ordering::Relaxed);
                if new_level == BackpressureLevel::Critical {
                    self.metrics.critical_pressure_events.fetch_add(1, Ordering::Relaxed);
                }

                // Propagate backpressure if enabled
                if self.config.propagation_enabled {
                    self.propagate_backpressure(stream_id, new_level);
                }

                // Update global pressure
                self.update_global_pressure();

                Err(e)
            },
        }
    }

    /// Adds credits to a stream
    pub fn add_credits(&self, stream_id: StreamId, amount: i32) -> Result<(), MultiplexerError> {
        if let Some(state) = self.stream_states.get(&stream_id) {
            state.credits.fetch_add(amount, Ordering::Relaxed);
            state.total_refilled.fetch_add(amount as u64, Ordering::Relaxed);
            self.metrics.credits_refilled.fetch_add(amount as u64, Ordering::Relaxed);

            // Update pressure level
            state.update_pressure_level(self.config.backpressure_threshold);

            Ok(())
        } else {
            Err(MultiplexerError::Stream(format!("Stream {} not found", stream_id)))
        }
    }

    /// Adds a dependency relationship for pressure propagation
    pub fn add_dependency(&self, downstream_id: StreamId, upstream_id: StreamId) {
        self.propagation_graph.add_dependency(downstream_id, upstream_id);
    }

    /// Gets current backpressure level for a stream
    pub fn get_pressure_level(&self, stream_id: StreamId) -> BackpressureLevel {
        self.stream_states
            .get(&stream_id)
            .map(|state| state.pressure_level())
            .unwrap_or(BackpressureLevel::None)
    }

    /// Gets global backpressure level
    pub fn current_level(&self) -> BackpressureLevel {
        self.global_pressure.load()
    }

    /// Performs periodic credit refill
    pub fn refill_credits(&self) {
        for entry in self.stream_states.iter() {
            let state = entry.value();
            state.refill_credits(self.config.max_refill_rate);

            // Update pressure level after refill
            state.update_pressure_level(self.config.backpressure_threshold);
        }

        // Update global pressure after refills
        self.update_global_pressure();
    }

    /// Propagates backpressure to related streams
    fn propagate_backpressure(&self, source_stream: StreamId, pressure_level: BackpressureLevel) {
        if pressure_level == BackpressureLevel::None {
            return;
        }

        let targets = self.propagation_graph.get_propagation_targets(source_stream);

        for target_stream in targets {
            if let Some(state) = self.stream_states.get(&target_stream) {
                // Apply proportional pressure
                let current_pressure = state.pressure_level();
                let new_pressure = std::cmp::max(current_pressure, pressure_level);
                state.pressure_level.store(new_pressure);

                self.metrics.propagation_events.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Updates global pressure level based on individual streams
    fn update_global_pressure(&self) {
        let mut pressure_counts = [0usize; 5]; // Count for each pressure level

        for entry in self.stream_states.iter() {
            let level = entry.value().pressure_level() as usize;
            pressure_counts[level] += 1;
        }

        let total_streams = self.stream_states.len();
        if total_streams == 0 {
            self.global_pressure.store(BackpressureLevel::None);
            return;
        }

        // Determine global level based on distribution
        let critical_ratio = pressure_counts[4] as f64 / total_streams as f64;
        let heavy_ratio = pressure_counts[3] as f64 / total_streams as f64;
        let moderate_ratio = pressure_counts[2] as f64 / total_streams as f64;

        let global_level = if critical_ratio > 0.1 {
            BackpressureLevel::Critical
        } else if heavy_ratio > 0.2 {
            BackpressureLevel::Heavy
        } else if moderate_ratio > 0.3 {
            BackpressureLevel::Moderate
        } else if pressure_counts[1] > 0 {
            BackpressureLevel::Light
        } else {
            BackpressureLevel::None
        };

        self.global_pressure.store(global_level);

        // Update metrics
        self.metrics
            .streams_under_pressure
            .store(total_streams - pressure_counts[0], Ordering::Relaxed);
    }

    /// Gets current metrics snapshot
    pub fn get_metrics(&self) -> BackpressureMetricsSnapshot {
        BackpressureMetricsSnapshot {
            backpressure_events: self.metrics.backpressure_events.load(Ordering::Relaxed),
            credits_consumed: self.metrics.credits_consumed.load(Ordering::Relaxed),
            credits_refilled: self.metrics.credits_refilled.load(Ordering::Relaxed),
            streams_under_pressure: self.metrics.streams_under_pressure.load(Ordering::Relaxed),
            propagation_events: self.metrics.propagation_events.load(Ordering::Relaxed),
            critical_pressure_events: self.metrics.critical_pressure_events.load(Ordering::Relaxed),
            global_pressure_level: self.global_pressure.load(),
            backpressure_ratio: self.metrics.backpressure_ratio(),
            credit_efficiency: self.metrics.credit_efficiency(),
        }
    }
}

/// Backpressure metrics snapshot
#[derive(Debug, Clone)]
pub struct BackpressureMetricsSnapshot {
    pub backpressure_events: u64,
    pub credits_consumed: u64,
    pub credits_refilled: u64,
    pub streams_under_pressure: usize,
    pub propagation_events: u64,
    pub critical_pressure_events: u64,
    pub global_pressure_level: BackpressureLevel,
    pub backpressure_ratio: f64,
    pub credit_efficiency: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backpressure_manager_creation() {
        let config = BackpressureConfig::default();
        let manager = BackpressureManager::new(config);

        assert_eq!(manager.current_level(), BackpressureLevel::None);
    }

    #[test]
    fn test_stream_registration() {
        let config = BackpressureConfig::default();
        let manager = BackpressureManager::new(config);

        manager.register_stream(1).unwrap();
        assert_eq!(manager.get_pressure_level(1), BackpressureLevel::None);

        manager.unregister_stream(1).unwrap();
        assert_eq!(manager.get_pressure_level(1), BackpressureLevel::None);
    }

    #[test]
    fn test_credit_consumption() {
        let config = BackpressureConfig::default();
        let manager = BackpressureManager::new(config);

        manager.register_stream(1).unwrap();

        // Should succeed with initial credits
        assert!(manager.try_consume_credits(1, 100).is_ok());

        // Should eventually fail if we consume too much
        for _ in 0..50 {
            let _ = manager.try_consume_credits(1, 100);
        }

        // This should definitely fail
        assert!(manager.try_consume_credits(1, 100).is_err());
    }

    #[test]
    fn test_credit_refill() {
        let config = BackpressureConfig::default();
        let manager = BackpressureManager::new(config);

        manager.register_stream(1).unwrap();

        // Consume most credits
        for _ in 0..15 {
            let _ = manager.try_consume_credits(1, 100);
        }

        // Add credits manually
        manager.add_credits(1, 1000).unwrap();

        // Should work again
        assert!(manager.try_consume_credits(1, 100).is_ok());
    }

    #[test]
    fn test_pressure_levels() {
        assert_eq!(BackpressureLevel::None.delay_multiplier(), 1.0);
        assert_eq!(BackpressureLevel::Light.delay_multiplier(), 1.2);
        assert_eq!(BackpressureLevel::Critical.delay_multiplier(), 3.0);

        assert!(BackpressureLevel::None < BackpressureLevel::Light);
        assert!(BackpressureLevel::Light < BackpressureLevel::Critical);
    }

    #[test]
    fn test_dependency_tracking() {
        let config = BackpressureConfig::default();
        let manager = BackpressureManager::new(config);

        manager.add_dependency(2, 1); // Stream 2 depends on Stream 1

        let targets = manager.propagation_graph.get_propagation_targets(2);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0], 1);
    }

    #[test]
    fn test_metrics_collection() {
        let config = BackpressureConfig::default();
        let manager = BackpressureManager::new(config);

        manager.register_stream(1).unwrap();
        let _ = manager.try_consume_credits(1, 100);

        let metrics = manager.get_metrics();
        assert!(metrics.credits_consumed > 0);
    }
}
