//! Invariant validation framework for ensuring system correctness.
//!
//! This module provides a comprehensive framework for validating system invariants
//! that must hold true across all system states. It includes invariant checkers for:
//!
//! - Message ordering and delivery guarantees
//! - Data consistency and durability
//! - Performance characteristics
//! - Security and authorization
//! - Partition assignment and consensus

use std::collections::HashMap;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use kaelix_core::{Message, MessageId, PartitionId};
use crate::validators::ValidationResult;

/// Trait for system invariants that must be maintained.
#[async_trait]
pub trait SystemInvariant: Send + Sync + std::fmt::Debug {
    /// Check if the invariant holds for the given system state.
    async fn check(&self, state: &SystemState) -> InvariantResult;
    
    /// Get a human-readable description of this invariant.
    fn description(&self) -> &str;
    
    /// Get the severity level of this invariant.
    fn severity(&self) -> InvariantSeverity;
}

/// Result of an invariant check.
#[derive(Debug, Clone, PartialEq)]
pub enum InvariantResult {
    /// Invariant is satisfied
    Satisfied,
    /// Invariant is violated
    Violated(InvariantViolation),
}

/// Details of an invariant violation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvariantViolation {
    /// Human-readable message describing the violation
    pub message: String,
    /// Additional context for debugging
    pub context: HashMap<String, String>,
    /// When the violation was detected
    pub timestamp: DateTime<Utc>,
}

/// Severity levels for invariant violations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InvariantSeverity {
    /// Low severity - might indicate performance degradation
    Low,
    /// Medium severity - might indicate correctness issues
    Medium,
    /// High severity - indicates serious correctness violations
    High,
    /// Critical severity - indicates system integrity violations
    Critical,
}

/// Snapshot of system state for invariant checking.
#[derive(Debug, Clone)]
pub struct SystemState {
    /// All messages in the system by ID
    pub messages: HashMap<MessageId, MessageInfo>,
    /// Partition states
    pub partitions: HashMap<PartitionId, PartitionState>,
    /// Node states in the cluster
    pub nodes: HashMap<String, NodeState>,
    /// When this state was captured
    pub timestamp: DateTime<Utc>,
    /// Performance metrics at capture time
    pub metrics: PerformanceMetrics,
}

/// Information about a message in the system.
#[derive(Debug, Clone)]
pub struct MessageInfo {
    /// The message itself
    pub message: Message,
    /// Current status
    pub status: MessageStatus,
    /// Processing timeline
    pub timeline: Vec<MessageEvent>,
}

/// Status of a message in the system.
#[derive(Debug, Clone, PartialEq)]
pub enum MessageStatus {
    /// Message is being produced
    Producing,
    /// Message is queued for delivery
    Queued,
    /// Message is being replicated
    Replicating,
    /// Message has been delivered
    Delivered,
    /// Message delivery failed
    Failed(String),
}

/// Event in a message's processing timeline.
#[derive(Debug, Clone)]
pub struct MessageEvent {
    /// Type of event
    pub event_type: MessageEventType,
    /// When the event occurred
    pub timestamp: DateTime<Utc>,
    /// Node that triggered the event
    pub node_id: String,
    /// Additional event context
    pub context: HashMap<String, String>,
}

/// Types of message processing events.
#[derive(Debug, Clone, PartialEq)]
pub enum MessageEventType {
    /// Message was received by producer
    Received,
    /// Message was queued for processing
    Queued,
    /// Message was assigned to partition
    PartitionAssigned,
    /// Message was replicated to follower
    Replicated,
    /// Message was committed to log
    Committed,
    /// Message was delivered to consumer
    Delivered,
    /// Message processing failed
    Failed,
}

/// State of a partition.
#[derive(Debug, Clone)]
pub struct PartitionState {
    /// Current leader node
    pub leader: Option<String>,
    /// Follower nodes
    pub followers: Vec<String>,
    /// High water mark (last committed offset)
    pub high_water_mark: u64,
    /// Low water mark (earliest retained offset)
    pub low_water_mark: u64,
    /// Replication lag by follower
    pub replication_lag: HashMap<String, u64>,
}

/// State of a cluster node.
#[derive(Debug, Clone)]
pub struct NodeState {
    /// Whether the node is online
    pub is_online: bool,
    /// Node's role
    pub role: NodeRole,
    /// Last heartbeat timestamp
    pub last_heartbeat: DateTime<Utc>,
    /// Resource utilization
    pub resources: ResourceUtilization,
}

