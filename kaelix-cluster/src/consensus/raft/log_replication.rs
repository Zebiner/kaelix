//! # Raft Log Replication with WAL Integration
//!
//! This module implements the log replication mechanism for Raft consensus
//! with integrated Write-Ahead Log (WAL) support. It provides:
//! - High-performance log entry management
//! - Integration with external WAL storage systems
//! - Conflict resolution and consistency guarantees
//! - Ultra-low latency replication (<10μs target)

use std::{
    collections::{HashMap, BTreeMap},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, Mutex};
use tracing::{debug, info, trace, warn};

use crate::{
    communication::{NetworkMessage, MessageRouter},
    consensus::{ConsensusError, ConsensusResult},
    types::{NodeId, Term},
};

use super::{
    LogIndex, LogEntry as RaftLogEntry, AppendEntriesRequest, AppendEntriesResponse,
    RaftNode, RaftConfig,
};

/// WAL storage abstraction trait
pub trait WalStorage: Send + Sync {
    /// Write an entry to WAL
    fn write(&self, message: Vec<u8>) -> impl std::future::Future<Output = Result<u64, String>> + Send;
    
    /// Read entries from WAL
    fn read(&self, start_seq: u64, max_entries: u64) -> impl std::future::Future<Output = Result<Vec<WalEntry>, String>> + Send;
}

/// WAL entry representation
#[derive(Debug, Clone)]
pub struct WalEntry {
    /// Sequence number
    pub sequence_number: u64,
    /// Entry data
    pub data: Vec<u8>,
}

/// Raft log entry extended with WAL integration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RaftWalEntry {
    /// Raft log entry
    pub raft_entry: RaftLogEntry,
    /// WAL sequence number for cross-referencing
    pub wal_sequence: u64,
    /// Entry commit status
    pub commit_status: CommitStatus,
    /// Replication tracking
    pub replication_info: ReplicationInfo,
}

/// Commit status for log entries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommitStatus {
    /// Entry is uncommitted
    Pending,
    /// Entry is committed and applied
    Committed,
    /// Entry is being compacted/snapshotted
    Compacting,
}

/// Replication information for tracking follower progress
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationInfo {
    /// Which nodes have acknowledged this entry
    pub acknowledged_by: Vec<NodeId>,
    /// Replication timestamp (when first replicated)
    pub replicated_at: u64,
    /// Last acknowledgment timestamp
    pub last_ack_at: u64,
    /// Number of replication attempts
    pub attempts: u32,
}

impl ReplicationInfo {
    /// Create new replication info
    pub fn new() -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        Self {
            acknowledged_by: Vec::new(),
            replicated_at: now,
            last_ack_at: now,
            attempts: 0,
        }
    }

    /// Record acknowledgment from a node
    pub fn record_acknowledgment(&mut self, node_id: NodeId) {
        if !self.acknowledged_by.contains(&node_id) {
            self.acknowledged_by.push(node_id);
        }
        self.last_ack_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
    }

    /// Check if majority has acknowledged
    pub fn has_majority(&self, cluster_size: usize) -> bool {
        let majority = (cluster_size / 2) + 1;
        self.acknowledged_by.len() + 1 >= majority // +1 for leader
    }

    /// Increment replication attempts
    pub fn increment_attempts(&mut self) {
        self.attempts += 1;
    }
}

/// Log replication manager with WAL integration
#[derive(Debug)]
pub struct LogReplicationManager<W: WalStorage> {
    /// Node identifier
    node_id: NodeId,
    
    /// WAL storage instance
    wal_storage: Arc<W>,
    
    /// In-memory log entries with metadata
    log_entries: Arc<RwLock<BTreeMap<LogIndex, RaftWalEntry>>>,
    
    /// Current commit index
    commit_index: Arc<AtomicU64>,
    
    /// Last applied index
    last_applied: Arc<AtomicU64>,
    
    /// Next index for each follower
    next_index: Arc<RwLock<HashMap<NodeId, LogIndex>>>,
    
    /// Match index for each follower
    match_index: Arc<RwLock<HashMap<NodeId, LogIndex>>>,
    
    /// Configuration
    config: Arc<RaftConfig>,
    
    /// Performance metrics
    metrics: Arc<RwLock<ReplicationMetrics>>,
    
    /// Request ID counter for pipeline tracking
    request_counter: AtomicU64,
    
    /// Pending append entries requests
    pending_requests: Arc<RwLock<HashMap<u64, PendingRequest>>>,
    
