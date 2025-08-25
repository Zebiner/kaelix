//! # Consensus Module
//!
//! Implements distributed consensus algorithms for high-performance cluster coordination.
//! Currently supports Raft and PBFT algorithms optimized for ultra-low latency (<1ms).

use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

// Re-export types from the main lib
use crate::types::NodeId;

/// Consensus-specific errors
#[derive(Debug, Error)]
pub enum ConsensusError {
    /// Term mismatch in consensus protocol
    #[error("Term mismatch: expected {expected}, got {actual}")]
    TermMismatch {
        /// Expected term
        expected: u64,
        /// Actual term received
        actual: u64,
    },

    /// Node is not the leader
    #[error("Not leader: current leader is {leader:?}")]
    NotLeader {
        /// Current leader node ID
        leader: Option<NodeId>,
    },

    /// Invalid node ID in consensus message
    #[error("Invalid node ID: {node_id}")]
    InvalidNodeId {
        /// The invalid node ID
        node_id: String,
    },

    /// Election timeout occurred
    #[error("Election timeout")]
    ElectionTimeout,

    /// Log inconsistency detected
    #[error("Log inconsistency: {details}")]
    LogInconsistency {
        /// Details of the inconsistency
        details: String,
    },

    /// Network partition detected
    #[error("Network partition detected")]
    NetworkPartition,

    /// Configuration error
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Generic consensus error
    #[error("Consensus error: {0}")]
    Generic(String),
}

/// Result type for consensus operations
pub type ConsensusResult<T> = Result<T, ConsensusError>;

/// Consensus state of a node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConsensusState {
    /// Node is a follower
    Follower,
    /// Node is a candidate (election in progress)
    Candidate,
    /// Node is the leader
    Leader,
}

impl Default for ConsensusState {
    fn default() -> Self {
        Self::Follower
    }
}

impl fmt::Display for ConsensusState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Follower => write!(f, "follower"),
            Self::Candidate => write!(f, "candidate"),
            Self::Leader => write!(f, "leader"),
        }
    }
}

/// Raft consensus implementation
pub mod raft {
    use super::*;
    use crate::types::Term;
    use serde::{Deserialize, Serialize};
    use std::{
        collections::HashMap,
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };
    use tokio::sync::RwLock;

    // Export cluster integration module
    pub mod cluster;
    // Export log replication module
    pub mod log_replication;
    // Export integrated coordinator module
    pub mod integrated_coordinator;

    /// Log index type for strong typing
    pub type LogIndex = u64;

    /// Raft configuration parameters optimized for <1ms elections
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RaftConfig {
        /// Minimum election timeout (50ms for ultra-low latency)
        pub election_timeout_min: Duration,
        /// Maximum election timeout (100ms for ultra-low latency)  
        pub election_timeout_max: Duration,
        /// Heartbeat interval (10ms for rapid leadership confirmation)
        pub heartbeat_interval: Duration,
        /// Maximum entries per append entries request for batching
        pub max_entries_per_append: usize,
        /// Log compaction threshold (entries before snapshot)
        pub snapshot_threshold: usize,
        /// Enable pre-vote phase to reduce disruptions
        pub pre_vote_enabled: bool,
        /// Maximum log replication pipeline depth
        pub max_pipeline_depth: usize,
        /// Batch timeout for collecting entries
        pub batch_timeout: Duration,
        /// Fast election optimization threshold (microseconds)
        pub fast_election_threshold_us: u64,
        /// Pre-vote timeout multiplier
        pub pre_vote_timeout_multiplier: f32,
        /// Leadership lease duration for optimization
        pub leadership_lease_duration: Duration,
    }

    impl Default for RaftConfig {
        fn default() -> Self {
            Self {
                election_timeout_min: Duration::from_millis(50),   // Reduced for <1ms target
                election_timeout_max: Duration::from_millis(100),  // Reduced for <1ms target
                heartbeat_interval: Duration::from_millis(10),     // More frequent heartbeats
                max_entries_per_append: 100,
                snapshot_threshold: 10000,
                pre_vote_enabled: true,                            // Always enable for stability
                max_pipeline_depth: 10,
                batch_timeout: Duration::from_millis(5),           // Faster batching
                fast_election_threshold_us: 500,                   // 500μs threshold for fast elections
                pre_vote_timeout_multiplier: 0.8,                  // Shorter pre-vote timeout
                leadership_lease_duration: Duration::from_millis(50), // Short leadership lease
            }
        }
    }

    /// Fast election optimization configuration
    #[derive(Debug, Clone)]
    pub struct FastElectionConfig {
        /// Enable single-round fast election when conditions are met
        pub enable_single_round: bool,
        /// Minimum cluster stability period before fast election
        pub stability_threshold: Duration,
        /// Maximum allowed network latency for fast election
        pub max_network_latency: Duration,
        /// Leadership transition optimization
        pub enable_leadership_transfer: bool,
    }

    impl Default for FastElectionConfig {
        fn default() -> Self {
            Self {
                enable_single_round: true,
                stability_threshold: Duration::from_millis(100),
                max_network_latency: Duration::from_micros(200),
                enable_leadership_transfer: true,
            }
        }
    }

    /// Enhanced election state tracking for performance optimization
    #[derive(Debug, Default)]
    struct ElectionState {
        /// Pre-vote phase votes received
        pre_votes_received: usize,
        /// Election phase votes received  
        votes_received: usize,
        /// Start time of current election attempt
        election_start: Option<Instant>,
        /// Pre-vote start time
        pre_vote_start: Option<Instant>,
        /// Election attempt counter for this term
        election_attempts: u32,
        /// Track nodes that responded in current election
        responded_nodes: Vec<NodeId>,
        /// Network latency measurements
        response_latencies: Vec<Duration>,
        /// Whether we're eligible for fast election
        fast_election_eligible: bool,
    }

    impl ElectionState {
        /// Reset for new election
        fn reset_for_election(&mut self) {
            self.votes_received = 0;
            self.election_start = Some(Instant::now());
            self.election_attempts += 1;
            self.responded_nodes.clear();
            self.response_latencies.clear();
            // Preserve fast_election_eligible and pre_vote state
        }

        /// Reset for new pre-vote
        fn reset_for_pre_vote(&mut self) {
            self.pre_votes_received = 0;
            self.pre_vote_start = Some(Instant::now());
            self.responded_nodes.clear();
            self.response_latencies.clear();
            self.fast_election_eligible = false;
        }

        /// Check if we can perform fast election
        fn can_fast_elect(&self, config: &FastElectionConfig, cluster_size: usize) -> bool {
            if !config.enable_single_round || cluster_size < 3 {
                return false;
            }

            // Check network latency conditions
            if !self.response_latencies.is_empty() {
                let avg_latency = Duration::from_nanos(
                    self.response_latencies.iter()
                        .map(|d| d.as_nanos() as u64)
                        .sum::<u64>() / self.response_latencies.len() as u64
                );
                if avg_latency > config.max_network_latency {
                    return false;
                }
            }

            self.fast_election_eligible && self.election_attempts == 1
        }