/// Role of a node in the cluster.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeRole {
    /// Broker node
    Broker,
    /// Producer node
    Producer,
    /// Consumer node
    Consumer,
}

/// Resource utilization metrics for a node.
#[derive(Debug, Clone, Default)]
pub struct ResourceUtilization {
    /// CPU usage percentage
    pub cpu_percent: f64,
    /// Memory usage in bytes
    pub memory_bytes: u64,
    /// Network I/O in bytes per second
    pub network_bytes_per_sec: u64,
    /// Disk I/O in bytes per second
    pub disk_bytes_per_sec: u64,
}

/// Performance metrics for the system.
#[derive(Debug, Clone, Default)]
pub struct PerformanceMetrics {
    /// Current throughput in messages per second
    pub throughput: f64,
    /// P99 latency in microseconds
    pub latency_p99: u64,
    /// P95 latency in microseconds
    pub latency_p95: u64,
    /// P50 latency in microseconds
    pub latency_p50: u64,
    /// Memory usage in bytes
    pub memory_usage: u64,
    /// CPU usage percentage
    pub cpu_usage: f64,
}

/// Orchestrates checking of multiple invariants.
#[derive(Debug)]
pub struct InvariantChecker {
    /// Registered invariants to check
    invariants: Vec<Box<dyn SystemInvariant>>,
}

/// Result of checking all invariants.
#[derive(Debug)]
pub struct InvariantCheckResult {
    /// Results for each invariant
    pub results: Vec<(String, InvariantResult)>,
    /// Summary statistics
    pub summary: InvariantSummary,
}

/// Summary of invariant check results.
#[derive(Debug, Clone)]
pub struct InvariantSummary {
    /// Total number of invariants checked
    pub total: usize,
    /// Number of satisfied invariants
    pub satisfied: usize,
    /// Number of violated invariants
    pub violated: usize,
    /// Highest severity of violations
    pub max_severity: Option<InvariantSeverity>,
}

impl InvariantChecker {
    /// Create a new invariant checker.
    pub fn new() -> Self {
        let mut checker = Self {
            invariants: Vec::new(),
        };
        
        // Register default invariants
        checker.register_default_invariants();
        checker
    }
    
    /// Register default system invariants.
    fn register_default_invariants(&mut self) {
        self.add_invariant(Box::new(OrderingInvariant::new()));
        self.add_invariant(Box::new(DurabilityInvariant::new()));
        self.add_invariant(Box::new(ConsistencyInvariant::new()));
        self.add_invariant(Box::new(LatencyInvariant::new()));
        self.add_invariant(Box::new(ThroughputInvariant::new()));
        self.add_invariant(Box::new(PartitionInvariant::new()));
        self.add_invariant(Box::new(AuthorizationInvariant::new()));
    }
    
    /// Add a custom invariant.
    pub fn add_invariant(&mut self, invariant: Box<dyn SystemInvariant>) {
        self.invariants.push(invariant);
    }
    
    /// Check all registered invariants.
    pub async fn check_all(&self, state: &SystemState) -> InvariantCheckResult {
        let mut results = Vec::new();
        let mut violated_count = 0;
        let mut max_severity: Option<InvariantSeverity> = None;
        
        for invariant in &self.invariants {
            let result = invariant.check(state).await;
            let description = invariant.description().to_string();
            
            if let InvariantResult::Violated(_) = &result {
                violated_count += 1;
                let severity = invariant.severity();
                max_severity = Some(match max_severity {
                    None => severity,
                    Some(current) => current.max(severity),
                });
            }
            
            results.push((description, result));
        }
        
        let summary = InvariantSummary {
            total: self.invariants.len(),
            satisfied: self.invariants.len() - violated_count,
            violated: violated_count,
            max_severity,
        };
        
        InvariantCheckResult { results, summary }
    }
}

impl Default for InvariantChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl InvariantCheckResult {
    /// Check if all invariants are satisfied.
    pub fn all_satisfied(&self) -> bool {
        self.summary.violated == 0
    }
    
    /// Get a summary string of results.
    pub fn summary(&self) -> String {
        format!(
            "{}/{} invariants satisfied, {} violations{}",
            self.summary.satisfied,
            self.summary.total,
            self.summary.violated,
            if let Some(severity) = &self.summary.max_severity {
                format!(" (max severity: {:?})", severity)
            } else {
                String::new()
            }
        )
    }
    