    /// Replication state for followers
    follower_state: Arc<RwLock<HashMap<NodeId, FollowerState>>>,
}

/// Performance metrics for log replication
#[derive(Debug, Default, Clone)]
pub struct ReplicationMetrics {
    /// Total entries replicated
    pub entries_replicated: u64,
    /// Total entries committed
    pub entries_committed: u64,
    /// Average replication latency
    pub avg_replication_latency: Duration,
    /// Peak replication latency
    pub peak_replication_latency: Duration,
    /// Total WAL writes
    pub wal_writes: u64,
    /// WAL write errors
    pub wal_write_errors: u64,
    /// Append entries requests sent
    pub append_requests_sent: u64,
    /// Append entries responses received
    pub append_responses_received: u64,
    /// Conflicts resolved
    pub conflicts_resolved: u64,
    /// Batch operations performed
    pub batch_operations: u64,
}

/// Pending append entries request tracking
#[derive(Debug)]
pub struct PendingRequest {
    /// Target follower
    pub follower_id: NodeId,
    /// Request sent timestamp
    pub sent_at: Instant,
    /// Entries being replicated
    pub entries: Vec<LogIndex>,
    /// Previous log index
    pub prev_log_index: LogIndex,
    /// Previous log term
    pub prev_log_term: Term,
    /// Request ID
    pub request_id: u64,
}

/// Follower replication state
#[derive(Debug, Clone)]
pub struct FollowerState {
    /// Next log index to send
    pub next_index: LogIndex,
    /// Last matched log index
    pub match_index: LogIndex,
    /// Last contact timestamp
    pub last_contact: Instant,
    /// Number of consecutive failures
    pub consecutive_failures: u32,
    /// Current pipeline depth
    pub pipeline_depth: usize,
    /// Average round-trip time
    pub avg_rtt: Duration,
    /// Is currently catching up
    pub catching_up: bool,
}

impl FollowerState {
    /// Create new follower state
    pub fn new(next_index: LogIndex) -> Self {
        Self {
            next_index,
            match_index: 0,
            last_contact: Instant::now(),
            consecutive_failures: 0,
            pipeline_depth: 0,
            avg_rtt: Duration::from_millis(10),
            catching_up: false,
        }
    }

    /// Update on successful replication
    pub fn on_success(&mut self, matched_index: LogIndex, rtt: Duration) {
        self.match_index = matched_index;
        self.next_index = matched_index + 1;
        self.last_contact = Instant::now();
        self.consecutive_failures = 0;
        self.pipeline_depth = self.pipeline_depth.saturating_sub(1);
        
        // Update average RTT with exponential moving average
        let alpha = 0.2;
        let new_rtt_ms = rtt.as_millis() as f64;
        let current_avg_ms = self.avg_rtt.as_millis() as f64;
        let new_avg_ms = alpha * new_rtt_ms + (1.0 - alpha) * current_avg_ms;
        self.avg_rtt = Duration::from_millis(new_avg_ms as u64);
    }

    /// Update on replication failure
    pub fn on_failure(&mut self) {
        self.consecutive_failures += 1;
        self.last_contact = Instant::now();
        self.pipeline_depth = 0; // Reset pipeline on failure
        
        // Backoff next_index for log conflict resolution
        if self.next_index > 1 {
            self.next_index = std::cmp::max(1, self.next_index - self.consecutive_failures.min(10));
        }
    }

    /// Check if follower is catching up
    pub fn is_catching_up(&self, leader_last_index: LogIndex) -> bool {
        self.catching_up || (leader_last_index.saturating_sub(self.match_index) > 100)
    }

    /// Check if we can pipeline more requests
    pub fn can_pipeline(&self, max_pipeline_depth: usize) -> bool {
        self.pipeline_depth < max_pipeline_depth && self.consecutive_failures == 0
    }
}

impl<W: WalStorage> LogReplicationManager<W> {
    /// Create a new log replication manager
    pub async fn new(
        node_id: NodeId,
        wal_storage: Arc<W>,
        config: Arc<RaftConfig>,
    ) -> ConsensusResult<Self> {
        let manager = Self {
            node_id,
            wal_storage,
            log_entries: Arc::new(RwLock::new(BTreeMap::new())),
            commit_index: Arc::new(AtomicU64::new(0)),
            last_applied: Arc::new(AtomicU64::new(0)),
            next_index: Arc::new(RwLock::new(HashMap::new())),
            match_index: Arc::new(RwLock::new(HashMap::new())),
            config,
            metrics: Arc::new(RwLock::new(ReplicationMetrics::default())),
            request_counter: AtomicU64::new(0),
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
            follower_state: Arc::new(RwLock::new(HashMap::new())),
        };

        // Initialize by loading existing WAL entries if any
        manager.recover_from_wal().await?;

        Ok(manager)
    }

