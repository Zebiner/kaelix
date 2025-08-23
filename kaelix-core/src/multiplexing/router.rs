//! Intelligent message routing system for optimal distribution.
//!
//! Provides sophisticated routing strategies and load balancing algorithms
//! for distributing messages across multiple streams and partitions with
//! maximum efficiency and reliability.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use crate::message::MessageId;
use crate::multiplexing::{Priority, StreamId};
use crate::{Error, Result};

/// Routing strategies for intelligent message distribution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoutingStrategy {
    /// Round-robin distribution
    RoundRobin,
    /// Load-based routing (least loaded stream)
    LeastLoaded,
    /// Hash-based consistent routing
    ConsistentHash,
    /// Geographic routing based on location
    Geographic,
    /// Priority-based routing
    PriorityBased,
    /// Adaptive routing that switches strategies based on conditions
    Adaptive,
    /// Custom routing with user-defined logic
    Custom,
}

impl Default for RoutingStrategy {
    fn default() -> Self {
        RoutingStrategy::LeastLoaded
    }
}

/// Routing configuration parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingConfig {
    /// Primary routing strategy
    pub strategy: RoutingStrategy,
    /// Backup routing strategy if primary fails
    pub fallback_strategy: Option<RoutingStrategy>,
    /// Enable adaptive routing based on load
    pub adaptive_routing: bool,
    /// Load balancing window size for metrics
    pub load_window_size: usize,
    /// Maximum routing attempts before failure
    pub max_routing_attempts: u32,
    /// Routing timeout per attempt
    pub routing_timeout: Duration,
    /// Enable geographic awareness
    pub geographic_routing: bool,
    /// Priority queue configuration
    pub priority_config: PriorityConfig,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            strategy: RoutingStrategy::default(),
            fallback_strategy: Some(RoutingStrategy::RoundRobin),
            adaptive_routing: true,
            load_window_size: 1000,
            max_routing_attempts: 3,
            routing_timeout: Duration::from_millis(100),
            geographic_routing: false,
            priority_config: PriorityConfig::default(),
        }
    }
}

/// Priority configuration for routing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityConfig {
    /// Enable priority-based routing
    pub enabled: bool,
    /// Number of priority levels (0 = highest)
    pub levels: u8,
    /// Bandwidth allocation per priority level (percentage)
    pub bandwidth_allocation: Vec<f32>,
    /// Enable priority preemption
    pub preemption_enabled: bool,
}

impl Default for PriorityConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            levels: 4,
            bandwidth_allocation: vec![40.0, 30.0, 20.0, 10.0], // High to low priority
            preemption_enabled: false,
        }
    }
}

/// Stream load metrics for routing decisions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamLoadMetrics {
    /// Stream identifier
    pub stream_id: StreamId,
    /// Current load (messages per second)
    pub current_load: f64,
    /// Average load over the window
    pub average_load: f64,
    /// Peak load in the window
    pub peak_load: f64,
    /// Queue depth
    pub queue_depth: usize,
    /// Processing latency (ms)
    pub latency_ms: f64,
    /// Error rate (percentage)
    pub error_rate: f64,
    /// Available capacity (percentage)
    pub available_capacity: f64,
    /// Last update timestamp
    pub last_update: Instant,
    /// Geographic region (optional)
    pub region: Option<String>,
}

impl StreamLoadMetrics {
    pub fn new(stream_id: StreamId) -> Self {
        Self {
            stream_id,
            current_load: 0.0,
            average_load: 0.0,
            peak_load: 0.0,
            queue_depth: 0,
            latency_ms: 0.0,
            error_rate: 0.0,
            available_capacity: 100.0,
            last_update: Instant::now(),
            region: None,
        }
    }

    /// Calculate overall health score (0.0 = unhealthy, 1.0 = perfect)
    pub fn health_score(&self) -> f64 {
        let capacity_score = self.available_capacity / 100.0;
        let latency_score = (200.0 - self.latency_ms.min(200.0)) / 200.0;
        let error_score = (100.0 - self.error_rate.min(100.0)) / 100.0;

        (capacity_score + latency_score + error_score) / 3.0
    }

    /// Check if stream is overloaded
    pub fn is_overloaded(&self) -> bool {
        self.available_capacity < 10.0 || self.error_rate > 5.0 || self.latency_ms > 1000.0
    }
}

