//! # Consensus Module
//!
//! Provides distributed consensus mechanisms including Raft implementation
//! for ensuring data consistency across cluster nodes.

use crate::types::{NodeId, Term};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info};

/// Consensus-related error types
#[derive(Error, Debug)]
pub enum ConsensusError {
    /// Network communication error
    #[error("Network error: {0}")]
    Network(String),

    /// Invalid state transition
    #[error("Invalid state transition: {0}")]
    InvalidState(String),

    /// Timeout error
    #[error("Operation timed out: {0}")]
    Timeout(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Generic consensus error
    #[error("Consensus error: {0}")]
    Generic(String),
}

pub type ConsensusResult<T> = Result<T, ConsensusError>;

/// Consensus state of a node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConsensusState {
    /// Node is a follower
    Follower,
    /// Node is a candidate for leader election
    Candidate,
    /// Node is the cluster leader
    Leader,
}

impl Default for ConsensusState {
    fn default() -> Self {
        Self::Follower
    }
}

/// Log entry for consensus operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Entry index
    pub index: u64,
    /// Term when entry was created
    pub term: Term,
    /// Entry data
    pub data: Vec<u8>,
}

impl LogEntry {
    /// Create a new log entry
    pub fn new(index: u64, term: Term, data: Vec<u8>) -> Self {
        Self { index, term, data }
    }

    /// Get the entry size in bytes
    pub fn size(&self) -> usize {
        std::mem::size_of::<u64>() + std::mem::size_of::<Term>() + self.data.len()
    }
}

/// Vote request message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteRequest {
    /// Candidate's term
    pub term: Term,
    /// Candidate requesting vote
    pub candidate_id: NodeId,
    /// Index of candidate's last log entry
    pub last_log_index: u64,
    /// Term of candidate's last log entry
    pub last_log_term: Term,
}

/// Vote response message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteResponse {
    /// Current term, for candidate to update itself
    pub term: Term,
    /// True means candidate received vote
    pub vote_granted: bool,
}

/// Append entries request message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppendEntriesRequest {
    /// Leader's term
    pub term: Term,
    /// Leader ID for followers to redirect clients
    pub leader_id: NodeId,
    /// Index of log entry immediately preceding new ones
    pub prev_log_index: u64,
    /// Term of prev_log_index entry
    pub prev_log_term: Term,
    /// Log entries to store (empty for heartbeat)
    pub entries: Vec<LogEntry>,
    /// Leader's commit index
    pub leader_commit: u64,
}

/// Append entries response message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppendEntriesResponse {
    /// Current term, for leader to update itself
    pub term: Term,
    /// True if follower contained entry matching prev_log_index and prev_log_term
    pub success: bool,
}

/// Consensus message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsensusMessage {
    /// Vote request
    VoteRequest(VoteRequest),
    /// Vote response
    VoteResponse(VoteResponse),
    /// Append entries request
    AppendEntriesRequest(AppendEntriesRequest),
    /// Append entries response
    AppendEntriesResponse(AppendEntriesResponse),
}

impl ConsensusMessage {
    /// Get the term associated with this message
    pub fn term(&self) -> Term {
        match self {
            Self::VoteRequest(req) => req.term,
            Self::VoteResponse(resp) => resp.term,
            Self::AppendEntriesRequest(req) => req.term,
            Self::AppendEntriesResponse(resp) => resp.term,
        }
    }
}

/// Configuration for consensus algorithms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusConfig {
    /// Election timeout range (min, max) in milliseconds
    pub election_timeout_ms: (u64, u64),
    /// Heartbeat interval in milliseconds
    pub heartbeat_interval_ms: u64,
    /// Maximum number of log entries in append entries message
    pub max_append_entries: usize,
    /// Snapshot threshold (number of log entries before snapshotting)
    pub snapshot_threshold: u64,
    /// Enable pre-vote optimization
    pub enable_pre_vote: bool,
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        Self {
            election_timeout_ms: (150, 300),
            heartbeat_interval_ms: 50,
            max_append_entries: 100,
            snapshot_threshold: 10000,
            enable_pre_vote: false,
        }
    }
}

/// Consensus manager coordinating distributed consensus
pub struct ConsensusManager {
    /// Node identifier
    node_id: NodeId,
    /// Current consensus state
    state: RwLock<ConsensusState>,
    /// Current term
    current_term: RwLock<Term>,
    /// Node voted for in current term
    voted_for: RwLock<Option<NodeId>>,
    /// Configuration
    config: ConsensusConfig,
}