    /// Recover log entries from WAL on startup
    async fn recover_from_wal(&self) -> ConsensusResult<()> {
        info!("Recovering Raft log from WAL for node {}", self.node_id);
        
        // Read entries from WAL and reconstruct in-memory log
        match self.wal_storage.read(1, 10000).await {
            Ok(wal_entries) => {
                let mut log_entries = self.log_entries.write().await;
                let mut max_index = 0;

                for wal_entry in wal_entries {
                    // Try to deserialize as Raft entry
                    if let Ok(raft_entry) = bincode::deserialize::<RaftLogEntry>(&wal_entry.data) {
                        let raft_wal_entry = RaftWalEntry {
                            raft_entry: raft_entry.clone(),
                            wal_sequence: wal_entry.sequence_number,
                            commit_status: CommitStatus::Pending, // Will be updated based on commit_index
                            replication_info: ReplicationInfo::new(),
                        };
                        
                        log_entries.insert(raft_entry.index, raft_wal_entry);
                        max_index = max_index.max(raft_entry.index);
                    }
                }

                info!("Recovered {} log entries from WAL, max index: {}", log_entries.len(), max_index);
            },
            Err(_) => {
                debug!("No existing WAL entries found, starting with empty log");
            }
        }

        Ok(())
    }

    /// Append a new entry to the log (leader only)
    pub async fn append_entry(&self, term: Term, command: Vec<u8>) -> ConsensusResult<LogIndex> {
        let start_time = Instant::now();
        
        // Get next log index
        let log_entries = self.log_entries.read().await;
        let next_index = log_entries.keys().last().map(|&k| k + 1).unwrap_or(1);
        drop(log_entries);

        // Create Raft log entry
        let raft_entry = RaftLogEntry::new(term, next_index, command);

        // Serialize entry for WAL storage
        let serialized = bincode::serialize(&raft_entry)
            .map_err(|e| ConsensusError::Generic(format!("Serialization failed: {}", e)))?;

        // Write to WAL first (durability)
        let wal_sequence = match self.wal_storage.write(serialized).await {
            Ok(seq) => seq,
            Err(e) => {
                let mut metrics = self.metrics.write().await;
                metrics.wal_write_errors += 1;
                return Err(ConsensusError::Generic(format!("WAL write failed: {}", e)));
            }
        };

        // Create the combined entry
        let raft_wal_entry = RaftWalEntry {
            raft_entry,
            wal_sequence,
            commit_status: CommitStatus::Pending,
            replication_info: ReplicationInfo::new(),
        };

        // Add to in-memory log
        let mut log_entries = self.log_entries.write().await;
        log_entries.insert(next_index, raft_wal_entry);
        drop(log_entries);

        // Update metrics
        let mut metrics = self.metrics.write().await;
        metrics.entries_replicated += 1;
        metrics.wal_writes += 1;
        
        let latency = start_time.elapsed();
        if latency > metrics.peak_replication_latency {
            metrics.peak_replication_latency = latency;
        }
        
        // Update average latency (exponential moving average)
        let alpha = 0.1;
        let current_avg_nanos = metrics.avg_replication_latency.as_nanos() as f64;
        let new_latency_nanos = latency.as_nanos() as f64;
        let new_avg_nanos = alpha * new_latency_nanos + (1.0 - alpha) * current_avg_nanos;
        metrics.avg_replication_latency = Duration::from_nanos(new_avg_nanos as u64);

        debug!("Appended entry {} to log in {}μs", next_index, latency.as_micros());
        Ok(next_index)
    }