/// Message metadata for routing decisions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageMetadata {
    /// Message identifier
    pub message_id: MessageId,
    /// Message priority
    pub priority: Priority,
    /// Target stream hint
    pub target_stream: Option<StreamId>,
    /// Geographic origin
    pub origin_region: Option<String>,
    /// Message size in bytes
    pub size_bytes: usize,
    /// Processing deadline
    pub deadline: Option<Instant>,
    /// Routing tags for custom logic
    pub tags: HashMap<String, String>,
}

/// Routing result containing target stream and metadata
#[derive(Debug, Clone)]
pub struct RoutingResult {
    /// Target stream for message
    pub target_stream: StreamId,
    /// Routing strategy used
    pub strategy_used: RoutingStrategy,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,
    /// Estimated processing latency
    pub estimated_latency: Duration,
    /// Alternative streams (for failover)
    pub alternatives: Vec<StreamId>,
}

/// High-performance message router with intelligent distribution
pub struct IntelligentRouter {
    /// Router configuration
    config: RoutingConfig,
    /// Available streams with their load metrics
    streams: Arc<RwLock<HashMap<StreamId, StreamLoadMetrics>>>,
    /// Round-robin counter
    round_robin_counter: AtomicU64,
    /// Consistent hash ring
    hash_ring: Arc<RwLock<ConsistentHashRing>>,
    /// Priority queues for each priority level
    priority_queues: Arc<RwLock<BTreeMap<Priority, VecDeque<MessageMetadata>>>>,
    /// Routing statistics
    stats: Arc<RwLock<RoutingStats>>,
    /// Geographic region mapping
    region_mapping: Arc<RwLock<HashMap<String, Vec<StreamId>>>>,
}