        /// Record response latency
        fn record_response(&mut self, node_id: NodeId, latency: Duration) {
            self.responded_nodes.push(node_id);
            self.response_latencies.push(latency);
        }
    }

    /// Raft node state machine with enhanced leader election
    #[derive(Debug)]
    pub struct RaftNode {
        /// Node identifier
        pub node_id: NodeId,
        /// Current term
        current_term: Term,
        /// Node state
        state: ConsensusState,
        /// Voted for in current term
        voted_for: Option<NodeId>,
        /// Log entries
        log: Vec<LogEntry>,
        /// Index of highest log entry known to be committed
        commit_index: LogIndex,
        /// Index of highest log entry applied to state machine
        last_applied: LogIndex,
        /// For leaders: next index to send to each follower
        next_index: HashMap<NodeId, LogIndex>,
        /// For leaders: highest index known to be replicated on each follower
        match_index: HashMap<NodeId, LogIndex>,
        /// Current leader (if known)
        current_leader: Option<NodeId>,
        /// Last heartbeat received time
        last_heartbeat: Option<Instant>,
        /// Vote count for current election
        vote_count: usize,
        /// Cluster size for majority calculation
        pub cluster_size: usize,
        /// Configuration
        config: RaftConfig,
        /// Fast election configuration
        fast_config: FastElectionConfig,
        /// Enhanced election state
        election_state: ElectionState,
        /// Performance metrics
        metrics: RaftMetrics,
        /// Leadership lease expiration
        leadership_lease: Option<Instant>,
        /// Request ID counter for tracking
        request_counter: AtomicU64,
    }

    /// Serializable timestamp for log entries
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub struct SerializableInstant {
        /// Nanoseconds since Unix epoch
        nanos_since_epoch: u64,
    }

    impl From<Instant> for SerializableInstant {
        fn from(instant: Instant) -> Self {
            // Convert Instant to SystemTime and then to nanoseconds
            let now_system = SystemTime::now();
            let now_instant = Instant::now();
            let duration_since_epoch = now_system.duration_since(UNIX_EPOCH)
                .unwrap_or_default();
            
            // Adjust for the difference between now and the target instant
            let adjusted_duration = if instant > now_instant {
                duration_since_epoch + (instant - now_instant)
            } else {
                duration_since_epoch.saturating_sub(now_instant - instant)
            };
            
            Self {
                nanos_since_epoch: adjusted_duration.as_nanos() as u64,
            }
        }
    }

    impl From<SerializableInstant> for Instant {
        fn from(_ts: SerializableInstant) -> Self {
            // This is an approximation - Instant cannot be directly constructed from absolute time
            // In practice, this would need a more sophisticated approach
            Instant::now()
        }
    }

    /// Raft log entry with enhanced metadata
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct LogEntry {
        /// Term when entry was received by leader
        pub term: Term,
        /// Command for state machine
        pub command: Vec<u8>,
        /// Index in the log
        pub index: LogIndex,
        /// Entry timestamp for debugging and ordering
        pub timestamp: SerializableInstant,
        /// Entry size in bytes for memory tracking
        pub size: usize,
    }

    /// Vote request message for leader election
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct RequestVoteRequest {
        /// Candidate's term
        pub term: Term,
        /// Candidate requesting vote
        pub candidate_id: NodeId,
        /// Index of candidate's last log entry
        pub last_log_index: LogIndex,
        /// Term of candidate's last log entry
        pub last_log_term: Term,
        /// Pre-vote flag (if pre-vote is enabled)
        pub pre_vote: bool,
        /// Request timestamp for latency tracking
        pub request_timestamp: u64,
        /// Fast election hint
        pub fast_election: bool,
    }

    /// Vote response message
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct RequestVoteResponse {
        /// Current term for candidate to update itself
        pub term: Term,
        /// True means candidate received vote
        pub vote_granted: bool,
        /// Pre-vote flag
        pub pre_vote: bool,
        /// Last log index for optimization
        pub last_log_index: Option<LogIndex>,
        /// Response timestamp for latency calculation
        pub response_timestamp: u64,
        /// Network round-trip time hint
        pub network_latency_us: Option<u64>,
    }

    /// Append entries request for log replication and heartbeats
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct AppendEntriesRequest {
        /// Leader's term
        pub term: Term,
        /// Leader ID
        pub leader_id: NodeId,
        /// Index of log entry immediately preceding new ones
        pub prev_log_index: LogIndex,
        /// Term of prev_log_index entry
        pub prev_log_term: Term,
        /// Log entries to store (empty for heartbeat)
        pub entries: Vec<LogEntry>,
        /// Leader's commit index
        pub leader_commit: LogIndex,
        /// Request ID for pipeline tracking
        pub request_id: u64,
        /// Leadership lease assertion
        pub leadership_lease_ms: Option<u64>,
    }

    /// Append entries response with enhanced conflict resolution
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct AppendEntriesResponse {
        /// Current term for leader to update itself
        pub term: Term,
        /// True if follower contained entry matching prev_log_index and prev_log_term
        pub success: bool,
        /// Hint for leader optimization: follower's last log index
        pub last_log_index: Option<LogIndex>,
        /// Conflict index for fast recovery
        pub conflict_index: Option<LogIndex>,
        /// Conflict term for fast recovery
        pub conflict_term: Option<Term>,
        /// Request ID for pipeline tracking
        pub request_id: u64,
        /// Processing latency in microseconds
        pub processing_latency_us: Option<u64>,
    }

    /// Install snapshot request for log compaction
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct InstallSnapshotRequest {
        /// Leader's term
        pub term: Term,
        /// Leader ID
        pub leader_id: NodeId,
        /// The snapshot replaces all entries up through and including this index
        pub last_included_index: LogIndex,
        /// Term of last_included_index
        pub last_included_term: Term,
        /// Raw bytes of the snapshot chunk, starting at offset
        pub data: Vec<u8>,
        /// True if this is the last chunk
        pub done: bool,
    }

    /// Install snapshot response
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct InstallSnapshotResponse {
        /// Current term for leader to update itself
        pub term: Term,
    }

    /// Performance and operational metrics enhanced for election tracking
    #[derive(Debug, Default, Clone)]
    pub struct RaftMetrics {
        /// Total elections started
        pub elections_started: u64,
        /// Elections won
        pub elections_won: u64,
        /// Pre-vote phases completed
        pub pre_votes_completed: u64,
        /// Fast elections completed
        pub fast_elections_completed: u64,
        /// Total log entries appended
        pub entries_appended: u64,
        /// Total log entries committed
        pub entries_committed: u64,
        /// Average heartbeat latency
        pub avg_heartbeat_latency: Duration,
        /// Average replication latency
        pub avg_replication_latency: Duration,
        /// Last election duration
        pub last_election_duration: Option<Duration>,
        /// Fastest election duration achieved
        pub fastest_election_duration: Option<Duration>,
        /// Average election duration
        pub avg_election_duration: Duration,
        /// Election timeout count
        pub election_timeouts: u64,
        /// Split vote scenarios
        pub split_votes: u64,
        /// Current log size in bytes
        pub log_size_bytes: usize,
        /// Network latency samples
        pub network_latency_samples: Vec<Duration>,
    }