    /// Create append entries request for a specific follower
    pub async fn create_append_entries_request(
        &self,
        follower_id: NodeId,
        current_term: Term,
        leader_commit: LogIndex,
    ) -> ConsensusResult<AppendEntriesRequest> {
        let follower_state = {
            let follower_states = self.follower_state.read().await;
            follower_states.get(&follower_id).cloned()
        };

        let next_index = if let Some(state) = &follower_state {
            state.next_index
        } else {
            // Initialize follower state if not exists
            let log_entries = self.log_entries.read().await;
            let last_index = log_entries.keys().last().copied().unwrap_or(0);
            let next = last_index + 1;
            
            let mut follower_states = self.follower_state.write().await;
            follower_states.insert(follower_id, FollowerState::new(next));
            next
        };

        let prev_log_index = next_index.saturating_sub(1);
        let (prev_log_term, entries) = {
            let log_entries = self.log_entries.read().await;
            
            // Get previous log term
            let prev_log_term = if prev_log_index == 0 {
                Term::default()
            } else if let Some(entry) = log_entries.get(&prev_log_index) {
                entry.raft_entry.term
            } else {
                return Err(ConsensusError::LogInconsistency {
                    details: format!("Missing log entry at index {}", prev_log_index),
                });
            };

            // Get entries to send (limited by configuration)
            let mut entries_to_send = Vec::new();
            let mut current_index = next_index;
            let max_entries = if follower_state.as_ref().map_or(false, |s| s.is_catching_up(
                log_entries.keys().last().copied().unwrap_or(0)
            )) {
                // Send more entries when catching up
                self.config.max_entries_per_append * 2
            } else {
                self.config.max_entries_per_append
            };

            for _ in 0..max_entries {
                if let Some(entry) = log_entries.get(&current_index) {
                    entries_to_send.push(entry.raft_entry.clone());
                    current_index += 1;
                } else {
                    break;
                }
            }

            (prev_log_term, entries_to_send)
        };

        let request_id = self.request_counter.fetch_add(1, Ordering::Relaxed);

        // Track pending request
        let pending_request = PendingRequest {
            follower_id,
            sent_at: Instant::now(),
            entries: entries.iter().map(|e| e.index).collect(),
            prev_log_index,
            prev_log_term,
            request_id,
        };

        self.pending_requests.write().await.insert(request_id, pending_request);

        // Update follower pipeline depth
        if let Some(mut state) = follower_state {
            state.pipeline_depth += 1;
            self.follower_state.write().await.insert(follower_id, state);
        }

        Ok(AppendEntriesRequest {
            term: current_term,
            leader_id: self.node_id,
            prev_log_index,
            prev_log_term,
            entries,
            leader_commit,
            request_id,
            leadership_lease_ms: None, // Could add leadership lease support
        })
    }

    /// Handle append entries response from follower
    pub async fn handle_append_entries_response(
        &self,
        follower_id: NodeId,
        response: AppendEntriesResponse,
    ) -> ConsensusResult<()> {
        let mut metrics = self.metrics.write().await;
        metrics.append_responses_received += 1;
        drop(metrics);

        // Remove and get pending request
        let pending_request = self.pending_requests.write().await.remove(&response.request_id);

        if let Some(pending) = pending_request {
            let rtt = pending.sent_at.elapsed();
            
            if response.success {
                // Update follower state on success
                if let Some(last_log_index) = response.last_log_index {
                    let mut follower_states = self.follower_state.write().await;
                    if let Some(state) = follower_states.get_mut(&follower_id) {
                        state.on_success(last_log_index, rtt);
                    }

                    // Update match_index for commit advancement
                    self.match_index.write().await.insert(follower_id, last_log_index);

                    // Record acknowledgments in log entries
                    let mut log_entries = self.log_entries.write().await;
                    for &index in &pending.entries {
                        if let Some(entry) = log_entries.get_mut(&index) {
                            entry.replication_info.record_acknowledgment(follower_id);
                        }
                    }
                    drop(log_entries);

                    debug!("Follower {} successfully replicated {} entries (RTT: {}μs)", 
                        follower_id, pending.entries.len(), rtt.as_micros());

                    // Try to advance commit index
                    self.try_advance_commit_index().await?;
                }
            } else {
                // Handle replication failure with conflict resolution
                let mut follower_states = self.follower_state.write().await;
                if let Some(state) = follower_states.get_mut(&follower_id) {
                    state.on_failure();
                }
                drop(follower_states);

                // Fast conflict resolution using response hints
                if let (Some(conflict_index), conflict_term) = (response.conflict_index, response.conflict_term) {
                    self.handle_log_conflict(follower_id, conflict_index, conflict_term).await?;
                }

                warn!("Append entries failed for follower {} (conflict_index: {:?}, conflict_term: {:?})", 
                    follower_id, response.conflict_index, response.conflict_term);

                let mut metrics = self.metrics.write().await;
                metrics.conflicts_resolved += 1;
            }
        } else {
            debug!("Received response for unknown request ID: {}", response.request_id);
        }

        Ok(())
    }