impl IntelligentRouter {
    /// Create a new intelligent router
    pub fn new(config: RoutingConfig) -> Self {
        Self {
            config,
            streams: Arc::new(RwLock::new(HashMap::new())),
            round_robin_counter: AtomicU64::new(0),
            hash_ring: Arc::new(RwLock::new(ConsistentHashRing::new())),
            priority_queues: Arc::new(RwLock::new(BTreeMap::new())),
            stats: Arc::new(RwLock::new(RoutingStats::new())),
            region_mapping: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a stream to the routing table
    pub async fn add_stream(&self, stream_id: StreamId, region: Option<String>) -> Result<()> {
        let mut metrics = StreamLoadMetrics::new(stream_id);
        metrics.region = region.clone();

        {
            let mut streams = self.streams.write().await;
            streams.insert(stream_id, metrics);
        }

        // Add to hash ring
        {
            let mut hash_ring = self.hash_ring.write().await;
            hash_ring.add_node(stream_id);
        }

        // Update region mapping
        if let Some(region) = region {
            let mut region_mapping = self.region_mapping.write().await;
            region_mapping.entry(region).or_insert_with(Vec::new).push(stream_id);
        }

        tracing::info!("Added stream {} to router", stream_id);
        Ok(())
    }

    /// Remove a stream from the routing table
    pub async fn remove_stream(&self, stream_id: StreamId) -> Result<()> {
        // Remove from streams
        let region = {
            let mut streams = self.streams.write().await;
            streams.remove(&stream_id).map(|m| m.region)
        };

        // Remove from hash ring
        {
            let mut hash_ring = self.hash_ring.write().await;
            hash_ring.remove_node(stream_id);
        }

        // Update region mapping
        if let Some(Some(region)) = region {
            let mut region_mapping = self.region_mapping.write().await;
            if let Some(streams) = region_mapping.get_mut(&region) {
                streams.retain(|&id| id != stream_id);
                if streams.is_empty() {
                    region_mapping.remove(&region);
                }
            }
        }

        tracing::info!("Removed stream {} from router", stream_id);
        Ok(())
    }

    /// Update stream load metrics
    pub async fn update_stream_metrics(&self, metrics: StreamLoadMetrics) -> Result<()> {
        let mut streams = self.streams.write().await;
        streams.insert(metrics.stream_id, metrics);
        Ok(())
    }

    /// Route a message to the optimal stream
    pub async fn route_message(&self, metadata: MessageMetadata) -> Result<RoutingResult> {
        let mut stats = self.stats.write().await;
        stats.total_requests += 1;

        let start_time = Instant::now();

        // Try primary strategy first
        let result = match self.config.strategy {
            RoutingStrategy::RoundRobin => self.route_round_robin().await,
            RoutingStrategy::LeastLoaded => self.route_least_loaded().await,
            RoutingStrategy::ConsistentHash => self.route_consistent_hash(&metadata).await,
            RoutingStrategy::Geographic => self.route_geographic(&metadata).await,
            RoutingStrategy::PriorityBased => self.route_priority_based(&metadata).await,
            RoutingStrategy::Adaptive => self.route_adaptive(&metadata).await,
            RoutingStrategy::Custom => self.route_custom(&metadata).await,
        };

        let routing_result = match result {
            Ok(result) => result,
            Err(_) if self.config.fallback_strategy.is_some() => {
                // Try fallback strategy
                stats.fallback_used += 1;
                let fallback = self.config.fallback_strategy.unwrap();
                match fallback {
                    RoutingStrategy::RoundRobin => self.route_round_robin().await?,
                    RoutingStrategy::LeastLoaded => self.route_least_loaded().await?,
                    _ => return Err(Error::Runtime("No viable routing strategy".to_string())),
                }
            },
            Err(e) => return Err(e),
        };

        let routing_latency = start_time.elapsed();
        stats.total_latency += routing_latency;
        stats.successful_routes += 1;

        Ok(routing_result)
    }

    /// Round-robin routing
    async fn route_round_robin(&self) -> Result<RoutingResult> {
        let streams = self.streams.read().await;
        let stream_ids: Vec<StreamId> = streams.keys().copied().collect();

        if stream_ids.is_empty() {
            return Err(Error::Runtime("No streams available for routing".to_string()));
        }

        let index = self.round_robin_counter.fetch_add(1, Ordering::Relaxed) as usize;
        let target_stream = stream_ids[index % stream_ids.len()];

        Ok(RoutingResult {
            target_stream,
            strategy_used: RoutingStrategy::RoundRobin,
            confidence: 0.7,
            estimated_latency: Duration::from_millis(50),
            alternatives: stream_ids.into_iter().filter(|&s| s != target_stream).collect(),
        })
    }

    /// Least-loaded routing
    async fn route_least_loaded(&self) -> Result<RoutingResult> {
        let streams = self.streams.read().await;

        if streams.is_empty() {
            return Err(Error::Runtime("No streams available for routing".to_string()));
        }

        // Find stream with highest available capacity and lowest load
        let (target_stream, metrics) = streams
            .iter()
            .filter(|(_, m)| !m.is_overloaded())
            .max_by(|(_, a), (_, b)| {
                let score_a = a.health_score();
                let score_b = b.health_score();
                score_a.partial_cmp(&score_b).unwrap_or(std::cmp::Ordering::Equal)
            })
            .ok_or_else(|| Error::Runtime("No healthy streams available".to_string()))?;

        Ok(RoutingResult {
            target_stream: *target_stream,
            strategy_used: RoutingStrategy::LeastLoaded,
            confidence: metrics.health_score(),
            estimated_latency: Duration::from_millis(metrics.latency_ms as u64),
            alternatives: streams.keys().copied().filter(|&s| s != *target_stream).collect(),
        })
    }

    /// Consistent hash routing
    async fn route_consistent_hash(&self, metadata: &MessageMetadata) -> Result<RoutingResult> {
        let hash_ring = self.hash_ring.read().await;
        let target_stream = hash_ring
            .get_node(&metadata.message_id.to_string())
            .ok_or_else(|| Error::Runtime("No nodes in hash ring".to_string()))?;

        Ok(RoutingResult {
            target_stream,
            strategy_used: RoutingStrategy::ConsistentHash,
            confidence: 0.9, // Consistent hashing is very reliable
            estimated_latency: Duration::from_millis(30),
            alternatives: vec![], // Consistent hashing doesn't provide alternatives
        })
    }

    /// Geographic routing
    async fn route_geographic(&self, metadata: &MessageMetadata) -> Result<RoutingResult> {
        if let Some(ref origin_region) = metadata.origin_region {
            let region_mapping = self.region_mapping.read().await;
            if let Some(region_streams) = region_mapping.get(origin_region) {
                if !region_streams.is_empty() {
                    // Use least-loaded within the region
                    let streams = self.streams.read().await;
                    let (target_stream, metrics) = region_streams
                        .iter()
                        .filter_map(|&stream_id| streams.get(&stream_id).map(|m| (stream_id, m)))
                        .filter(|(_, m)| !m.is_overloaded())
                        .max_by(|(_, a), (_, b)| {
                            a.health_score()
                                .partial_cmp(&b.health_score())
                                .unwrap_or(std::cmp::Ordering::Equal)
                        })
                        .ok_or_else(|| {
                            Error::Runtime("No healthy streams in region".to_string())
                        })?;

                    return Ok(RoutingResult {
                        target_stream,
                        strategy_used: RoutingStrategy::Geographic,
                        confidence: metrics.health_score(),
                        estimated_latency: Duration::from_millis(metrics.latency_ms as u64 / 2), // Local region
                        alternatives: region_streams
                            .iter()
                            .copied()
                            .filter(|&s| s != target_stream)
                            .collect(),
                    });
                }
            }
        }

        // Fallback to least-loaded if no geographic info
        self.route_least_loaded().await
    }

    /// Priority-based routing
    async fn route_priority_based(&self, metadata: &MessageMetadata) -> Result<RoutingResult> {
        // For now, use least-loaded but with priority awareness
        // In a full implementation, this would maintain separate queues per priority
        let result = self.route_least_loaded().await?;
        Ok(RoutingResult { strategy_used: RoutingStrategy::PriorityBased, ..result })
    }

    /// Adaptive routing that switches strategies based on conditions
    async fn route_adaptive(&self, metadata: &MessageMetadata) -> Result<RoutingResult> {
        let streams = self.streams.read().await;
        let avg_load: f64 =
            streams.values().map(|m| m.current_load).sum::<f64>() / streams.len() as f64;

        // Use different strategies based on system load
        if avg_load > 1000.0 {
            // High load - use least loaded
            drop(streams);
            self.route_least_loaded().await
        } else if metadata.origin_region.is_some() {
            // Normal load with geographic info - use geographic routing
            drop(streams);
            self.route_geographic(metadata).await
        } else {
            // Normal load - use consistent hashing for better distribution
            drop(streams);
            self.route_consistent_hash(metadata).await
        }
    }

    /// Custom routing (placeholder for user-defined logic)
    async fn route_custom(&self, _metadata: &MessageMetadata) -> Result<RoutingResult> {
        // Fallback to least-loaded for now
        self.route_least_loaded().await
    }

    /// Get routing statistics
    pub async fn get_stats(&self) -> RoutingStats {
        self.stats.read().await.clone()
    }

    /// Get current stream metrics
    pub async fn get_stream_metrics(&self) -> HashMap<StreamId, StreamLoadMetrics> {
        self.streams.read().await.clone()
    }
}

/// Consistent hash ring implementation
#[derive(Debug)]
struct ConsistentHashRing {
    ring: BTreeMap<u64, StreamId>,
    nodes: HashMap<StreamId, Vec<u64>>,
    virtual_nodes: usize,
}

impl ConsistentHashRing {
    fn new() -> Self {
        Self {
            ring: BTreeMap::new(),
            nodes: HashMap::new(),
            virtual_nodes: 150, // Number of virtual nodes per physical node
        }
    }

    fn add_node(&mut self, node: StreamId) {
        let mut hashes = Vec::new();
        for i in 0..self.virtual_nodes {
            let hash = self.hash(&format!("{}:{}", node, i));
            self.ring.insert(hash, node);
            hashes.push(hash);
        }
        self.nodes.insert(node, hashes);
    }

    fn remove_node(&mut self, node: StreamId) {
        if let Some(hashes) = self.nodes.remove(&node) {
            for hash in hashes {
                self.ring.remove(&hash);
            }
        }
    }

    fn get_node(&self, key: &str) -> Option<StreamId> {
        if self.ring.is_empty() {
            return None;
        }

        let hash = self.hash(key);

        // Find the first node with hash >= key hash
        if let Some((_, &node)) = self.ring.range(hash..).next() {
            Some(node)
        } else {
            // Wrap around to the first node
            self.ring.values().next().copied()
        }
    }

    fn hash(&self, key: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish()
    }
}

/// Routing statistics for monitoring and optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingStats {
    /// Total routing requests
    pub total_requests: u64,
    /// Successful routes
    pub successful_routes: u64,
    /// Failed routes
    pub failed_routes: u64,
    /// Times fallback strategy was used
    pub fallback_used: u64,
    /// Total routing latency
    pub total_latency: Duration,
    /// Average routing latency
    pub average_latency: Duration,
    /// Strategy usage counts
    pub strategy_usage: HashMap<String, u64>,
}

impl RoutingStats {
    fn new() -> Self {
        Self {
            total_requests: 0,
            successful_routes: 0,
            failed_routes: 0,
            fallback_used: 0,
            total_latency: Duration::ZERO,
            average_latency: Duration::ZERO,
            strategy_usage: HashMap::new(),
        }
    }