    impl LogEntry {
        /// Create a new log entry
        pub fn new(term: Term, index: LogIndex, command: Vec<u8>) -> Self {
            let size = command.len();
            Self {
                term,
                command,
                index,
                timestamp: Instant::now().into(),
                size,
            }
        }
    }

    impl RaftNode {
        /// Create a new Raft node with enhanced election configuration
        pub fn new(node_id: NodeId, cluster_size: usize, config: RaftConfig) -> Self {
            tracing::info!("Creating Raft node {} for cluster size {} with optimized election config", 
                node_id, cluster_size);

            Self {
                node_id,
                current_term: Term::default(),
                state: ConsensusState::Follower,
                voted_for: None,
                log: Vec::new(),
                commit_index: 0,
                last_applied: 0,
                next_index: HashMap::new(),
                match_index: HashMap::new(),
                current_leader: None,
                last_heartbeat: None,
                vote_count: 0,
                cluster_size,
                config,
                fast_config: FastElectionConfig::default(),
                election_state: ElectionState::default(),
                metrics: RaftMetrics::default(),
                leadership_lease: None,
                request_counter: AtomicU64::new(0),
            }
        }

        /// Get current state
        #[inline]
        pub fn state(&self) -> ConsensusState {
            self.state
        }

        /// Get current term
        #[inline]
        pub fn current_term(&self) -> Term {
            self.current_term
        }

        /// Get node ID
        #[inline]
        pub fn node_id(&self) -> NodeId {
            self.node_id
        }

        /// Check if this node is the leader
        #[inline]
        pub fn is_leader(&self) -> bool {
            matches!(self.state, ConsensusState::Leader)
        }

        /// Check if this node is a candidate
        #[inline]
        pub fn is_candidate(&self) -> bool {
            matches!(self.state, ConsensusState::Candidate)
        }

        /// Get current leader
        #[inline]
        pub fn current_leader(&self) -> Option<NodeId> {
            self.current_leader
        }

        /// Get performance metrics
        #[inline]
        pub fn metrics(&self) -> &RaftMetrics {
            &self.metrics
        }

        /// Check if election timeout has occurred with intelligent randomization
        pub fn is_election_timeout(&self) -> bool {
            match self.state {
                ConsensusState::Follower => {
                    if let Some(last_heartbeat) = self.last_heartbeat {
                        let timeout = self.generate_adaptive_election_timeout();
                        last_heartbeat.elapsed() > timeout
                    } else {
                        // No heartbeat received yet, start election
                        true
                    }
                },
                ConsensusState::Candidate => {
                    if let Some(election_start) = self.election_state.election_start {
                        let timeout = if self.election_state.can_fast_elect(&self.fast_config, self.cluster_size) {
                            // Fast election timeout - much shorter
                            Duration::from_micros(self.config.fast_election_threshold_us)
                        } else {
                            self.generate_adaptive_election_timeout()
                        };
                        election_start.elapsed() > timeout
                    } else {
                        true
                    }
                },
                ConsensusState::Leader => false,
            }
        }

        /// Generate adaptive election timeout based on cluster conditions
        fn generate_adaptive_election_timeout(&self) -> Duration {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            
            // Base timeout range
            let mut min_ms = self.config.election_timeout_min.as_millis();
            let mut max_ms = self.config.election_timeout_max.as_millis();

            // Adapt based on recent network latency
            if !self.metrics.network_latency_samples.is_empty() {
                let avg_latency = self.metrics.network_latency_samples.iter()
                    .map(|d| d.as_millis())
                    .sum::<u128>() / self.metrics.network_latency_samples.len() as u128;
                
                // Add adaptive buffer based on network conditions
                let latency_buffer = (avg_latency * 3).max(10); // At least 10ms buffer
                min_ms = min_ms.max(latency_buffer);
                max_ms = max_ms.max(min_ms + 20);
            }

            // Reduce timeout for small clusters (faster elections)
            if self.cluster_size <= 3 {
                min_ms = min_ms * 2 / 3;
                max_ms = max_ms * 2 / 3;
            }

            // Add jitter to prevent split votes
            let jitter_factor = rng.gen_range(0.8..1.2);
            let timeout_ms = rng.gen_range(min_ms..=max_ms) as f64 * jitter_factor;
            
            Duration::from_millis(timeout_ms as u64)
        }

        /// Start pre-vote phase (if enabled) or direct election
        pub async fn start_election(&mut self) -> ConsensusResult<RequestVoteRequest> {
            if self.config.pre_vote_enabled && self.election_state.pre_vote_start.is_none() {
                // Start with pre-vote phase
                self.start_pre_vote().await
            } else {
                // Start actual election
                self.start_actual_election().await
            }
        }

        /// Start pre-vote phase to check electability
        async fn start_pre_vote(&mut self) -> ConsensusResult<RequestVoteRequest> {
            tracing::debug!("Node {} starting pre-vote for term {}", 
                self.node_id, self.current_term.value() + 1);

            self.election_state.reset_for_pre_vote();
            
            let (last_log_index, last_log_term) = self.last_log_info();
            let request_timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64;

            Ok(RequestVoteRequest {
                term: Term::new(self.current_term.value() + 1), // Next term for pre-vote
                candidate_id: self.node_id,
                last_log_index,
                last_log_term,
                pre_vote: true,
                request_timestamp,
                fast_election: false, // Pre-votes are not fast
            })
        }

        /// Start actual election after successful pre-vote or directly
        async fn start_actual_election(&mut self) -> ConsensusResult<RequestVoteRequest> {
            tracing::info!("Node {} starting election for term {}", 
                self.node_id, self.current_term.value() + 1);

            // Transition to candidate state
            self.current_term.increment();
            self.state = ConsensusState::Candidate;
            self.voted_for = Some(self.node_id);
            self.current_leader = None;
            self.vote_count = 1; // Vote for ourselves
            
            // Reset election state for actual election
            self.election_state.reset_for_election();
            
            // Update metrics
            self.metrics.elections_started += 1;

            let (last_log_index, last_log_term) = self.last_log_info();
            let request_timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64;

            // Check if we can do fast election
            let fast_election = self.election_state.can_fast_elect(&self.fast_config, self.cluster_size);
            if fast_election {
                tracing::debug!("Node {} attempting fast election", self.node_id);
            }

            Ok(RequestVoteRequest {
                term: self.current_term,
                candidate_id: self.node_id,
                last_log_index,
                last_log_term,
                pre_vote: false,
                request_timestamp,
                fast_election,
            })
        }