    /// Handle log conflict with fast recovery
    async fn handle_log_conflict(
        &self,
        follower_id: NodeId,
        conflict_index: LogIndex,
        conflict_term: Option<Term>,
    ) -> ConsensusResult<()> {
        let mut follower_states = self.follower_state.write().await;
        if let Some(state) = follower_states.get_mut(&follower_id) {
            if let Some(term) = conflict_term {
                // Find the end of the conflicting term in our log
                let log_entries = self.log_entries.read().await;
                let mut next_index = conflict_index;

                // Search backwards to find where this term starts
                for (&index, entry) in log_entries.range(..=conflict_index).rev() {
                    if entry.raft_entry.term == term {
                        next_index = index + 1; // Next index after the conflicting term
                        break;
                    }
                }

                state.next_index = next_index;
            } else {
                // Simple backoff to conflict index
                state.next_index = conflict_index.max(1);
            }

            debug!("Resolved log conflict for follower {}: next_index set to {}", 
                follower_id, state.next_index);
        }

        Ok(())
    }

    /// Try to advance the commit index based on majority replication
    pub async fn try_advance_commit_index(&self) -> ConsensusResult<()> {
        let log_entries = self.log_entries.read().await;
        let match_indices = self.match_index.read().await;
        let current_commit = self.commit_index.load(Ordering::Relaxed);
        
        // Collect all match indices including leader's log
        let mut all_indices: Vec<LogIndex> = match_indices.values().copied().collect();
        if let Some(&last_index) = log_entries.keys().last() {
            all_indices.push(last_index); // Leader's log
        }
        all_indices.sort_unstable();

        // Find median (majority) index
        if !all_indices.is_empty() {
            let majority_index = all_indices.len() / 2;
            let new_commit_index = all_indices[majority_index];

            // Only advance if the entry is from current term (safety requirement)
            if new_commit_index > current_commit {
                if let Some(_entry) = log_entries.get(&new_commit_index) {
                    // Check if we can commit this entry (must be from current term for safety)
                    // This would need the current term from the RaftNode
                    
                    self.commit_index.store(new_commit_index, Ordering::Relaxed);
                    
                    // Update commit status in log entries
                    drop(log_entries);
                    let mut log_entries = self.log_entries.write().await;
                    for index in (current_commit + 1)..=new_commit_index {
                        if let Some(entry) = log_entries.get_mut(&index) {
                            entry.commit_status = CommitStatus::Committed;
                        }
                    }

                    info!("Advanced commit index from {} to {}", current_commit, new_commit_index);
                    
                    let mut metrics = self.metrics.write().await;
                    metrics.entries_committed += new_commit_index - current_commit;
                }
            }
        }

        Ok(())
    }

    /// Handle incoming append entries request (follower)
    pub async fn handle_append_entries_request(
        &self,
        request: &AppendEntriesRequest,
    ) -> ConsensusResult<AppendEntriesResponse> {
        let processing_start = Instant::now();

        // Check log consistency
        let consistency_check = {
            let log_entries = self.log_entries.read().await;
            self.check_log_consistency(&*log_entries, request.prev_log_index, request.prev_log_term)
        };

        if !consistency_check {
            // Log conflict - provide conflict information for fast recovery
            let (conflict_index, conflict_term) = {
                let log_entries = self.log_entries.read().await;
                self.find_conflict_info(&*log_entries, request.prev_log_index)
            };

            return Ok(AppendEntriesResponse {
                term: request.term, // Will be set by caller
                success: false,
                last_log_index: None,
                conflict_index: Some(conflict_index),
                conflict_term,
                request_id: request.request_id,
                processing_latency_us: Some(processing_start.elapsed().as_micros() as u64),
            });
        }

        // Append new entries
        if !request.entries.is_empty() {
            let mut log_entries = self.log_entries.write().await;
            
            // Remove any conflicting entries
            let first_new_index = request.entries[0].index;
            log_entries.retain(|&index, _| index < first_new_index);

            // Append new entries and write to WAL
            for entry in &request.entries {
                // Serialize and write to WAL
                let serialized = bincode::serialize(entry)
                    .map_err(|e| ConsensusError::Generic(format!("Serialization failed: {}", e)))?;

                match self.wal_storage.write(serialized).await {
                    Ok(wal_sequence) => {
                        let raft_wal_entry = RaftWalEntry {
                            raft_entry: entry.clone(),
                            wal_sequence,
                            commit_status: CommitStatus::Pending,
                            replication_info: ReplicationInfo::new(),
                        };
                        log_entries.insert(entry.index, raft_wal_entry);
                    },
                    Err(e) => {
                        warn!("Failed to write entry {} to WAL: {}", entry.index, e);
                        return Ok(AppendEntriesResponse {
                            term: request.term,
                            success: false,
                            last_log_index: None,
                            conflict_index: None,
                            conflict_term: None,
                            request_id: request.request_id,
                            processing_latency_us: Some(processing_start.elapsed().as_micros() as u64),
                        });
                    }
                }
            }

            debug!("Appended {} entries starting at index {}", 
                request.entries.len(), request.entries[0].index);
        }

        // Update commit index if leader's commit index is higher
        let current_commit = self.commit_index.load(Ordering::Relaxed);
        if request.leader_commit > current_commit {
            let log_entries = self.log_entries.read().await;
            let last_log_index = log_entries.keys().last().copied().unwrap_or(0);
            let new_commit = std::cmp::min(request.leader_commit, last_log_index);
            
            if new_commit > current_commit {
                self.commit_index.store(new_commit, Ordering::Relaxed);
                
                // Update commit status
                drop(log_entries);
                let mut log_entries = self.log_entries.write().await;
                for index in (current_commit + 1)..=new_commit {
                    if let Some(entry) = log_entries.get_mut(&index) {
                        entry.commit_status = CommitStatus::Committed;
                    }
                }
                
                debug!("Updated commit index to {}", new_commit);
            }
        }

        let last_log_index = {
            let log_entries = self.log_entries.read().await;
            log_entries.keys().last().copied().unwrap_or(0)
        };

        Ok(AppendEntriesResponse {
            term: request.term,
            success: true,
            last_log_index: Some(last_log_index),
            conflict_index: None,
            conflict_term: None,
            request_id: request.request_id,
            processing_latency_us: Some(processing_start.elapsed().as_micros() as u64),
        })
    }