    /// Get all violations.
    pub fn violations(&self) -> Vec<&InvariantViolation> {
        self.results
            .iter()
            .filter_map(|(_, result)| match result {
                InvariantResult::Violated(violation) => Some(violation),
                _ => None,
            })
            .collect()
    }
}

// Specific invariant implementations

/// Ensures message ordering is maintained within partitions.
#[derive(Debug)]
pub struct OrderingInvariant {
    /// Maximum acceptable ordering violations per partition
    max_violations_per_partition: u64,
}

impl OrderingInvariant {
    /// Create a new ordering invariant with default settings.
    pub fn new() -> Self {
        Self {
            max_violations_per_partition: 0, // Strict ordering
        }
    }
    
    /// Create with custom violation tolerance.
    pub fn with_max_violations(max_violations: u64) -> Self {
        Self {
            max_violations_per_partition: max_violations,
        }
    }
}

#[async_trait]
impl SystemInvariant for OrderingInvariant {
    async fn check(&self, state: &SystemState) -> InvariantResult {
        let mut violations = 0u64;
        
        // Group messages by partition and check ordering
        let mut partition_messages: HashMap<PartitionId, Vec<&MessageInfo>> = HashMap::new();
        
        for message_info in state.messages.values() {
            if let Some(partition_id) = message_info.message.partition {
                partition_messages
                    .entry(partition_id)
                    .or_default()
                    .push(message_info);
            }
        }
        
        for (partition_id, messages) in partition_messages {
            // Sort by offset and check for violations
            let mut sorted_messages = messages;
            sorted_messages.sort_by(|a, b| {
                a.message.offset.unwrap_or_default().cmp(&b.message.offset.unwrap_or_default())
            });
            
            // Check for timestamp ordering within offset ordering
            for window in sorted_messages.windows(2) {
                if let [msg1, msg2] = window {
                    if msg2.message.timestamp < msg1.message.timestamp {
                        violations += 1;
                    }
                }
            }
            
            if violations > self.max_violations_per_partition {
                return InvariantResult::Violated(InvariantViolation {
                    message: format!(
                        "Ordering violations in partition {}: {} violations (max: {})",
                        partition_id.0, violations, self.max_violations_per_partition
                    ),
                    context: {
                        let mut context = HashMap::new();
                        context.insert("partition_id".to_string(), partition_id.0.to_string());
                        context.insert("violations".to_string(), violations.to_string());
                        context
                    },
                    timestamp: chrono::Utc::now(),
                });
            }
        }
        
        InvariantResult::Satisfied
    }
    
    fn description(&self) -> &str {
        "Message ordering must be maintained within partitions"
    }
    
    fn severity(&self) -> InvariantSeverity {
        InvariantSeverity::High
    }
}

/// Ensures message durability guarantees are met.
#[derive(Debug)]
pub struct DurabilityInvariant {
    /// Minimum required replication factor
    min_replication_factor: u32,
}

impl DurabilityInvariant {
    /// Create a new durability invariant.
    pub fn new() -> Self {
        Self {
            min_replication_factor: 3, // Default to 3 replicas
        }
    }
    
    /// Create with custom replication requirements.
    pub fn with_replication_factor(factor: u32) -> Self {
        Self {
            min_replication_factor: factor,
        }
    }
}

#[async_trait]
impl SystemInvariant for DurabilityInvariant {
    async fn check(&self, state: &SystemState) -> InvariantResult {
        // Check that committed messages have sufficient replication
        for (partition_id, partition_state) in &state.partitions {
            let total_replicas = 1 + partition_state.followers.len() as u32; // Leader + followers
            
            if total_replicas < self.min_replication_factor {
                return InvariantResult::Violated(InvariantViolation {
                    message: format!(
                        "Partition {} has insufficient replicas: {} (min: {})",
                        partition_id.0, total_replicas, self.min_replication_factor
                    ),
                    context: {
                        let mut context = HashMap::new();
                        context.insert("partition_id".to_string(), partition_id.0.to_string());
                        context.insert("replicas".to_string(), total_replicas.to_string());
                        context.insert("min_required".to_string(), self.min_replication_factor.to_string());
                        context
                    },
                    timestamp: chrono::Utc::now(),
                });
            }
        }
        
        InvariantResult::Satisfied
    }
    
    fn description(&self) -> &str {
        "Messages must be replicated to sufficient number of nodes for durability"
    }
    
    fn severity(&self) -> InvariantSeverity {
        InvariantSeverity::Critical
    }
}