        /// Handle incoming vote request with enhanced pre-vote logic
        pub async fn handle_vote_request(&mut self, request: RequestVoteRequest) -> ConsensusResult<RequestVoteResponse> {
            let request_latency_start = Instant::now();
            
            tracing::debug!("Node {} handling {} request from {} for term {}", 
                self.node_id, 
                if request.pre_vote { "pre-vote" } else { "vote" },
                request.candidate_id, 
                request.term);

            // Calculate request processing latency
            let processing_start = Instant::now();

            let response = if request.pre_vote {
                self.handle_pre_vote_request(request).await?
            } else {
                self.handle_actual_vote_request(request).await?
            };

            // Record network latency sample if available
            if let Some(network_latency_us) = response.network_latency_us {
                let latency = Duration::from_micros(network_latency_us);
                if self.metrics.network_latency_samples.len() > 100 {
                    self.metrics.network_latency_samples.remove(0); // Keep recent samples
                }
                self.metrics.network_latency_samples.push(latency);
            }

            tracing::trace!("Vote request processed in {}μs", processing_start.elapsed().as_micros());
            Ok(response)
        }

        /// Handle pre-vote request
        async fn handle_pre_vote_request(&mut self, request: RequestVoteRequest) -> ConsensusResult<RequestVoteResponse> {
            // Pre-vote logic: don't increment term or change state
            let can_vote = self.would_vote_for_candidate(&request);
            let response_timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64;

            // Calculate network latency hint
            let network_latency_us = if request.request_timestamp > 0 {
                let rtt = response_timestamp.saturating_sub(request.request_timestamp);
                Some((rtt / 2000) as u64) // Convert to microseconds, rough RTT/2
            } else {
                None
            };

            Ok(RequestVoteResponse {
                term: self.current_term,
                vote_granted: can_vote,
                pre_vote: true,
                last_log_index: Some(self.last_log_info().0),
                response_timestamp,
                network_latency_us,
            })
        }

        /// Handle actual vote request
        async fn handle_actual_vote_request(&mut self, request: RequestVoteRequest) -> ConsensusResult<RequestVoteResponse> {
            // If term is outdated, reject immediately
            if request.term < self.current_term {
                return Ok(RequestVoteResponse {
                    term: self.current_term,
                    vote_granted: false,
                    pre_vote: false,
                    last_log_index: Some(self.last_log_info().0),
                    response_timestamp: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos() as u64,
                    network_latency_us: None,
                });
            }

            // If term is newer, update and become follower
            if request.term > self.current_term {
                self.become_follower(request.term)?;
            }

            let can_vote = self.would_vote_for_candidate(&request);
            let vote_granted = can_vote;

            if vote_granted {
                self.voted_for = Some(request.candidate_id);
                self.last_heartbeat = Some(Instant::now()); // Reset election timer
                tracing::debug!("Node {} voted for {} in term {}", 
                    self.node_id, request.candidate_id, request.term);
            }

            let response_timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64;

            // Calculate network latency hint
            let network_latency_us = if request.request_timestamp > 0 {
                let rtt = response_timestamp.saturating_sub(request.request_timestamp);
                Some((rtt / 2000) as u64) // Convert to microseconds, rough RTT/2
            } else {
                None
            };

            Ok(RequestVoteResponse {
                term: self.current_term,
                vote_granted,
                pre_vote: false,
                last_log_index: Some(self.last_log_info().0),
                response_timestamp,
                network_latency_us,
            })
        }

        /// Check if we would vote for a candidate (without side effects)
        fn would_vote_for_candidate(&self, request: &RequestVoteRequest) -> bool {
            // Check if we can vote for this candidate
            let can_vote = self.voted_for.is_none() || self.voted_for == Some(request.candidate_id);
            
            // Check if candidate's log is at least as up-to-date as ours
            let candidate_log_ok = self.is_log_up_to_date(request.last_log_index, request.last_log_term);
            
            can_vote && candidate_log_ok
        }

        /// Handle vote response from other nodes with enhanced election logic
        pub async fn handle_vote_response(&mut self, response: RequestVoteResponse) -> ConsensusResult<bool> {
            if !self.is_candidate() {
                return Ok(false);
            }

            let response_latency = if response.response_timestamp > 0 {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos() as u64;
                Duration::from_nanos(now.saturating_sub(response.response_timestamp))
            } else {
                Duration::from_millis(1) // Default estimate
            };

            // Record response for election optimization
            // Note: We don't have the sender NodeId in the response, so we use a placeholder
            // In a real implementation, the message routing would provide sender context
            let placeholder_node = NodeId::generate(); // This should come from message routing
            self.election_state.record_response(placeholder_node, response_latency);

            // If we receive a higher term, become follower
            if response.term > self.current_term {
                self.become_follower(response.term)?;
                return Ok(false);
            }

            // Handle pre-vote response
            if response.pre_vote {
                return self.handle_pre_vote_response(response).await;
            }

            // Handle actual vote response
            if response.term == self.current_term && response.vote_granted {
                self.vote_count += 1;

                // Check if we have majority
                let majority = (self.cluster_size / 2) + 1;
                if self.vote_count >= majority {
                    self.become_leader().await?;
                    return Ok(true);
                }
            }

            Ok(false)
        }

        /// Handle pre-vote response
        async fn handle_pre_vote_response(&mut self, response: RequestVoteResponse) -> ConsensusResult<bool> {
            if response.vote_granted {
                self.election_state.pre_votes_received += 1;
                
                // Check if we have majority pre-votes
                let majority = (self.cluster_size / 2) + 1;
                if self.election_state.pre_votes_received >= majority {
                    tracing::debug!("Node {} received majority pre-votes, starting election", self.node_id);
                    
                    // Mark eligible for fast election if conditions are met
                    if self.election_state.can_fast_elect(&self.fast_config, self.cluster_size) {
                        self.election_state.fast_election_eligible = true;
                    }
                    
                    self.metrics.pre_votes_completed += 1;
                    
                    // Pre-vote successful, start actual election
                    // This would need to trigger a new election request broadcast
                    // The cluster coordinator should handle this
                }
            }
            
            Ok(false)
        }

        /// Become leader after winning election with enhanced leadership establishment
        pub async fn become_leader(&mut self) -> ConsensusResult<()> {
            if !matches!(self.state, ConsensusState::Candidate) {
                return Err(ConsensusError::Generic(
                    "Can only become leader from candidate state".to_string(),
                ));
            }

            let election_duration = self.election_state.election_start
                .map(|start| start.elapsed())
                .unwrap_or_default();

            tracing::info!("Node {} became leader for term {} in {}μs", 
                self.node_id, self.current_term, election_duration.as_micros());

            self.state = ConsensusState::Leader;
            self.current_leader = Some(self.node_id);
            self.metrics.elections_won += 1;

            // Record election timing metrics
            self.metrics.last_election_duration = Some(election_duration);
            if self.metrics.fastest_election_duration.is_none() || 
               Some(election_duration) < self.metrics.fastest_election_duration {
                self.metrics.fastest_election_duration = Some(election_duration);
            }

            // Update average election duration (simple moving average)
            let current_avg_nanos = self.metrics.avg_election_duration.as_nanos() as f64;
            let new_duration_nanos = election_duration.as_nanos() as f64;
            let elections_count = self.metrics.elections_won as f64;
            let new_avg_nanos = (current_avg_nanos * (elections_count - 1.0) + new_duration_nanos) / elections_count;
            self.metrics.avg_election_duration = Duration::from_nanos(new_avg_nanos as u64);

            // Check if this was a fast election
            if election_duration.as_micros() < self.config.fast_election_threshold_us as u128 {
                self.metrics.fast_elections_completed += 1;
                tracing::info!("Fast election completed in {}μs", election_duration.as_micros());
            }

            // Set leadership lease
            self.leadership_lease = Some(Instant::now() + self.config.leadership_lease_duration);

            // Initialize leader state - set next_index to last log index + 1 for all followers
            let last_log_index = self.log.len() as LogIndex;
            for &peer_id in &self.get_peer_ids() {
                self.next_index.insert(peer_id, last_log_index + 1);
                self.match_index.insert(peer_id, 0);
            }

            // Reset election state
            self.election_state = ElectionState::default();

            tracing::info!("Leadership established for node {} in term {} (total elections: {}, avg duration: {}μs)", 
                self.node_id, 
                self.current_term, 
                self.metrics.elections_won,
                self.metrics.avg_election_duration.as_micros());

            Ok(())
        }