impl ConsensusManager {
    /// Create a new consensus manager
    pub fn new(node_id: NodeId, config: ConsensusConfig) -> Self {
        Self {
            node_id,
            state: RwLock::new(ConsensusState::Follower),
            current_term: RwLock::new(Term::new(0)),
            voted_for: RwLock::new(None),
            config,
        }
    }

    /// Get current consensus state
    pub async fn state(&self) -> ConsensusState {
        *self.state.read().await
    }

    /// Get current term
    pub async fn current_term(&self) -> Term {
        *self.current_term.read().await
    }

    /// Get node ID
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Handle incoming consensus message
    pub async fn handle_message(&self, message: ConsensusMessage) -> ConsensusResult<()> {
        match message {
            ConsensusMessage::VoteRequest(req) => {
                self.handle_vote_request(req).await?;
            }
            ConsensusMessage::VoteResponse(resp) => {
                self.handle_vote_response(resp).await?;
            }
            ConsensusMessage::AppendEntriesRequest(req) => {
                self.handle_append_entries_request(req).await?;
            }
            ConsensusMessage::AppendEntriesResponse(resp) => {
                self.handle_append_entries_response(resp).await?;
            }
        }

        Ok(())
    }

    /// Start election process
    pub async fn start_election(&self) -> ConsensusResult<()> {
        info!("Starting election for node {}", self.node_id);

        // Increment current term and vote for self
        {
            let mut term = self.current_term.write().await;
            *term = Term::new(term.value() + 1);
        }

        {
            let mut voted_for = self.voted_for.write().await;
            *voted_for = Some(self.node_id);
        }

        // Transition to candidate state
        {
            let mut state = self.state.write().await;
            *state = ConsensusState::Candidate;
        }

        debug!("Node {} became candidate in term {}", self.node_id, self.current_term().await.value());

        Ok(())
    }

    /// Step down to follower state
    pub async fn step_down(&self, new_term: Option<Term>) -> ConsensusResult<()> {
        if let Some(term) = new_term {
            let mut current_term = self.current_term.write().await;
            if term.value() > current_term.value() {
                *current_term = term;
                let mut voted_for = self.voted_for.write().await;
                *voted_for = None;
            }
        }

        let mut state = self.state.write().await;
        if *state != ConsensusState::Follower {
            info!("Node {} stepping down to follower", self.node_id);
            *state = ConsensusState::Follower;
        }

        Ok(())
    }

    async fn handle_vote_request(&self, _req: VoteRequest) -> ConsensusResult<()> {
        // Implementation for handling vote requests
        debug!("Handling vote request");
        Ok(())
    }

    async fn handle_vote_response(&self, _resp: VoteResponse) -> ConsensusResult<()> {
        // Implementation for handling vote responses
        debug!("Handling vote response");
        Ok(())
    }

    async fn handle_append_entries_request(&self, _req: AppendEntriesRequest) -> ConsensusResult<()> {
        // Implementation for handling append entries requests
        debug!("Handling append entries request");
        Ok(())
    }

    async fn handle_append_entries_response(&self, _resp: AppendEntriesResponse) -> ConsensusResult<()> {
        // Implementation for handling append entries responses
        debug!("Handling append entries response");
        Ok(())
    }
}

/// Raft consensus engine implementation
pub struct RaftEngine {
    /// Base consensus manager
    consensus: ConsensusManager,
    /// Replicated log
    log: RwLock<Vec<LogEntry>>,
    /// Index of highest log entry applied to state machine
    last_applied: RwLock<u64>,
    /// Index of highest log entry known to be committed
    commit_index: RwLock<u64>,
    /// Message sender for outgoing messages
    message_sender: mpsc::UnboundedSender<ConsensusMessage>,
}

impl RaftEngine {
    /// Create a new Raft engine
    pub fn new(
        node_id: NodeId,
        config: ConsensusConfig,
        message_sender: mpsc::UnboundedSender<ConsensusMessage>,
    ) -> Self {
        Self {
            consensus: ConsensusManager::new(node_id, config),
            log: RwLock::new(Vec::new()),
            last_applied: RwLock::new(0),
            commit_index: RwLock::new(0),
            message_sender,
        }
    }

    /// Get current consensus state
    pub async fn state(&self) -> ConsensusResult<ConsensusState> {
        Ok(self.consensus.state().await)
    }

    /// Get current term
    pub async fn current_term(&self) -> ConsensusResult<Term> {
        Ok(self.consensus.current_term().await)
    }

    /// Get log length
    pub async fn log_length(&self) -> u64 {
        self.log.read().await.len() as u64
    }

    /// Get commit index
    pub async fn commit_index(&self) -> u64 {
        *self.commit_index.read().await
    }

