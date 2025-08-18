//! Sophisticated Backpressure Management
//!
//! Credit-based flow control system with pressure propagation and global system monitoring.
//! Designed for preventing cascade failures and maintaining system stability under load.

use crate::multiplexing::error::{
    BackpressureError, BackpressureResult, FlowControlError, PropagationError, StreamId,
};
use crossbeam::atomic::AtomicCell;
use dashmap::DashMap;
use parking_lot::RwLock;
use smallvec::SmallVec;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicI32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Default initial credits per stream
const DEFAULT_INITIAL_CREDITS: i32 = 1000;

/// Maximum credits a stream can accumulate
const MAX_CREDITS_PER_STREAM: i32 = 10000;

/// Minimum credits before throttling
const MIN_CREDITS_THRESHOLD: i32 = 100;

/// Global pressure monitoring interval
const PRESSURE_MONITORING_INTERVAL_MS: u64 = 100;

/// Maximum propagation depth to prevent infinite loops
const MAX_PROPAGATION_DEPTH: usize = 10;

/// Backpressure levels with percentage thresholds
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum BackpressureLevel {
    None = 0,       // 0-25% capacity utilization
    Low = 1,        // 25-50% capacity utilization
    Medium = 2,     // 50-75% capacity utilization
    High = 3,       // 75-90% capacity utilization
    Critical = 4,   // 90-100% capacity utilization
}

impl BackpressureLevel {
    pub fn from_utilization(utilization: f64) -> Self {
        match utilization {
            x if x < 0.25 => BackpressureLevel::None,
            x if x < 0.50 => BackpressureLevel::Low,
            x if x < 0.75 => BackpressureLevel::Medium,
            x if x < 0.90 => BackpressureLevel::High,
            _ => BackpressureLevel::Critical,
        }
    }
    
    pub fn to_utilization(&self) -> f64 {
        match self {
            BackpressureLevel::None => 0.125,      // 12.5% (middle of 0-25%)
            BackpressureLevel::Low => 0.375,       // 37.5% (middle of 25-50%)
            BackpressureLevel::Medium => 0.625,    // 62.5% (middle of 50-75%)
            BackpressureLevel::High => 0.825,      // 82.5% (middle of 75-90%)
            BackpressureLevel::Critical => 0.95,   // 95% (high end of 90-100%)
        }
    }
    
    pub fn requires_throttling(&self) -> bool {
        *self >= BackpressureLevel::Medium
    }
    
    pub fn credit_multiplier(&self) -> f64 {
        match self {
            BackpressureLevel::None => 1.0,
            BackpressureLevel::Low => 0.9,
            BackpressureLevel::Medium => 0.7,
            BackpressureLevel::High => 0.5,
            BackpressureLevel::Critical => 0.2,
        }
    }
}

/// Per-stream backpressure state
#[derive(Debug)]
pub struct BackpressureState {
    pub level: AtomicCell<BackpressureLevel>,
    pub credits: AtomicI32,
    pub queue_depth: AtomicUsize,
    pub processing_rate: AtomicCell<f64>, // Messages per second
    pub last_updated: AtomicU64,
    pub credit_limit: AtomicI32,
    pub throttle_start: AtomicU64,
    pub pressure_score: AtomicCell<f64>,
}

impl BackpressureState {
    pub fn new(initial_credits: i32) -> Self {
        Self {
            level: AtomicCell::new(BackpressureLevel::None),
            credits: AtomicI32::new(initial_credits),
            queue_depth: AtomicUsize::new(0),
            processing_rate: AtomicCell::new(0.0),
            last_updated: AtomicU64::new(Self::current_time_ns()),
            credit_limit: AtomicI32::new(MAX_CREDITS_PER_STREAM),
            throttle_start: AtomicU64::new(0),
            pressure_score: AtomicCell::new(0.0),
        }
    }
    