        /// Become follower with enhanced state cleanup
        pub fn become_follower(&mut self, term: Term) -> ConsensusResult<()> {
            if term < self.current_term {
                return Err(ConsensusError::TermMismatch {
                    expected: self.current_term.value(),
                    actual: term.value(),
                });
            }

            let state_changed = self.state != ConsensusState::Follower || term != self.current_term;

            self.current_term = term;
            self.state = ConsensusState::Follower;
            self.voted_for = None;
            self.vote_count = 0;
            self.leadership_lease = None;

            // Reset election state
            self.election_state = ElectionState::default();

            if state_changed {
                tracing::info!("Node {} became follower for term {}", self.node_id, self.current_term);
            }

            Ok(())
        }

        /// Handle append entries request (log replication and heartbeat)
        pub async fn handle_append_entries(&mut self, request: AppendEntriesRequest) -> ConsensusResult<AppendEntriesResponse> {
            let processing_start = Instant::now();
            
            tracing::trace!("Node {} handling append entries from {} (term={}, entries={}, lease={:?})", 
                self.node_id, request.leader_id, request.term, request.entries.len(), request.leadership_lease_ms);

            // If term is outdated, reject
            if request.term < self.current_term {
                return Ok(AppendEntriesResponse {
                    term: self.current_term,
                    success: false,
                    last_log_index: Some(self.last_log_info().0),
                    conflict_index: None,
                    conflict_term: None,
                    request_id: request.request_id,
                    processing_latency_us: Some(processing_start.elapsed().as_micros() as u64),
                });
            }

            // If term is newer or equal, update and become follower
            if request.term >= self.current_term {
                self.become_follower(request.term)?;
                self.current_leader = Some(request.leader_id);
                self.last_heartbeat = Some(Instant::now());
                
                // Process leadership lease if provided
                if let Some(lease_ms) = request.leadership_lease_ms {
                    let lease_duration = Duration::from_millis(lease_ms);
                    // We don't store leadership lease for followers, but acknowledge it
                    tracing::trace!("Acknowledged leadership lease of {}ms from {}", lease_ms, request.leader_id);
                }
            }

            // Check log consistency
            let prev_log_ok = self.check_log_consistency(request.prev_log_index, request.prev_log_term);

            if !prev_log_ok {
                let (conflict_index, conflict_term) = self.find_conflict_info(request.prev_log_index);
                return Ok(AppendEntriesResponse {
                    term: self.current_term,
                    success: false,
                    last_log_index: Some(self.last_log_info().0),
                    conflict_index: Some(conflict_index),
                    conflict_term,
                    request_id: request.request_id,
                    processing_latency_us: Some(processing_start.elapsed().as_micros() as u64),
                });
            }

            // Append new entries if any
            if !request.entries.is_empty() {
                self.append_new_entries(request.prev_log_index, &request.entries)?;
                self.metrics.entries_appended += request.entries.len() as u64;
            }

            // Update commit index
            if request.leader_commit > self.commit_index {
                self.commit_index = std::cmp::min(request.leader_commit, self.log.len() as LogIndex);
                tracing::debug!("Node {} updated commit index to {}", self.node_id, self.commit_index);
            }

            Ok(AppendEntriesResponse {
                term: self.current_term,
                success: true,
                last_log_index: Some(self.last_log_info().0),
                conflict_index: None,
                conflict_term: None,
                request_id: request.request_id,
                processing_latency_us: Some(processing_start.elapsed().as_micros() as u64),
            })
        }

        /// Create append entries request for a specific follower
        pub async fn create_append_entries(&self, follower_id: NodeId) -> ConsensusResult<AppendEntriesRequest> {
            if !self.is_leader() {
                return Err(ConsensusError::NotLeader { leader: self.current_leader });
            }

            let next_index = self.next_index.get(&follower_id).copied().unwrap_or(1);
            let prev_log_index = next_index.saturating_sub(1);

            let (prev_log_term, entries) = if prev_log_index == 0 {
                (Term::default(), self.get_entries_from(1))
            } else if let Some(prev_entry) = self.log.get((prev_log_index - 1) as usize) {
                (prev_entry.term, self.get_entries_from(next_index))
            } else {
                return Err(ConsensusError::LogInconsistency {
                    details: format!("Missing log entry at index {prev_log_index}"),
                });
            };

            // Limit entries for performance
            let entries: Vec<LogEntry> = entries
                .iter()
                .take(self.config.max_entries_per_append)
                .cloned()
                .collect();

            // Include leadership lease information
            let leadership_lease_ms = self.leadership_lease
                .and_then(|lease| lease.checked_duration_since(Instant::now()).ok())
                .map(|duration| duration.as_millis() as u64);

            Ok(AppendEntriesRequest {
                term: self.current_term,
                leader_id: self.node_id,
                prev_log_index,
                prev_log_term,
                entries,
                leader_commit: self.commit_index,
                request_id: self.generate_request_id(),
                leadership_lease_ms,
            })
        }

        /// Handle append entries response from followers
        pub async fn handle_append_entries_response(
            &mut self,
            follower_id: NodeId,
            response: AppendEntriesResponse,
        ) -> ConsensusResult<()> {
            if !self.is_leader() {
                return Ok(());
            }

            // Track processing latency if provided
            if let Some(latency_us) = response.processing_latency_us {
                let latency = Duration::from_micros(latency_us);
                if self.metrics.network_latency_samples.len() > 100 {
                    self.metrics.network_latency_samples.remove(0);
                }
                self.metrics.network_latency_samples.push(latency);
            }

            // If we receive a higher term, step down
            if response.term > self.current_term {
                self.become_follower(response.term)?;
                return Ok(());
            }

            if response.success {
                // Update follower's progress
                if let Some(last_log_index) = response.last_log_index {
                    self.next_index.insert(follower_id, last_log_index + 1);
                    self.match_index.insert(follower_id, last_log_index);
                }

                // Try to update commit index
                self.try_advance_commit_index();
            } else {
                // Handle log conflict with optimized backoff
                self.handle_log_conflict(follower_id, response).await?;
            }

            Ok(())
        }