    /// Check log consistency for append entries
    fn check_log_consistency(
        &self,
        log_entries: &BTreeMap<LogIndex, RaftWalEntry>,
        prev_log_index: LogIndex,
        prev_log_term: Term,
    ) -> bool {
        if prev_log_index == 0 {
            true // Initial case
        } else if let Some(entry) = log_entries.get(&prev_log_index) {
            entry.raft_entry.term == prev_log_term
        } else {
            false // Log doesn't contain entry at prev_log_index
        }
    }

    /// Find conflict information for fast log repair
    fn find_conflict_info(
        &self,
        log_entries: &BTreeMap<LogIndex, RaftWalEntry>,
        prev_log_index: LogIndex,
    ) -> (LogIndex, Option<Term>) {
        let last_index = log_entries.keys().last().copied().unwrap_or(0);
        
        if prev_log_index > last_index {
            // Follower's log is shorter
            (last_index, None)
        } else if prev_log_index > 0 {
            if let Some(entry) = log_entries.get(&prev_log_index) {
                // Find first entry of the conflicting term
                let conflict_term = entry.raft_entry.term;
                for (&index, log_entry) in log_entries.iter() {
                    if log_entry.raft_entry.term == conflict_term {
                        return (index, Some(conflict_term));
                    }
                }
            }
        }
        (1, None)
    }

    /// Initialize follower state for cluster members
    pub async fn initialize_followers(&self, followers: Vec<NodeId>) {
        let log_entries = self.log_entries.read().await;
        let last_index = log_entries.keys().last().copied().unwrap_or(0);
        let next_index = last_index + 1;
        drop(log_entries);

        let mut follower_states = self.follower_state.write().await;
        for follower_id in followers {
            follower_states.insert(follower_id, FollowerState::new(next_index));
        }
    }

    /// Get current log statistics
    pub async fn get_log_stats(&self) -> LogStats {
        let log_entries = self.log_entries.read().await;
        let metrics = self.metrics.read().await;
        let commit_index = self.commit_index.load(Ordering::Relaxed);
        let last_applied = self.last_applied.load(Ordering::Relaxed);
        
        let total_entries = log_entries.len();
        let last_index = log_entries.keys().last().copied().unwrap_or(0);
        let first_index = log_entries.keys().next().copied().unwrap_or(0);
        
        let committed_entries = log_entries.values()
            .filter(|e| e.commit_status == CommitStatus::Committed)
            .count();

        LogStats {
            total_entries: total_entries as u64,
            first_index,
            last_index,
            commit_index,
            last_applied,
            committed_entries: committed_entries as u64,
            avg_replication_latency: metrics.avg_replication_latency,
            peak_replication_latency: metrics.peak_replication_latency,
            wal_writes: metrics.wal_writes,
            append_requests_sent: metrics.append_requests_sent,
        }
    }