    fn current_time_ns() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64
    }
    
    pub fn get_level(&self) -> BackpressureLevel {
        self.level.load()
    }
    
    pub fn set_level(&self, level: BackpressureLevel) {
        self.level.store(level);
        self.last_updated.store(Self::current_time_ns(), Ordering::Relaxed);
        
        if level.requires_throttling() && self.throttle_start.load(Ordering::Relaxed) == 0 {
            self.throttle_start.store(Self::current_time_ns(), Ordering::Relaxed);
        } else if !level.requires_throttling() {
            self.throttle_start.store(0, Ordering::Relaxed);
        }
    }
    
    pub fn get_credits(&self) -> i32 {
        self.credits.load(Ordering::Acquire)
    }
    
    pub fn set_credits(&self, credits: i32) {
        let limit = self.credit_limit.load(Ordering::Relaxed);
        let bounded_credits = credits.min(limit).max(0);
        self.credits.store(bounded_credits, Ordering::Release);
    }
    
    pub fn add_credits(&self, count: i32) -> i32 {
        let limit = self.credit_limit.load(Ordering::Relaxed);
        let new_credits = self.credits.fetch_add(count, Ordering::AcqRel) + count;
        
        if new_credits > limit {
            let excess = new_credits - limit;
            self.credits.fetch_sub(excess, Ordering::AcqRel);
            limit
        } else {
            new_credits
        }
    }
    
    pub fn try_consume_credits(&self, count: i32) -> bool {
        let current = self.credits.load(Ordering::Acquire);
        if current < count {
            return false;
        }
        
        self.credits
            .compare_exchange(current, current - count, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
    
    pub fn update_queue_depth(&self, depth: usize) {
        self.queue_depth.store(depth, Ordering::Relaxed);
        self.update_pressure_score();
    }
    
    pub fn update_processing_rate(&self, rate: f64) {
        self.processing_rate.store(rate);
        self.update_pressure_score();
    }
    
    fn update_pressure_score(&self) {
        let queue_depth = self.queue_depth.load(Ordering::Relaxed) as f64;
        let processing_rate = self.processing_rate.load();
        let credits = self.credits.load(Ordering::Relaxed) as f64;
        let credit_limit = self.credit_limit.load(Ordering::Relaxed) as f64;
        
        // Calculate pressure score (0.0 = no pressure, 1.0 = maximum pressure)
        let queue_pressure = (queue_depth / 1000.0).min(1.0); // Assume 1000 is high queue depth
        let credit_pressure = 1.0 - (credits / credit_limit);
        let rate_pressure = if processing_rate > 0.0 {
            (queue_depth / (processing_rate * 10.0)).min(1.0) // 10 seconds to clear queue
        } else {
            1.0
        };
        
        let combined_pressure = (queue_pressure * 0.4 + credit_pressure * 0.3 + rate_pressure * 0.3);
        self.pressure_score.store(combined_pressure);
        
        // Update backpressure level based on pressure score
        let new_level = BackpressureLevel::from_utilization(combined_pressure);
        self.set_level(new_level);
    }
    
    pub fn get_pressure_score(&self) -> f64 {
        self.pressure_score.load()
    }
    
    pub fn is_throttling(&self) -> bool {
        self.get_level().requires_throttling()
    }
    
    pub fn get_throttle_duration_ms(&self) -> u64 {
        let start = self.throttle_start.load(Ordering::Relaxed);
        if start == 0 {
            return 0;
        }
        
        let current = Self::current_time_ns();
        ((current - start) / 1_000_000).max(0) // Convert to milliseconds
    }
}

/// Global system pressure state
#[derive(Debug, Clone)]
pub struct GlobalPressureState {
    pub overall_level: BackpressureLevel,
    pub active_streams: usize,
    pub throttled_streams: usize,
    pub total_credits: i64,
    pub average_pressure_score: f64,
    pub memory_pressure: f64,
    pub cpu_pressure: f64,
    pub network_pressure: f64,
    pub last_updated: Instant,
}

impl GlobalPressureState {
    pub fn new() -> Self {
        Self {
            overall_level: BackpressureLevel::None,
            active_streams: 0,
            throttled_streams: 0,
            total_credits: 0,
            average_pressure_score: 0.0,
            memory_pressure: 0.0,
            cpu_pressure: 0.0,
            network_pressure: 0.0,
            last_updated: Instant::now(),
        }
    }
    
    pub fn is_healthy(&self) -> bool {
        self.overall_level < BackpressureLevel::Critical &&
        self.average_pressure_score < 0.8 &&
        self.memory_pressure < 0.9 &&
        self.cpu_pressure < 0.9
    }
    
    pub fn requires_global_throttling(&self) -> bool {
        self.overall_level >= BackpressureLevel::High ||
        self.memory_pressure > 0.85 ||
        self.cpu_pressure > 0.85
    }
}

/// Stream dependency graph for pressure propagation
#[derive(Debug)]
pub struct PropagationGraph {
    pub dependencies: DashMap<StreamId, SmallVec<[StreamId; 4]>>, // Stream -> dependencies
    pub dependents: DashMap<StreamId, SmallVec<[StreamId; 4]>>,   // Stream -> dependents
    pub propagation_weights: DashMap<(StreamId, StreamId), f64>,  // Edge weights
}

impl PropagationGraph {
    pub fn new() -> Self {
        Self {
            dependencies: DashMap::new(),
            dependents: DashMap::new(),
            propagation_weights: DashMap::new(),
        }
    }
    
    pub fn add_dependency(&self, dependent: StreamId, dependency: StreamId, weight: f64) {
        // Add to dependencies map
        self.dependencies
            .entry(dependent)
            .or_insert_with(SmallVec::new)
            .push(dependency);
        
        // Add to dependents map
        self.dependents
            .entry(dependency)
            .or_insert_with(SmallVec::new)
            .push(dependent);
        
        // Store propagation weight
        self.propagation_weights.insert((dependent, dependency), weight);
    }
    
    pub fn remove_dependency(&self, dependent: StreamId, dependency: StreamId) {
        // Remove from dependencies map
        if let Some(mut deps) = self.dependencies.get_mut(&dependent) {
            deps.retain(|&id| id != dependency);
        }
        
        // Remove from dependents map
        if let Some(mut deps) = self.dependents.get_mut(&dependency) {
            deps.retain(|&id| id != dependent);
        }
        
        // Remove propagation weight
        self.propagation_weights.remove(&(dependent, dependency));
    }
    
    pub fn get_dependencies(&self, stream_id: StreamId) -> Vec<StreamId> {
        self.dependencies
            .get(&stream_id)
            .map(|deps| deps.iter().copied().collect())
            .unwrap_or_default()
    }
    
    pub fn get_dependents(&self, stream_id: StreamId) -> Vec<StreamId> {
        self.dependents
            .get(&stream_id)
            .map(|deps| deps.iter().copied().collect())
            .unwrap_or_default()
    }
    
    pub fn get_propagation_weight(&self, dependent: StreamId, dependency: StreamId) -> f64 {
        self.propagation_weights
            .get(&(dependent, dependency))
            .map(|weight| *weight)
            .unwrap_or(1.0) // Default weight
    }
}

/// Credit-based flow controller
#[derive(Debug)]
pub struct FlowController {
    pub default_credits: i32,
    pub credit_refresh_rate: f64, // Credits per second
    pub burst_allowance: i32,
    pub decay_rate: f64, // Rate at which unused credits decay
}

impl FlowController {
    pub fn new() -> Self {
        Self {
            default_credits: DEFAULT_INITIAL_CREDITS,
            credit_refresh_rate: 100.0, // 100 credits per second
            burst_allowance: 500,
            decay_rate: 0.1, // 10% decay per second for unused credits
        }
    }
    
    pub fn allocate_initial_credits(&self, stream_id: StreamId) -> i32 {
        // Could use stream_id for per-stream customization
        let _ = stream_id;
        self.default_credits
    }
    
    pub fn calculate_credit_refresh(&self, elapsed_ms: u64, current_level: BackpressureLevel) -> i32 {
        let elapsed_secs = elapsed_ms as f64 / 1000.0;
        let base_refresh = (self.credit_refresh_rate * elapsed_secs) as i32;
        let multiplier = current_level.credit_multiplier();
        (base_refresh as f64 * multiplier) as i32
    }
    
    pub fn should_throttle(&self, state: &BackpressureState) -> bool {
        state.get_credits() < MIN_CREDITS_THRESHOLD ||
        state.get_level().requires_throttling()
    }
    
    pub fn get_throttle_delay_ms(&self, state: &BackpressureState) -> u64 {
        match state.get_level() {
            BackpressureLevel::None | BackpressureLevel::Low => 0,
            BackpressureLevel::Medium => 1,
            BackpressureLevel::High => 5,
            BackpressureLevel::Critical => 10,
        }
    }
}

/// Backpressure management metrics
#[derive(Debug, Default)]
pub struct BackpressureMetrics {
    pub credit_allocations: AtomicU64,
    pub credit_consumptions: AtomicU64,
    pub throttle_events: AtomicU64,
    pub pressure_propagations: AtomicU64,
    pub global_pressure_events: AtomicU64,
    pub flow_control_violations: AtomicU64,
    pub credit_refreshes: AtomicU64,
    pub total_credits_allocated: AtomicU64,
    pub total_credits_consumed: AtomicU64,
    pub average_pressure_score: AtomicCell<f64>,
}

impl BackpressureMetrics {
    pub fn record_credit_allocation(&self, credits: i32) {
        self.credit_allocations.fetch_add(1, Ordering::Relaxed);
        self.total_credits_allocated.fetch_add(credits as u64, Ordering::Relaxed);
    }
    
    pub fn record_credit_consumption(&self, credits: i32) {
        self.credit_consumptions.fetch_add(1, Ordering::Relaxed);
        self.total_credits_consumed.fetch_add(credits as u64, Ordering::Relaxed);
    }
    
    pub fn record_throttle_event(&self) {
        self.throttle_events.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_pressure_propagation(&self) {
        self.pressure_propagations.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_global_pressure_event(&self) {
        self.global_pressure_events.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_flow_violation(&self) {
        self.flow_control_violations.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_credit_refresh(&self) {
        self.credit_refreshes.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn update_average_pressure(&self, pressure: f64) {
        self.average_pressure_score.store(pressure);
    }
    
    pub fn get_credit_utilization_rate(&self) -> f64 {
        let allocated = self.total_credits_allocated.load(Ordering::Relaxed) as f64;
        let consumed = self.total_credits_consumed.load(Ordering::Relaxed) as f64;
        if allocated == 0.0 {
            return 0.0;
        }
        consumed / allocated
    }
    
    pub fn get_throttle_rate(&self) -> f64 {
        let throttles = self.throttle_events.load(Ordering::Relaxed) as f64;
        let allocations = self.credit_allocations.load(Ordering::Relaxed) as f64;
        if allocations == 0.0 {
            return 0.0;
        }
        throttles / allocations
    }
}

/// Backpressure manager configuration
#[derive(Debug, Clone)]
pub struct BackpressureConfig {
    pub default_credits: i32,
    pub max_credits_per_stream: i32,
    pub credit_refresh_interval_ms: u64,
    pub pressure_monitoring_interval_ms: u64,
    pub enable_global_monitoring: bool,
    pub enable_pressure_propagation: bool,
    pub propagation_depth_limit: usize,
    pub throttle_threshold: f64,
    pub burst_allowance: i32,
}

impl Default for BackpressureConfig {
    fn default() -> Self {
        Self {
            default_credits: DEFAULT_INITIAL_CREDITS,
            max_credits_per_stream: MAX_CREDITS_PER_STREAM,
            credit_refresh_interval_ms: 100,
            pressure_monitoring_interval_ms: PRESSURE_MONITORING_INTERVAL_MS,
            enable_global_monitoring: true,
            enable_pressure_propagation: true,
            propagation_depth_limit: MAX_PROPAGATION_DEPTH,
            throttle_threshold: 0.7,
            burst_allowance: 500,
        }
    }
}

/// Sophisticated Backpressure Manager
#[derive(Debug)]
pub struct BackpressureManager {
    /// Per-stream backpressure state
    stream_pressure: DashMap<StreamId, Arc<BackpressureState>>,
    /// Global system pressure monitoring
    global_pressure: Arc<AtomicCell<GlobalPressureState>>,
    /// Pressure propagation graph
    propagation_graph: Arc<PropagationGraph>,
    /// Credit-based flow controller
    flow_controller: Arc<FlowController>,
    /// Backpressure metrics
    metrics: BackpressureMetrics,
    /// Configuration
    config: BackpressureConfig,
    /// Last global pressure update
    last_global_update: AtomicU64,
}

impl BackpressureManager {
    /// Create a new backpressure manager
    pub fn new(config: BackpressureConfig) -> Self {
        Self {
            stream_pressure: DashMap::new(),
            global_pressure: Arc::new(AtomicCell::new(GlobalPressureState::new())),
            propagation_graph: Arc::new(PropagationGraph::new()),
            flow_controller: Arc::new(FlowController::new()),
            metrics: BackpressureMetrics::default(),
            config,
            last_global_update: AtomicU64::new(BackpressureState::current_time_ns()),
        }
    }
    
    /// Allocate credits to a stream
    pub async fn allocate_credits(&self, stream_id: StreamId, count: i32) -> Result<(), FlowControlError> {
        let state = self.get_or_create_stream_state(stream_id);
        
        // Check if allocation would exceed limits
        let current_credits = state.get_credits();
        let credit_limit = state.credit_limit.load(Ordering::Relaxed);
        
        if current_credits + count > credit_limit {
            return Err(FlowControlError::AllocationFailed { stream_id });
        }
        
        state.add_credits(count);
        self.metrics.record_credit_allocation(count);
        
        Ok(())
    }
    
    /// Consume credits from a stream
    pub async fn consume_credits(&self, stream_id: StreamId, count: i32) -> Result<bool, FlowControlError> {
        let state = self.get_or_create_stream_state(stream_id);
        
        // Try to consume credits atomically
        if state.try_consume_credits(count) {
            self.metrics.record_credit_consumption(count);
            
            // Update stream activity
            state.update_queue_depth(state.queue_depth.load(Ordering::Relaxed).saturating_sub(1));
            
            Ok(true)
        } else {
            // Insufficient credits
            self.metrics.record_flow_violation();
            
            // Check if this should trigger throttling
            if self.flow_controller.should_throttle(&state) {
                self.metrics.record_throttle_event();
            }
            
            Ok(false)
        }
    }
    
    /// Get available credits for a stream
    pub fn get_available_credits(&self, stream_id: StreamId) -> i32 {
        if let Some(state) = self.stream_pressure.get(&stream_id) {
            state.get_credits()
        } else {
            0
        }
    }
    
    /// Report pressure level for a stream
    pub async fn report_pressure(&self, stream_id: StreamId, level: BackpressureLevel) -> BackpressureResult<()> {
        let state = self.get_or_create_stream_state(stream_id);
        let old_level = state.get_level();
        
        state.set_level(level);
        
        // Trigger pressure propagation if level increased significantly
        if level > old_level && level >= BackpressureLevel::Medium {
            if self.config.enable_pressure_propagation {
                self.propagate_pressure(stream_id).await?;
            }
        }
        
        // Update global pressure monitoring
        if self.config.enable_global_monitoring {
            self.update_global_pressure().await;
        }
        
        Ok(())
    }
    
    /// Propagate pressure to dependent streams
    pub async fn propagate_pressure(&self, stream_id: StreamId) -> Result<(), PropagationError> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        
        queue.push_back((stream_id, 0u32)); // (stream_id, depth)
        visited.insert(stream_id);
        
        while let Some((current_stream, depth)) = queue.pop_front() {
            if depth >= self.config.propagation_depth_limit as u32 {
                return Err(PropagationError::DepthExceeded {
                    depth: depth as usize,
                    max: self.config.propagation_depth_limit,
                });
            }
            
            // Get dependents of current stream
            let dependents = self.propagation_graph.get_dependents(current_stream);
            
            for dependent in dependents {
                if visited.contains(&dependent) {
                    continue; // Avoid cycles
                }
                
                visited.insert(dependent);
                
                // Calculate propagated pressure
                let weight = self.propagation_graph.get_propagation_weight(dependent, current_stream);
                let source_state = self.get_or_create_stream_state(current_stream);
                let source_pressure = source_state.get_pressure_score();
                
                let propagated_pressure = source_pressure * weight * 0.8; // Damping factor
                
                // Apply pressure to dependent stream
                if propagated_pressure > 0.3 { // Threshold for propagation
                    let dependent_state = self.get_or_create_stream_state(dependent);
                    let current_pressure = dependent_state.get_pressure_score();
                    let new_pressure = (current_pressure + propagated_pressure).min(1.0);
                    
                    let new_level = BackpressureLevel::from_utilization(new_pressure);
                    dependent_state.set_level(new_level);
                    
                    // Continue propagation
                    queue.push_back((dependent, depth + 1));
                }
            }
        }
        
        self.metrics.record_pressure_propagation();
        Ok(())
    }
    
    /// Get global pressure state
    pub fn get_global_pressure(&self) -> GlobalPressureState {
        self.global_pressure.load()
    }
    
    /// Update global pressure monitoring
    async fn update_global_pressure(&self) {
        let now = BackpressureState::current_time_ns();
        let last_update = self.last_global_update.load(Ordering::Relaxed);
        
        // Check if enough time has passed
        let interval_ns = self.config.pressure_monitoring_interval_ms * 1_000_000;
        if now - last_update < interval_ns {
            return;
        }
        
        self.last_global_update.store(now, Ordering::Relaxed);
        
        // Collect global statistics
        let mut total_pressure = 0.0;
        let mut active_streams = 0;
        let mut throttled_streams = 0;
        let mut total_credits = 0i64;
        
        for entry in self.stream_pressure.iter() {
            let state = entry.value();
            let pressure = state.get_pressure_score();
            let credits = state.get_credits();
            
            total_pressure += pressure;
            active_streams += 1;
            total_credits += credits as i64;
            
            if state.is_throttling() {
                throttled_streams += 1;
            }
        }
        
        let average_pressure = if active_streams > 0 {
            total_pressure / active_streams as f64
        } else {
            0.0
        };
        
        let overall_level = BackpressureLevel::from_utilization(average_pressure);
        
        // Update global state
        let global_state = GlobalPressureState {
            overall_level,
            active_streams,
            throttled_streams,
            total_credits,
            average_pressure_score: average_pressure,
            memory_pressure: self.estimate_memory_pressure(),
            cpu_pressure: self.estimate_cpu_pressure(),
            network_pressure: self.estimate_network_pressure(),
            last_updated: Instant::now(),
        };
        
        self.global_pressure.store(global_state);
        self.metrics.update_average_pressure(average_pressure);
        
        if overall_level >= BackpressureLevel::High {
            self.metrics.record_global_pressure_event();
        }
    }
    
    /// Add stream dependency for pressure propagation
    pub fn add_stream_dependency(&self, dependent: StreamId, dependency: StreamId, weight: f64) {
        self.propagation_graph.add_dependency(dependent, dependency, weight);
    }
    
    /// Remove stream dependency
    pub fn remove_stream_dependency(&self, dependent: StreamId, dependency: StreamId) {
        self.propagation_graph.remove_dependency(dependent, dependency);
    }
    
    /// Refresh credits for all streams
    pub async fn refresh_credits(&self) {
        let refresh_interval_ms = self.config.credit_refresh_interval_ms;
        
        for entry in self.stream_pressure.iter() {
            let state = entry.value();
            let current_level = state.get_level();
            
            let credits_to_add = self.flow_controller
                .calculate_credit_refresh(refresh_interval_ms, current_level);
            
            if credits_to_add > 0 {
                state.add_credits(credits_to_add);
                self.metrics.record_credit_refresh();
            }
        }
    }
    
    /// Get backpressure metrics
    pub fn get_metrics(&self) -> &BackpressureMetrics {
        &self.metrics
    }
    
    /// Check system health from backpressure perspective
    pub fn health_check(&self) -> bool {
        let global_state = self.get_global_pressure();
        let metrics = self.get_metrics();
        
        global_state.is_healthy() &&
        metrics.get_throttle_rate() < 0.1 && // Less than 10% throttling
        metrics.get_credit_utilization_rate() < 0.9 // Less than 90% credit utilization
    }
    
    /// Get stream pressure state
    pub fn get_stream_pressure(&self, stream_id: StreamId) -> Option<Arc<BackpressureState>> {
        self.stream_pressure.get(&stream_id).map(|entry| entry.clone())
    }
    
    /// Remove stream from backpressure management
    pub fn remove_stream(&self, stream_id: StreamId) -> BackpressureResult<()> {
        self.stream_pressure.remove(&stream_id);
        
        // Remove from propagation graph
        let dependents = self.propagation_graph.get_dependents(stream_id);
        let dependencies = self.propagation_graph.get_dependencies(stream_id);
        
        for dependent in dependents {
            self.propagation_graph.remove_dependency(dependent, stream_id);
        }
        
        for dependency in dependencies {
            self.propagation_graph.remove_dependency(stream_id, dependency);
        }
        
        Ok(())
    }
    
    // Private helper methods
    
    /// Get or create stream backpressure state
    fn get_or_create_stream_state(&self, stream_id: StreamId) -> Arc<BackpressureState> {
        self.stream_pressure
            .entry(stream_id)
            .or_insert_with(|| {
                let initial_credits = self.flow_controller.allocate_initial_credits(stream_id);
                Arc::new(BackpressureState::new(initial_credits))
            })
            .clone()
    }
    
    /// Estimate memory pressure (placeholder implementation)
    fn estimate_memory_pressure(&self) -> f64 {
        // In a real implementation, this would check system memory usage
        // For now, return a safe default
        0.3
    }
    
    /// Estimate CPU pressure (placeholder implementation)
    fn estimate_cpu_pressure(&self) -> f64 {
        // In a real implementation, this would check CPU usage
        // For now, return a safe default
        0.4
    }
    
    /// Estimate network pressure (placeholder implementation)
    fn estimate_network_pressure(&self) -> f64 {
        // In a real implementation, this would check network utilization
        // For now, return a safe default
        0.2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test;
    
    #[test]
    fn test_backpressure_level_from_utilization() {
        assert_eq!(BackpressureLevel::from_utilization(0.1), BackpressureLevel::None);
        assert_eq!(BackpressureLevel::from_utilization(0.3), BackpressureLevel::Low);
        assert_eq!(BackpressureLevel::from_utilization(0.6), BackpressureLevel::Medium);
        assert_eq!(BackpressureLevel::from_utilization(0.8), BackpressureLevel::High);
        assert_eq!(BackpressureLevel::from_utilization(0.95), BackpressureLevel::Critical);
    }
    
    #[test]
    fn test_backpressure_state_creation() {
        let state = BackpressureState::new(1000);
        
        assert_eq!(state.get_level(), BackpressureLevel::None);
        assert_eq!(state.get_credits(), 1000);
        assert!(!state.is_throttling());
    }
    
    #[test]
    fn test_credit_consumption() {
        let state = BackpressureState::new(1000);
        
        assert!(state.try_consume_credits(100));
        assert_eq!(state.get_credits(), 900);
        
        assert!(state.try_consume_credits(900));
        assert_eq!(state.get_credits(), 0);
        
        assert!(!state.try_consume_credits(1));
    }
    
    #[test]
    fn test_credit_addition() {
        let state = BackpressureState::new(500);
        
        let new_credits = state.add_credits(200);
        assert_eq!(new_credits, 700);
        assert_eq!(state.get_credits(), 700);
        
        // Test credit limit
        state.add_credits(20000); // Exceeds limit
        assert_eq!(state.get_credits(), MAX_CREDITS_PER_STREAM);
    }
    
    #[test]
    fn test_pressure_score_calculation() {
        let state = BackpressureState::new(1000);
        
        // Initial state should have low pressure
        assert!(state.get_pressure_score() < 0.5);
        
        // High queue depth should increase pressure
        state.update_queue_depth(500);
        state.update_processing_rate(10.0);
        
        let pressure_after_queue = state.get_pressure_score();
        assert!(pressure_after_queue > 0.1);
    }
    
    #[tokio::test]
    async fn test_backpressure_manager_creation() {
        let config = BackpressureConfig::default();
        let manager = BackpressureManager::new(config);
        
        assert!(manager.health_check());
        assert_eq!(manager.get_available_credits(123), 0); // Stream doesn't exist yet
    }
    
    #[tokio::test]
    async fn test_credit_allocation_and_consumption() {
        let config = BackpressureConfig::default();
        let manager = BackpressureManager::new(config);
        
        let stream_id = 123;
        
        // Allocate credits
        assert!(manager.allocate_credits(stream_id, 500).await.is_ok());
        
        // Check credits were allocated (plus initial default)
        let available = manager.get_available_credits(stream_id);
        assert_eq!(available, DEFAULT_INITIAL_CREDITS + 500);
        
        // Consume credits
        let consumed = manager.consume_credits(stream_id, 200).await.unwrap();
        assert!(consumed);
        
        // Check remaining credits
        let remaining = manager.get_available_credits(stream_id);
        assert_eq!(remaining, DEFAULT_INITIAL_CREDITS + 500 - 200);
        
        // Try to consume more than available
        let over_consumed = manager.consume_credits(stream_id, 10000).await.unwrap();
        assert!(!over_consumed);
    }
    
    #[tokio::test]
    async fn test_pressure_reporting() {
        let config = BackpressureConfig::default();
        let manager = BackpressureManager::new(config);
        
        let stream_id = 456;
        
        // Report pressure
        assert!(manager.report_pressure(stream_id, BackpressureLevel::High).await.is_ok());
        
        // Check pressure was recorded
        let state = manager.get_stream_pressure(stream_id).unwrap();
        assert_eq!(state.get_level(), BackpressureLevel::High);
        assert!(state.is_throttling());
    }
    
    #[tokio::test]
    async fn test_pressure_propagation() {
        let mut config = BackpressureConfig::default();
        config.enable_pressure_propagation = true;
        let manager = BackpressureManager::new(config);
        
        let producer = 100;
        let consumer = 200;
        
        // Add dependency: consumer depends on producer
        manager.add_stream_dependency(consumer, producer, 1.0);
        
        // Report high pressure on producer
        assert!(manager.report_pressure(producer, BackpressureLevel::Critical).await.is_ok());
        
        // Check if pressure propagated to consumer
        let consumer_state = manager.get_stream_pressure(consumer).unwrap();
        assert!(consumer_state.get_pressure_score() > 0.0);
    }
    
    #[test]
    fn test_propagation_graph() {
        let graph = PropagationGraph::new();
        
        let stream1 = 100;
        let stream2 = 200;
        let stream3 = 300;
        
        // Add dependencies
        graph.add_dependency(stream2, stream1, 0.8);
        graph.add_dependency(stream3, stream1, 0.6);
        graph.add_dependency(stream3, stream2, 0.5);
        
        // Test dependency queries
        let deps_of_2 = graph.get_dependencies(stream2);
        assert_eq!(deps_of_2, vec![stream1]);
        
        let deps_of_3 = graph.get_dependencies(stream3);
        assert!(deps_of_3.contains(&stream1));
        assert!(deps_of_3.contains(&stream2));
        
        let dependents_of_1 = graph.get_dependents(stream1);
        assert!(dependents_of_1.contains(&stream2));
        assert!(dependents_of_1.contains(&stream3));
        
        // Test weights
        assert_eq!(graph.get_propagation_weight(stream2, stream1), 0.8);
        assert_eq!(graph.get_propagation_weight(stream3, stream2), 0.5);
    }
    
    #[test]
    fn test_flow_controller() {
        let controller = FlowController::new();
        
        let initial_credits = controller.allocate_initial_credits(123);
        assert_eq!(initial_credits, DEFAULT_INITIAL_CREDITS);
        
        let refresh = controller.calculate_credit_refresh(1000, BackpressureLevel::Medium);
        assert!(refresh > 0);
        assert!(refresh < controller.credit_refresh_rate as i32); // Should be reduced due to pressure
    }
    
    #[test]
    fn test_global_pressure_state() {
        let state = GlobalPressureState::new();
        
        assert!(state.is_healthy());
        assert!(!state.requires_global_throttling());
        assert_eq!(state.overall_level, BackpressureLevel::None);
    }
    
    #[tokio::test]
    async fn test_metrics_tracking() {
        let config = BackpressureConfig::default();
        let manager = BackpressureManager::new(config);
        
        let stream_id = 789;
        
        // Perform operations that should be tracked
        assert!(manager.allocate_credits(stream_id, 100).await.is_ok());
        assert!(manager.consume_credits(stream_id, 50).await.is_ok());
        assert!(manager.report_pressure(stream_id, BackpressureLevel::Medium).await.is_ok());
        
        let metrics = manager.get_metrics();
        assert!(metrics.credit_allocations.load(Ordering::Relaxed) > 0);
        assert!(metrics.credit_consumptions.load(Ordering::Relaxed) > 0);
        assert!(metrics.get_credit_utilization_rate() > 0.0);
    }
    
    #[tokio::test]
    async fn test_credit_refresh() {
        let config = BackpressureConfig::default();
        let manager = BackpressureManager::new(config);
        
        let stream_id = 999;
        
        // Create stream and consume some credits
        assert!(manager.consume_credits(stream_id, 500).await.is_ok());
        let before_refresh = manager.get_available_credits(stream_id);
        
        // Refresh credits
        manager.refresh_credits().await;
        
        let after_refresh = manager.get_available_credits(stream_id);
        assert!(after_refresh > before_refresh);
    }
    
    #[tokio::test]
    async fn test_stream_removal() {
        let config = BackpressureConfig::default();
        let manager = BackpressureManager::new(config);
        
        let stream_id = 888;
        
        // Create stream state
        assert!(manager.allocate_credits(stream_id, 100).await.is_ok());
        assert!(manager.get_stream_pressure(stream_id).is_some());
        
        // Remove stream
        assert!(manager.remove_stream(stream_id).is_ok());
        assert!(manager.get_stream_pressure(stream_id).is_none());
    }
}