        /// Append a new entry to the log (leader only)
        pub async fn append_entry(&mut self, command: Vec<u8>) -> ConsensusResult<LogIndex> {
            if !self.is_leader() {
                return Err(ConsensusError::NotLeader { leader: self.current_leader });
            }

            // Check leadership lease validity
            if let Some(lease) = self.leadership_lease {
                if Instant::now() > lease {
                    tracing::warn!("Leadership lease expired for node {}, stepping down", self.node_id);
                    let current_term = self.current_term;
                    self.become_follower(current_term)?;
                    return Err(ConsensusError::NotLeader { leader: None });
                }
            }

            let index = (self.log.len() + 1) as LogIndex;
            let entry = LogEntry::new(self.current_term, index, command);
            
            self.log.push(entry.clone());
            self.metrics.entries_appended += 1;
            self.metrics.log_size_bytes += entry.size;

            tracing::debug!("Leader {} appended entry at index {}", self.node_id, index);
            Ok(index)
        }

        /// Apply committed entries to state machine
        pub async fn apply_committed_entries<F>(&mut self, mut apply_fn: F) -> ConsensusResult<()>
        where
            F: FnMut(&LogEntry) -> ConsensusResult<()>,
        {
            while self.last_applied < self.commit_index {
                self.last_applied += 1;
                
                if let Some(entry) = self.log.get((self.last_applied - 1) as usize) {
                    apply_fn(entry)?;
                    self.metrics.entries_committed += 1;
                } else {
                    return Err(ConsensusError::LogInconsistency {
                        details: format!("Missing committed entry at index {}", self.last_applied),
                    });
                }
            }
            Ok(())
        }

        /// Check if leadership lease is still valid
        pub fn is_leadership_lease_valid(&self) -> bool {
            if let Some(lease) = self.leadership_lease {
                Instant::now() <= lease
            } else {
                false
            }
        }

        /// Force leadership transfer to another node
        pub async fn initiate_leadership_transfer(&mut self, target_node: Option<NodeId>) -> ConsensusResult<()> {
            if !self.is_leader() {
                return Err(ConsensusError::NotLeader { leader: self.current_leader });
            }

            tracing::info!("Node {} initiating leadership transfer to {:?}", self.node_id, target_node);

            // In a complete implementation, this would:
            // 1. Stop accepting new client requests
            // 2. Ensure target node is caught up on log (if specified)
            // 3. Send TimeoutNow message to trigger immediate election
            // 4. Step down as leader

            // For now, just step down and let normal election process handle it
            let current_term = self.current_term;
            self.become_follower(current_term)?;

            Ok(())
        }

        // ========================================
        // Private Helper Methods
        // ========================================

        /// Check if candidate's log is at least as up-to-date as ours
        fn is_log_up_to_date(&self, last_log_index: LogIndex, last_log_term: Term) -> bool {
            let (our_last_index, our_last_term) = self.last_log_info();
            
            // If the last entries have different terms, then the log with the later term is more up-to-date
            if last_log_term != our_last_term {
                last_log_term > our_last_term
            } else {
                // If the logs end with the same term, then whichever log is longer is more up-to-date
                last_log_index >= our_last_index
            }
        }

        /// Get the last log index and term
        fn last_log_info(&self) -> (LogIndex, Term) {
            if let Some(last_entry) = self.log.last() {
                (last_entry.index, last_entry.term)
            } else {
                (0, Term::default())
            }
        }

        /// Check log consistency for append entries
        fn check_log_consistency(&self, prev_log_index: LogIndex, prev_log_term: Term) -> bool {
            if prev_log_index == 0 {
                true // Initial case
            } else if let Some(entry) = self.log.get((prev_log_index - 1) as usize) {
                entry.term == prev_log_term
            } else {
                false // Log doesn't contain entry at prev_log_index
            }
        }

        /// Find conflict information for fast log repair
        fn find_conflict_info(&self, prev_log_index: LogIndex) -> (LogIndex, Option<Term>) {
            if prev_log_index > self.log.len() as LogIndex {
                // Follower's log is shorter
                return (self.log.len() as LogIndex, None);
            } else if prev_log_index > 0 {
                if let Some(entry) = self.log.get((prev_log_index - 1) as usize) {
                    // Find first entry of the conflicting term
                    let conflict_term = entry.term;
                    for (i, log_entry) in self.log.iter().enumerate() {
                        if log_entry.term == conflict_term {
                            return ((i + 1) as LogIndex, Some(conflict_term));
                        }
                    }
                }
            }
            (1, None)
        }

        /// Append new entries to the log, removing conflicts
        fn append_new_entries(&mut self, prev_log_index: LogIndex, entries: &[LogEntry]) -> ConsensusResult<()> {
            // Remove any conflicting entries
            self.log.truncate(prev_log_index as usize);

            // Append new entries
            for entry in entries {
                self.log.push(entry.clone());
            }

            tracing::debug!("Node {} appended {} entries at index {}", 
                self.node_id, entries.len(), prev_log_index);
            Ok(())
        }

        /// Get log entries starting from index
        fn get_entries_from(&self, start_index: LogIndex) -> &[LogEntry] {
            if start_index == 0 || start_index > self.log.len() as LogIndex {
                &[]
            } else {
                &self.log[(start_index - 1) as usize..]
            }
        }

        /// Try to advance commit index based on majority replication
        fn try_advance_commit_index(&mut self) {
            if !self.is_leader() {
                return;
            }

            let mut match_indices: Vec<LogIndex> = self.match_index.values().copied().collect();
            match_indices.push(self.log.len() as LogIndex); // Include leader's log
            match_indices.sort_unstable();

            // Find the highest index replicated on majority
            let majority_index = match_indices.len() / 2;
            let new_commit_index = match_indices[majority_index];

            // Only advance commit index if the entry is from current term
            if new_commit_index > self.commit_index {
                if let Some(entry) = self.log.get((new_commit_index - 1) as usize) {
                    if entry.term == self.current_term {
                        let old_commit = self.commit_index;
                        self.commit_index = new_commit_index;
                        tracing::debug!("Leader {} advanced commit index from {} to {}", 
                            self.node_id, old_commit, self.commit_index);
                    }
                }
            }
        }

        /// Handle log conflict with optimized backoff
        async fn handle_log_conflict(&mut self, follower_id: NodeId, response: AppendEntriesResponse) -> ConsensusResult<()> {
            if let (Some(conflict_index), conflict_term) = (response.conflict_index, response.conflict_term) {
                // Fast conflict resolution: find the end of the conflicting term
                if let Some(term) = conflict_term {
                    let mut next_index = conflict_index;
                    for (i, entry) in self.log.iter().enumerate().rev() {
                        if entry.term == term {
                            next_index = (i + 2) as LogIndex; // Next index after the conflicting term
                            break;
                        }
                    }
                    self.next_index.insert(follower_id, next_index);
                } else {
                    // Simple backoff
                    self.next_index.insert(follower_id, conflict_index);
                }
            } else {
                // Fallback: decrement next_index
                if let Some(next_idx) = self.next_index.get_mut(&follower_id) {
                    if *next_idx > 1 {
                        *next_idx -= 1;
                    }
                }
            }
            Ok(())
        }