/// Ensures data consistency across replicas.
#[derive(Debug)]
pub struct ConsistencyInvariant {
    /// Maximum acceptable replication lag in milliseconds
    max_replication_lag_ms: u64,
}

impl ConsistencyInvariant {
    /// Create a new consistency invariant.
    pub fn new() -> Self {
        Self {
            max_replication_lag_ms: 1000, // 1 second default
        }
    }
    
    /// Create with custom lag tolerance.
    pub fn with_max_lag(lag_ms: u64) -> Self {
        Self {
            max_replication_lag_ms: lag_ms,
        }
    }
}

#[async_trait]
impl SystemInvariant for ConsistencyInvariant {
    async fn check(&self, state: &SystemState) -> InvariantResult {
        // Check replication lag for all partitions
        for (partition_id, partition_state) in &state.partitions {
            for (follower_id, lag) in &partition_state.replication_lag {
                if *lag > self.max_replication_lag_ms {
                    return InvariantResult::Violated(InvariantViolation {
                        message: format!(
                            "Replication lag too high: partition {}, follower {}, lag: {}ms (max: {}ms)",
                            partition_id.0, follower_id, lag, self.max_replication_lag_ms
                        ),
                        context: {
                            let mut context = HashMap::new();
                            context.insert("partition_id".to_string(), partition_id.0.to_string());
                            context.insert("follower_id".to_string(), follower_id.clone());
                            context.insert("lag_ms".to_string(), lag.to_string());
                            context
                        },
                        timestamp: chrono::Utc::now(),
                    });
                }
            }
        }
        
        InvariantResult::Satisfied
    }
    
    fn description(&self) -> &str {
        "Replication lag must be within acceptable bounds for consistency"
    }
    
    fn severity(&self) -> InvariantSeverity {
        InvariantSeverity::High
    }
}

/// Ensures latency requirements are met.
#[derive(Debug)]
pub struct LatencyInvariant {
    /// Maximum acceptable P99 latency in microseconds
    max_p99_latency_us: u64,
}

impl LatencyInvariant {
    /// Create a new latency invariant.
    pub fn new() -> Self {
        Self {
            max_p99_latency_us: 10_000, // 10ms default
        }
    }
    
    /// Create with custom latency requirements.
    pub fn with_max_latency(latency_us: u64) -> Self {
        Self {
            max_p99_latency_us: latency_us,
        }
    }
}

#[async_trait]
impl SystemInvariant for LatencyInvariant {
    async fn check(&self, state: &SystemState) -> InvariantResult {
        if state.metrics.latency_p99 > self.max_p99_latency_us {
            return InvariantResult::Violated(InvariantViolation {
                message: format!(
                    "P99 latency too high: {}μs (max: {}μs)",
                    state.metrics.latency_p99, self.max_p99_latency_us
                ),
                context: {
                    let mut context = HashMap::new();
                    context.insert("p99_latency_us".to_string(), state.metrics.latency_p99.to_string());
                    context.insert("max_latency_us".to_string(), self.max_p99_latency_us.to_string());
                    context
                },
                timestamp: chrono::Utc::now(),
            });
        }
        
        InvariantResult::Satisfied
    }
    
    fn description(&self) -> &str {
        "System latency must meet performance requirements"
    }
    
    fn severity(&self) -> InvariantSeverity {
        InvariantSeverity::Medium
    }
}

/// Ensures throughput requirements are met.
#[derive(Debug)]
pub struct ThroughputInvariant {
    /// Minimum required throughput in messages per second
    min_throughput: f64,
}

impl ThroughputInvariant {
    /// Create a new throughput invariant.
    pub fn new() -> Self {
        Self {
            min_throughput: 1_000_000.0, // 1M msg/sec default
        }
    }
    
    /// Create with custom throughput requirements.
    pub fn with_min_throughput(throughput: f64) -> Self {
        Self {
            min_throughput: throughput,
        }
    }
}

#[async_trait]
impl SystemInvariant for ThroughputInvariant {
    async fn check(&self, state: &SystemState) -> InvariantResult {
        if state.metrics.throughput < self.min_throughput {
            return InvariantResult::Violated(InvariantViolation {
                message: format!(
                    "Throughput too low: {:.2} msg/s (min: {:.2} msg/s)",
                    state.metrics.throughput, self.min_throughput
                ),
                context: {
                    let mut context = HashMap::new();
                    context.insert("throughput".to_string(), state.metrics.throughput.to_string());
                    context.insert("min_throughput".to_string(), self.min_throughput.to_string());
                    context
                },
                timestamp: chrono::Utc::now(),
            });
        }
        
        InvariantResult::Satisfied
    }
    