    /// Append entry to log
    pub async fn append_log_entry(&self, term: Term, data: Vec<u8>) -> ConsensusResult<u64> {
        let mut log = self.log.write().await;
        let index = log.len() as u64;
        let entry = LogEntry::new(index, term, data);
        log.push(entry);
        Ok(index)
    }

    /// Handle consensus message
    pub async fn handle_message(&self, message: ConsensusMessage) -> ConsensusResult<()> {
        self.consensus.handle_message(message).await
    }

    /// Start election
    pub async fn start_election(&self) -> ConsensusResult<()> {
        self.consensus.start_election().await
    }

    /// Get engine statistics
    pub async fn stats(&self) -> EngineStats {
        EngineStats {
            current_term: self.consensus.current_term().await.value(),
            log_entries: self.log.read().await.len(),
            commit_index: *self.commit_index.read().await,
            last_applied: *self.last_applied.read().await,
            state: self.consensus.state().await,
        }
    }
}

/// Engine statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStats {
    /// Current term
    pub current_term: u64,
    /// Number of log entries
    pub log_entries: usize,
    /// Commit index
    pub commit_index: u64,
    /// Last applied index
    pub last_applied: u64,
    /// Current state
    pub state: ConsensusState,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_consensus_manager_creation() {
        let node_id = NodeId::generate();
        let config = ConsensusConfig::default();
        let manager = ConsensusManager::new(node_id, config);

        assert_eq!(manager.node_id(), node_id);
        assert_eq!(manager.state().await, ConsensusState::Follower);
        assert_eq!(manager.current_term().await.value(), 0);
    }

    #[tokio::test]
    async fn test_raft_engine_creation() {
        let node_id = NodeId::generate();
        let config = ConsensusConfig::default();
        let (tx, _rx) = mpsc::unbounded_channel();

        let engine = RaftEngine::new(node_id, config, tx);

        let state = engine.state().await.unwrap();
        assert_eq!(state, ConsensusState::Follower);

        let term = engine.current_term().await.unwrap();
        assert_eq!(term.value(), 0);

        let log_length = engine.log_length().await;
        assert_eq!(log_length, 0);
    }

    #[tokio::test]
    async fn test_log_entry_creation() {
        let entry = LogEntry::new(1, Term::new(1), vec![1, 2, 3]);
        assert_eq!(entry.index, 1);
        assert_eq!(entry.term.value(), 1);
        assert_eq!(entry.data, vec![1, 2, 3]);

        let size = entry.size();
        assert!(size > 0);
    }

    #[tokio::test]
    async fn test_consensus_message_term() {
        let vote_req = ConsensusMessage::VoteRequest(VoteRequest {
            term: Term::new(5),
            candidate_id: NodeId::generate(),
            last_log_index: 0,
            last_log_term: Term::new(0),
        });

        assert_eq!(vote_req.term().value(), 5);
    }

    #[tokio::test]
    async fn test_start_election() {
        let node_id = NodeId::generate();
        let config = ConsensusConfig::default();
        let (tx, _rx) = mpsc::unbounded_channel();
        let engine = RaftEngine::new(node_id, config, tx);

        // Start election
        engine.start_election().await.unwrap();

        let state = engine.state().await.unwrap();
        assert_eq!(state, ConsensusState::Candidate);

        let term = engine.current_term().await.unwrap();
        assert_eq!(term.value(), 1);
    }

    #[tokio::test]
    async fn test_append_log_entry() {
        let node_id = NodeId::generate();
        let config = ConsensusConfig::default();
        let (tx, _rx) = mpsc::unbounded_channel();
        let engine = RaftEngine::new(node_id, config, tx);

        let data = vec![1, 2, 3, 4];
        let term = Term::new(1);
        let index = engine.append_log_entry(term, data.clone()).await.unwrap();

        assert_eq!(index, 0);
        assert_eq!(engine.log_length().await, 1);

        let log = engine.log.read().await;
        assert_eq!(log[0].data, data);
        assert_eq!(log[0].term.value(), 1);
    }

    #[tokio::test]
    async fn test_step_down() {
        let node_id = NodeId::generate();
        let config = ConsensusConfig::default();
        let manager = ConsensusManager::new(node_id, config);

        // Start as candidate
        manager.start_election().await.unwrap();
        assert_eq!(manager.state().await, ConsensusState::Candidate);

        // Step down with higher term
        let new_term = Term::new(5);
        manager.step_down(Some(new_term)).await.unwrap();

        assert_eq!(manager.state().await, ConsensusState::Follower);
        assert_eq!(manager.current_term().await.value(), 5);
    }
}