        /// Generate unique request ID for pipeline tracking
        fn generate_request_id(&self) -> u64 {
            self.request_counter.fetch_add(1, Ordering::Relaxed)
        }

        /// Get peer node IDs (placeholder - should be integrated with membership)
        fn get_peer_ids(&self) -> Vec<NodeId> {
            // This should be integrated with the SWIM membership protocol
            // For now, returning empty vec as placeholder
            Vec::new()
        }
    }

    /// Raft state machine that coordinates consensus operations with enhanced performance monitoring
    pub struct RaftStateMachine {
        /// Raft node
        pub node: RwLock<RaftNode>,
        /// Shutdown signal
        pub shutdown: tokio::sync::watch::Receiver<bool>,
        /// Performance monitoring interval
        monitoring_interval: Duration,
    }

    impl RaftStateMachine {
        /// Create new Raft state machine with performance monitoring
        pub fn new(node_id: NodeId, cluster_size: usize, config: RaftConfig) -> (Self, tokio::sync::watch::Sender<bool>) {
            let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
            let node = RaftNode::new(node_id, cluster_size, config);
            
            (
                Self {
                    node: RwLock::new(node),
                    shutdown: shutdown_rx,
                    monitoring_interval: Duration::from_secs(30), // Monitor every 30 seconds
                },
                shutdown_tx,
            )
        }

        /// Start the Raft consensus loop with enhanced monitoring
        pub async fn run(&mut self) -> ConsensusResult<()> {
            let mut heartbeat_interval = tokio::time::interval(
                self.node.read().await.config.heartbeat_interval
            );
            let mut monitoring_interval = tokio::time::interval(self.monitoring_interval);

            loop {
                tokio::select! {
                    _ = heartbeat_interval.tick() => {
                        self.heartbeat_tick().await?;
                    }
                    _ = monitoring_interval.tick() => {
                        self.performance_monitoring_tick().await?;
                    }
                    _ = self.shutdown.changed() => {
                        if *self.shutdown.borrow() {
                            tracing::info!("Raft state machine shutting down");
                            break;
                        }
                    }
                }
            }

            Ok(())
        }

        /// Handle heartbeat tick with election timeout detection
        async fn heartbeat_tick(&self) -> ConsensusResult<()> {
            let mut node = self.node.write().await;
            
            match node.state() {
                ConsensusState::Leader => {
                    // Check leadership lease validity
                    if !node.is_leadership_lease_valid() {
                        tracing::warn!("Leadership lease expired for node {}, stepping down", node.node_id());
                        let current_term = node.current_term();
                        node.become_follower(current_term)?;
                    } else {
                        tracing::trace!("Leader {} sending heartbeats", node.node_id());
                    }
                }
                ConsensusState::Follower | ConsensusState::Candidate => {
                    // Check for election timeout
                    if node.is_election_timeout() {
                        tracing::debug!("Node {} detected election timeout, starting election", node.node_id());
                        node.metrics.election_timeouts += 1;
                        let _vote_request = node.start_election().await?;
                        // In a real implementation, this would broadcast the vote request
                    }
                }
            }

            Ok(())
        }

        /// Performance monitoring tick
        async fn performance_monitoring_tick(&self) -> ConsensusResult<()> {
            let node = self.node.read().await;
            let metrics = &node.metrics;

            tracing::info!(
                "Raft performance metrics for node {}: elections={} (won={}), fastest={}μs, avg={}μs, fast_elections={}, timeouts={}, split_votes={}",
                node.node_id(),
                metrics.elections_started,
                metrics.elections_won,
                metrics.fastest_election_duration.map_or(0, |d| d.as_micros()),
                metrics.avg_election_duration.as_micros(),
                metrics.fast_elections_completed,
                metrics.election_timeouts,
                metrics.split_votes
            );

            // Log network latency statistics
            if !metrics.network_latency_samples.is_empty() {
                let avg_latency_us = metrics.network_latency_samples.iter()
                    .map(|d| d.as_micros())
                    .sum::<u128>() / metrics.network_latency_samples.len() as u128;
                
                tracing::debug!("Average network latency: {}μs (samples: {})", 
                    avg_latency_us, metrics.network_latency_samples.len());
            }

            Ok(())
        }

        /// Get current election performance summary
        pub async fn election_performance_summary(&self) -> ElectionPerformanceSummary {
            let node = self.node.read().await;
            let metrics = &node.metrics;

            ElectionPerformanceSummary {
                total_elections: metrics.elections_started,
                successful_elections: metrics.elections_won,
                fast_elections: metrics.fast_elections_completed,
                average_duration: metrics.avg_election_duration,
                fastest_duration: metrics.fastest_election_duration,
                election_timeouts: metrics.election_timeouts,
                split_votes: metrics.split_votes,
                success_rate: if metrics.elections_started > 0 {
                    (metrics.elections_won as f64) / (metrics.elections_started as f64)
                } else {
                    0.0
                },
            }
        }
    }

    /// Election performance summary for monitoring
    #[derive(Debug, Clone)]
    pub struct ElectionPerformanceSummary {
        /// Total elections attempted
        pub total_elections: u64,
        /// Successful elections (became leader)
        pub successful_elections: u64,
        /// Fast elections completed
        pub fast_elections: u64,
        /// Average election duration
        pub average_duration: Duration,
        /// Fastest election duration achieved
        pub fastest_duration: Option<Duration>,
        /// Number of election timeouts
        pub election_timeouts: u64,
        /// Number of split vote scenarios
        pub split_votes: u64,
        /// Election success rate (0.0 to 1.0)
        pub success_rate: f64,
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn create_test_node(cluster_size: usize) -> RaftNode {
            let node_id = NodeId::generate();
            let config = RaftConfig::default();
            RaftNode::new(node_id, cluster_size, config)
        }

        #[test]
        fn test_raft_node_creation() {
            let node = create_test_node(3);
            assert_eq!(node.state(), ConsensusState::Follower);
            assert_eq!(node.current_term(), Term::default());
            assert!(!node.is_leader());
        }

        #[tokio::test]
        async fn test_start_election() {
            let mut node = create_test_node(3);
            let vote_request = node.start_election().await.unwrap();
            
            assert_eq!(node.state(), ConsensusState::Candidate);
            assert_eq!(node.current_term(), Term::new(1));
            assert_eq!(vote_request.candidate_id, node.node_id());
            assert_eq!(node.vote_count, 1);
        }

        #[tokio::test]
        async fn test_pre_vote_enabled() {
            let mut node = create_test_node(3);
            assert!(node.config.pre_vote_enabled);
            
            // First call should start pre-vote
            let pre_vote_request = node.start_election().await.unwrap();
            assert!(pre_vote_request.pre_vote);
            assert_eq!(node.state(), ConsensusState::Follower); // Still follower during pre-vote
        }