    /// Calculate average latency
    pub fn calculate_average_latency(&mut self) {
        if self.successful_routes > 0 {
            self.average_latency = self.total_latency / self.successful_routes as u32;
        }
    }

    /// Get success rate as percentage
    pub fn success_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            (self.successful_routes as f64 / self.total_requests as f64) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::MessageId;

    #[tokio::test]
    async fn test_router_creation() {
        let config = RoutingConfig::default();
        let router = IntelligentRouter::new(config);

        let stats = router.get_stats().await;
        assert_eq!(stats.total_requests, 0);
    }

    #[tokio::test]
    async fn test_add_remove_stream() {
        let config = RoutingConfig::default();
        let router = IntelligentRouter::new(config);

        let stream_id = StreamId(1);
        router.add_stream(stream_id, None).await.unwrap();

        let metrics = router.get_stream_metrics().await;
        assert!(metrics.contains_key(&stream_id));

        router.remove_stream(stream_id).await.unwrap();
        let metrics = router.get_stream_metrics().await;
        assert!(!metrics.contains_key(&stream_id));
    }

    #[tokio::test]
    async fn test_round_robin_routing() {
        let config = RoutingConfig { strategy: RoutingStrategy::RoundRobin, ..Default::default() };
        let router = IntelligentRouter::new(config);

        // Add multiple streams
        for i in 1..=3 {
            router.add_stream(StreamId(i), None).await.unwrap();
        }

        let metadata = MessageMetadata {
            message_id: MessageId::new(),
            priority: Priority::Normal,
            target_stream: None,
            origin_region: None,
            size_bytes: 1024,
            deadline: None,
            tags: HashMap::new(),
        };

        // Route multiple messages and verify distribution
        let mut targets = Vec::new();
        for _ in 0..6 {
            let result = router.route_message(metadata.clone()).await.unwrap();
            targets.push(result.target_stream);
            assert_eq!(result.strategy_used, RoutingStrategy::RoundRobin);
        }

        // Should see all streams used in round-robin fashion
        let unique_targets: std::collections::HashSet<_> = targets.into_iter().collect();
        assert_eq!(unique_targets.len(), 3);
    }

    #[test]
    fn test_consistent_hash_ring() {
        let mut ring = ConsistentHashRing::new();

        ring.add_node(StreamId(1));
        ring.add_node(StreamId(2));
        ring.add_node(StreamId(3));

        // Test consistent mapping
        let key = "test_key";
        let node1 = ring.get_node(key).unwrap();
        let node2 = ring.get_node(key).unwrap();
        assert_eq!(node1, node2);

        // Test removal
        ring.remove_node(StreamId(2));
        let node3 = ring.get_node(key).unwrap();
        assert_ne!(node3, StreamId(2));
    }

    #[test]
    fn test_stream_load_metrics() {
        let mut metrics = StreamLoadMetrics::new(StreamId(1));

        // Test healthy stream
        assert!(metrics.health_score() > 0.8);
        assert!(!metrics.is_overloaded());

        // Test overloaded stream
        metrics.available_capacity = 5.0;
        metrics.error_rate = 10.0;
        metrics.latency_ms = 2000.0;

        assert!(metrics.health_score() < 0.5);
        assert!(metrics.is_overloaded());
    }
}