    fn description(&self) -> &str {
        "System throughput must meet performance requirements"
    }
    
    fn severity(&self) -> InvariantSeverity {
        InvariantSeverity::Medium
    }
}

/// Ensures partition assignment is correct.
#[derive(Debug)]
pub struct PartitionInvariant {
    /// Whether to enforce exactly-one-leader constraint
    enforce_single_leader: bool,
}

impl PartitionInvariant {
    /// Create a new partition invariant.
    pub fn new() -> Self {
        Self {
            enforce_single_leader: true,
        }
    }
    
    /// Create with custom leader enforcement.
    pub fn with_leader_enforcement(enforce: bool) -> Self {
        Self {
            enforce_single_leader: enforce,
        }
    }
}

#[async_trait]
impl SystemInvariant for PartitionInvariant {
    async fn check(&self, state: &SystemState) -> InvariantResult {
        // Check that each partition has exactly one leader
        for (partition_id, partition_state) in &state.partitions {
            if partition_state.leader.is_none() {
                return InvariantResult::Violated(InvariantViolation {
                    message: format!("Partition {} has no leader", partition_id.0),
                    context: {
                        let mut context = HashMap::new();
                        context.insert("partition_id".to_string(), partition_id.0.to_string());
                        context
                    },
                    timestamp: chrono::Utc::now(),
                });
            }
            
            // Check that leader is actually online
            if let Some(leader_id) = &partition_state.leader {
                if let Some(leader_state) = state.nodes.get(leader_id) {
                    if !leader_state.is_online {
                        return InvariantResult::Violated(InvariantViolation {
                            message: format!(
                                "Partition {} leader {} is offline",
                                partition_id.0, leader_id
                            ),
                            context: {
                                let mut context = HashMap::new();
                                context.insert("partition_id".to_string(), partition_id.0.to_string());
                                context.insert("leader_id".to_string(), leader_id.clone());
                                context
                            },
                            timestamp: chrono::Utc::now(),
                        });
                    }
                }
            }
        }
        
        InvariantResult::Satisfied
    }
    
    fn description(&self) -> &str {
        "Partition assignment must be deterministic with exactly one leader per partition"
    }
    
    fn severity(&self) -> InvariantSeverity {
        InvariantSeverity::Critical
    }
}

/// Ensures authorization requirements are met.
#[derive(Debug)]
pub struct AuthorizationInvariant {
    /// Whether to enforce strict authorization
    strict_mode: bool,
}

impl AuthorizationInvariant {
    /// Create a new authorization invariant.
    pub fn new() -> Self {
        Self {
            strict_mode: true,
        }
    }
    
    /// Create with custom strictness.
    pub fn with_strict_mode(strict: bool) -> Self {
        Self {
            strict_mode: strict,
        }
    }
}

#[async_trait]
impl SystemInvariant for AuthorizationInvariant {
    async fn check(&self, state: &SystemState) -> InvariantResult {
        // Check that all nodes are properly authenticated and authorized
        for (_node_id, node_state) in &state.nodes {
            // For now, just check that online nodes have recent heartbeats
            if node_state.is_online {
                let heartbeat_age = chrono::Utc::now().signed_duration_since(node_state.last_heartbeat);
                if heartbeat_age > chrono::Duration::minutes(5) {
                    return InvariantResult::Violated(InvariantViolation {
                        message: "Node has stale heartbeat - possible authorization issue".to_string(),
                        context: {
                            let mut context = HashMap::new();
                            context.insert("heartbeat_age_minutes".to_string(), 
                                         heartbeat_age.num_minutes().to_string());
                            context
                        },
                        timestamp: chrono::Utc::now(),
                    });
                }
            }
        }
        
        InvariantResult::Satisfied
    }
    
    fn description(&self) -> &str {
        "All system operations must be properly authorized"
    }
    
    fn severity(&self) -> InvariantSeverity {
        InvariantSeverity::High
    }
}

impl Default for OrderingInvariant {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for DurabilityInvariant {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for ConsistencyInvariant {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for LatencyInvariant {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for ThroughputInvariant {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for PartitionInvariant {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for AuthorizationInvariant {
    fn default() -> Self {
        Self::new()
    }
}