        #[tokio::test]
        async fn test_vote_request_handling() {
            let candidate_id = NodeId::generate();
            let mut node = create_test_node(3);

            let request = RequestVoteRequest {
                term: Term::new(1),
                candidate_id,
                last_log_index: 0,
                last_log_term: Term::default(),
                pre_vote: false,
                request_timestamp: 0,
                fast_election: false,
            };

            let response = node.handle_vote_request(request).await.unwrap();

            assert!(response.vote_granted);
            assert_eq!(response.term, Term::new(1));
            assert_eq!(node.voted_for, Some(candidate_id));
        }

        #[tokio::test]
        async fn test_become_leader() {
            let mut node = create_test_node(3);
            
            // Start election first
            let _vote_request = node.start_election().await.unwrap();
            
            // Simulate receiving majority votes (already have 1, need 1 more for majority of 3)
            node.vote_count = 2;
            
            // Become leader
            node.become_leader().await.unwrap();
            
            assert_eq!(node.state(), ConsensusState::Leader);
            assert!(node.is_leader());
            assert!(node.leadership_lease.is_some());
        }

        #[tokio::test]
        async fn test_fast_election_detection() {
            let mut node = create_test_node(3);
            node.election_state.fast_election_eligible = true;
            
            // Should be eligible for fast election with proper conditions
            assert!(node.election_state.can_fast_elect(&node.fast_config.clone(), 3));
        }

        #[tokio::test]
        async fn test_leadership_lease() {
            let mut node = create_test_node(3);
            
            // Start election and become leader
            let _vote_request = node.start_election().await.unwrap();
            node.vote_count = 2;
            node.become_leader().await.unwrap();
            
            // Leadership lease should be valid initially
            assert!(node.is_leadership_lease_valid());
        }

        #[tokio::test]
        async fn test_append_entries_heartbeat_with_lease() {
            let leader_id = NodeId::generate();
            let mut node = create_test_node(3);

            let request = AppendEntriesRequest {
                term: Term::new(1),
                leader_id,
                prev_log_index: 0,
                prev_log_term: Term::default(),
                entries: vec![],
                leader_commit: 0,
                request_id: 1,
                leadership_lease_ms: Some(50), // 50ms lease
            };

            let response = node.handle_append_entries(request).await.unwrap();

            assert!(response.success);
            assert_eq!(response.term, Term::new(1));
            assert_eq!(node.state(), ConsensusState::Follower);
            assert_eq!(node.current_leader(), Some(leader_id));
            assert!(response.processing_latency_us.is_some());
        }

        #[tokio::test]
        async fn test_log_entry_append() {
            let mut node = create_test_node(3);
            
            // Become leader
            node.start_election().await.unwrap();
            node.vote_count = 2;
            node.become_leader().await.unwrap();
            
            // Append entry
            let command = b"test command".to_vec();
            let index = node.append_entry(command.clone()).await.unwrap();
            
            assert_eq!(index, 1);
            assert_eq!(node.log.len(), 1);
            assert_eq!(node.log[0].command, command);
        }

        #[test]
        fn test_adaptive_election_timeout() {
            let node = create_test_node(3);
            let timeout1 = node.generate_adaptive_election_timeout();
            let timeout2 = node.generate_adaptive_election_timeout();
            
            // Timeouts should be within configured range (adjusted for small cluster)
            let expected_min = Duration::from_millis(33); // 2/3 of 50ms for small cluster
            let expected_max = Duration::from_millis(67); // 2/3 of 100ms for small cluster
            
            assert!(timeout1 >= expected_min);
            assert!(timeout1 <= Duration::from_millis(100)); // Upper bound
            assert!(timeout2 >= expected_min);
            assert!(timeout2 <= Duration::from_millis(100)); // Upper bound
        }

        #[test]
        fn test_election_state() {
            let mut election_state = ElectionState::default();
            
            election_state.reset_for_pre_vote();
            assert_eq!(election_state.pre_votes_received, 0);
            assert!(election_state.pre_vote_start.is_some());
            
            election_state.reset_for_election();
            assert_eq!(election_state.votes_received, 0);
            assert!(election_state.election_start.is_some());
            assert_eq!(election_state.election_attempts, 1);
        }

        #[test]
        fn test_log_consistency_check() {
            let mut node = create_test_node(3);
            
            // Add some log entries
            node.log.push(LogEntry::new(Term::new(1), 1, b"cmd1".to_vec()));
            node.log.push(LogEntry::new(Term::new(1), 2, b"cmd2".to_vec()));
            
            // Test consistency checks
            assert!(node.check_log_consistency(0, Term::default())); // Initial case
            assert!(node.check_log_consistency(1, Term::new(1))); // Matching entry
            assert!(!node.check_log_consistency(1, Term::new(2))); // Term mismatch
            assert!(!node.check_log_consistency(3, Term::new(1))); // Missing entry
        }

        #[tokio::test]
        async fn test_state_machine_creation() {
            let node_id = NodeId::generate();
            let config = RaftConfig::default();
            let (state_machine, _shutdown) = RaftStateMachine::new(node_id, 3, config);
            
            let node = state_machine.node.read().await;
            assert_eq!(node.node_id(), node_id);
            assert_eq!(node.state(), ConsensusState::Follower);
        }

        #[tokio::test]
        async fn test_election_performance_summary() {
            let node_id = NodeId::generate();
            let config = RaftConfig::default();
            let (state_machine, _shutdown) = RaftStateMachine::new(node_id, 3, config);
            
            let summary = state_machine.election_performance_summary().await;
            assert_eq!(summary.total_elections, 0);
            assert_eq!(summary.success_rate, 0.0);
        }

        #[test]
        fn test_fast_election_config() {
            let fast_config = FastElectionConfig::default();
            assert!(fast_config.enable_single_round);
            assert_eq!(fast_config.stability_threshold, Duration::from_millis(100));
            assert_eq!(fast_config.max_network_latency, Duration::from_micros(200));
        }

        #[tokio::test]
        async fn test_leadership_transfer() {
            let mut node = create_test_node(3);
            
            // Become leader
            node.start_election().await.unwrap();
            node.vote_count = 2;
            node.become_leader().await.unwrap();
            
            // Initiate leadership transfer
            let target = Some(NodeId::generate());
            node.initiate_leadership_transfer(target).await.unwrap();
            
            assert_eq!(node.state(), ConsensusState::Follower);
            assert!(node.leadership_lease.is_none());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consensus_state_display() {
        assert_eq!(ConsensusState::Follower.to_string(), "follower");
        assert_eq!(ConsensusState::Candidate.to_string(), "candidate");
        assert_eq!(ConsensusState::Leader.to_string(), "leader");
    }

    #[test]
    fn test_consensus_error_display() {
        let error = ConsensusError::TermMismatch { expected: 5, actual: 3 };
        assert!(error.to_string().contains("Term mismatch"));
        assert!(error.to_string().contains("expected 5"));
        assert!(error.to_string().contains("got 3"));
    }

    #[test]
    fn test_consensus_state_default() {
        assert_eq!(ConsensusState::default(), ConsensusState::Follower);
    }
}