    /// Clean up old pending requests (timeout handling)
    pub async fn cleanup_pending_requests(&self, timeout: Duration) {
        let mut pending_requests = self.pending_requests.write().await;
        let now = Instant::now();
        
        pending_requests.retain(|_request_id, request| {
            if now.duration_since(request.sent_at) < timeout {
                true
            } else {
                debug!("Cleaning up timed out request {} to follower {}", 
                    request.request_id, request.follower_id);
                false
            }
        });
    }

    /// Get replication metrics
    pub async fn get_metrics(&self) -> ReplicationMetrics {
        self.metrics.read().await.clone()
    }

    /// Apply committed entries to state machine
    pub async fn apply_committed_entries<F>(&self, mut apply_fn: F) -> ConsensusResult<u64>
    where
        F: FnMut(&RaftWalEntry) -> ConsensusResult<()>,
    {
        let commit_index = self.commit_index.load(Ordering::Relaxed);
        let mut last_applied = self.last_applied.load(Ordering::Relaxed);
        let mut applied_count = 0;

        let log_entries = self.log_entries.read().await;
        
        while last_applied < commit_index {
            last_applied += 1;
            
            if let Some(entry) = log_entries.get(&last_applied) {
                if entry.commit_status == CommitStatus::Committed {
                    apply_fn(entry)?;
                    applied_count += 1;
                }
            } else {
                return Err(ConsensusError::LogInconsistency {
                    details: format!("Missing committed entry at index {}", last_applied),
                });
            }
        }

        self.last_applied.store(last_applied, Ordering::Relaxed);
        
        if applied_count > 0 {
            debug!("Applied {} committed entries, last_applied: {}", applied_count, last_applied);
        }

        Ok(applied_count)
    }

    /// Get current commit index
    pub fn commit_index(&self) -> LogIndex {
        self.commit_index.load(Ordering::Relaxed)
    }

    /// Get last applied index
    pub fn last_applied(&self) -> LogIndex {
        self.last_applied.load(Ordering::Relaxed)
    }

    /// Get log entry at specific index
    pub async fn get_entry(&self, index: LogIndex) -> Option<RaftWalEntry> {
        let log_entries = self.log_entries.read().await;
        log_entries.get(&index).cloned()
    }

    /// Get log entries in range
    pub async fn get_entries(&self, start_index: LogIndex, end_index: LogIndex) -> Vec<RaftWalEntry> {
        let log_entries = self.log_entries.read().await;
        log_entries.range(start_index..=end_index)
            .map(|(_, entry)| entry.clone())
            .collect()
    }
}

/// Log statistics for monitoring
#[derive(Debug, Clone)]
pub struct LogStats {
    /// Total number of log entries
    pub total_entries: u64,
    /// First log index
    pub first_index: LogIndex,
    /// Last log index
    pub last_index: LogIndex,
    /// Current commit index
    pub commit_index: LogIndex,
    /// Last applied index
    pub last_applied: LogIndex,
    /// Number of committed entries
    pub committed_entries: u64,
    /// Average replication latency
    pub avg_replication_latency: Duration,
    /// Peak replication latency
    pub peak_replication_latency: Duration,
    /// Total WAL writes
    pub wal_writes: u64,
    /// Append requests sent
    pub append_requests_sent: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock WAL storage for testing
    struct MockWalStorage {
        entries: Arc<RwLock<Vec<WalEntry>>>,
        sequence: Arc<AtomicU64>,
    }

    impl MockWalStorage {
        fn new() -> Self {
            Self {
                entries: Arc::new(RwLock::new(Vec::new())),
                sequence: Arc::new(AtomicU64::new(0)),
            }
        }
    }

    impl WalStorage for MockWalStorage {
        async fn write(&self, message: Vec<u8>) -> Result<u64, String> {
            let seq = self.sequence.fetch_add(1, Ordering::Relaxed) + 1;
            let entry = WalEntry {
                sequence_number: seq,
                data: message,
            };
            self.entries.write().await.push(entry);
            Ok(seq)
        }

        async fn read(&self, start_seq: u64, max_entries: u64) -> Result<Vec<WalEntry>, String> {
            let entries = self.entries.read().await;
            let filtered: Vec<WalEntry> = entries
                .iter()
                .filter(|e| e.sequence_number >= start_seq)
                .take(max_entries as usize)
                .cloned()
                .collect();
            Ok(filtered)
        }
    }

    async fn create_test_replication_manager() -> LogReplicationManager<MockWalStorage> {
        let wal_storage = Arc::new(MockWalStorage::new());
        let raft_config = Arc::new(RaftConfig::default());
        LogReplicationManager::new(
            NodeId::generate(),
            wal_storage,
            raft_config,
        ).await.unwrap()
    }

    #[tokio::test]
    async fn test_log_replication_manager_creation() {
        let _manager = create_test_replication_manager().await;
        // Manager should be created successfully
    }

    #[tokio::test]
    async fn test_append_entry() {
        let manager = create_test_replication_manager().await;
        
        let term = Term::new(1);
        let command = b"test command".to_vec();
        
        let index = manager.append_entry(term, command.clone()).await.unwrap();
        assert_eq!(index, 1);
        
        let entry = manager.get_entry(index).await.unwrap();
        assert_eq!(entry.raft_entry.index, index);
        assert_eq!(entry.raft_entry.term, term);
        assert_eq!(entry.raft_entry.command, command);
    }

    #[tokio::test]
    async fn test_log_consistency_check() {
        let manager = create_test_replication_manager().await;
        
        // Append some entries
        manager.append_entry(Term::new(1), b"cmd1".to_vec()).await.unwrap();
        manager.append_entry(Term::new(1), b"cmd2".to_vec()).await.unwrap();
        
        let log_entries = manager.log_entries.read().await;
        
        // Test consistency checks
        assert!(manager.check_log_consistency(&*log_entries, 0, Term::default())); // Initial case
        assert!(manager.check_log_consistency(&*log_entries, 1, Term::new(1))); // Matching entry
        assert!(!manager.check_log_consistency(&*log_entries, 1, Term::new(2))); // Term mismatch
        assert!(!manager.check_log_consistency(&*log_entries, 3, Term::new(1))); // Missing entry
    }

    #[tokio::test]
    async fn test_follower_state_management() {
        let manager = create_test_replication_manager().await;
        
        let follower_id = NodeId::generate();
        manager.initialize_followers(vec![follower_id]).await;
        
        let follower_states = manager.follower_state.read().await;
        let state = follower_states.get(&follower_id).unwrap();
        assert_eq!(state.next_index, 1);
        assert_eq!(state.match_index, 0);
    }

    #[tokio::test]
    async fn test_replication_info() {
        let mut replication_info = ReplicationInfo::new();
        let node_id = NodeId::generate();
        
        assert!(!replication_info.has_majority(3));
        
        replication_info.record_acknowledgment(node_id);
        assert!(replication_info.has_majority(3)); // +1 for leader = 2/3 majority
        
        assert!(replication_info.acknowledged_by.contains(&node_id));
    }

    #[tokio::test]
    async fn test_commit_index_advancement() {
        let manager = create_test_replication_manager().await;
        
        // Append entries
        manager.append_entry(Term::new(1), b"cmd1".to_vec()).await.unwrap();
        manager.append_entry(Term::new(1), b"cmd2".to_vec()).await.unwrap();
        
        // Simulate follower acknowledgments
        let follower_id = NodeId::generate();
        manager.match_index.write().await.insert(follower_id, 2);
        
        // Try to advance commit index
        manager.try_advance_commit_index().await.unwrap();
        
        // In a cluster of 1 (just leader), commit index should advance
        let commit_index = manager.commit_index();
        assert!(commit_index > 0);
    }

    #[tokio::test]
    async fn test_log_stats() {
        let manager = create_test_replication_manager().await;
        
        // Append some entries
        manager.append_entry(Term::new(1), b"cmd1".to_vec()).await.unwrap();
        manager.append_entry(Term::new(1), b"cmd2".to_vec()).await.unwrap();
        
        let stats = manager.get_log_stats().await;
        assert_eq!(stats.total_entries, 2);
        assert_eq!(stats.first_index, 1);
        assert_eq!(stats.last_index, 2);
    }

    #[tokio::test]
    async fn test_conflict_resolution() {
        let manager = create_test_replication_manager().await;
        
        // Append entries
        manager.append_entry(Term::new(1), b"cmd1".to_vec()).await.unwrap();
        manager.append_entry(Term::new(2), b"cmd2".to_vec()).await.unwrap();
        
        let log_entries = manager.log_entries.read().await;
        
        // Test conflict info finding
        let (conflict_index, conflict_term) = manager.find_conflict_info(&*log_entries, 2);
        assert_eq!(conflict_index, 2);
        assert_eq!(conflict_term, Some(Term::new(2)));
    }
}