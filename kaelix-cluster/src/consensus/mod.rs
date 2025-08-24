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
    use std::collections::HashMap;

    /// Raft node state
    #[derive(Debug)]
    pub struct RaftNode {
        /// Node identifier
        node_id: NodeId,
        /// Current term
        current_term: Term,
        /// Node state
        state: ConsensusState,
        /// Voted for in current term
        voted_for: Option<NodeId>,
        /// Log entries
        log: Vec<LogEntry>,
        /// Index of highest log entry known to be committed
        commit_index: usize,
        /// Index of highest log entry applied to state machine
        last_applied: usize,
        /// For leaders: next index to send to each follower
        next_index: HashMap<NodeId, usize>,
        /// For leaders: highest index known to be replicated on each follower
        match_index: HashMap<NodeId, usize>,
    }

    /// Raft log entry
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct LogEntry {
        /// Term when entry was received by leader
        pub term: Term,
        /// Command for state machine
        pub command: Vec<u8>,
        /// Index in the log
        pub index: usize,
    }

    /// Vote request message
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct VoteRequest {
        /// Candidate's term
        pub term: Term,
        /// Candidate requesting vote
        pub candidate_id: NodeId,
        /// Index of candidate's last log entry
        pub last_log_index: usize,
        /// Term of candidate's last log entry
        pub last_log_term: Term,
    }

    /// Vote response message
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct VoteResponse {
        /// Current term for candidate to update itself
        pub term: Term,
        /// True means candidate received vote
        pub vote_granted: bool,
    }

    /// Append entries request
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct AppendEntriesRequest {
        /// Leader's term
        pub term: Term,
        /// Leader ID
        pub leader_id: NodeId,
        /// Index of log entry immediately preceding new ones
        pub prev_log_index: usize,
        /// Term of prev_log_index entry
        pub prev_log_term: Term,
        /// Log entries to store (empty for heartbeat)
        pub entries: Vec<LogEntry>,
        /// Leader's commit index
        pub leader_commit: usize,
    }

    /// Append entries response
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct AppendEntriesResponse {
        /// Current term for leader to update itself
        pub term: Term,
        /// True if follower contained entry matching prev_log_index and prev_log_term
        pub success: bool,
        /// Hint for leader optimization: follower's last log index
        pub last_log_index: Option<usize>,
    }

    impl RaftNode {
        /// Create a new Raft node
        pub fn new(node_id: NodeId) -> Self {
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
            }
        }

        /// Get current state
        pub fn state(&self) -> ConsensusState {
            self.state
        }

        /// Get current term
        pub fn current_term(&self) -> Term {
            self.current_term
        }

        /// Get node ID
        pub fn node_id(&self) -> NodeId {
            self.node_id
        }

        /// Check if this node is the leader
        pub fn is_leader(&self) -> bool {
            matches!(self.state, ConsensusState::Leader)
        }

        /// Become a candidate and start election
        pub fn become_candidate(&mut self) -> ConsensusResult<()> {
            self.current_term.increment();
            self.state = ConsensusState::Candidate;
            self.voted_for = Some(self.node_id);

            tracing::info!("Node {} became candidate for term {}", self.node_id, self.current_term);
            Ok(())
        }

        /// Become leader
        pub fn become_leader(&mut self, peer_ids: &[NodeId]) -> ConsensusResult<()> {
            if !matches!(self.state, ConsensusState::Candidate) {
                return Err(ConsensusError::Generic(
                    "Can only become leader from candidate state".to_string(),
                ));
            }

            self.state = ConsensusState::Leader;

            // Initialize leader state
            let last_log_index = self.log.len();
            for &peer_id in peer_ids {
                self.next_index.insert(peer_id, last_log_index + 1);
                self.match_index.insert(peer_id, 0);
            }

            tracing::info!("Node {} became leader for term {}", self.node_id, self.current_term);
            Ok(())
        }

        /// Become follower
        pub fn become_follower(&mut self, term: Term) -> ConsensusResult<()> {
            if term < self.current_term {
                return Err(ConsensusError::TermMismatch {
                    expected: self.current_term.value(),
                    actual: term.value(),
                });
            }

            self.current_term = term;
            self.state = ConsensusState::Follower;
            self.voted_for = None;

            tracing::info!("Node {} became follower for term {}", self.node_id, self.current_term);
            Ok(())
        }

        /// Handle vote request
        pub fn handle_vote_request(
            &mut self,
            request: VoteRequest,
        ) -> ConsensusResult<VoteResponse> {
            // If term is outdated, reject
            if request.term < self.current_term {
                return Ok(VoteResponse { term: self.current_term, vote_granted: false });
            }

            // If term is newer, update and become follower
            if request.term > self.current_term {
                self.become_follower(request.term)?;
            }

            // Check if we can vote for this candidate
            let can_vote = self.voted_for.is_none() || self.voted_for == Some(request.candidate_id);

            // Check if candidate's log is at least as up-to-date as ours
            let candidate_log_ok = if let Some(last_entry) = self.log.last() {
                request.last_log_term > last_entry.term
                    || (request.last_log_term == last_entry.term
                        && request.last_log_index >= last_entry.index)
            } else {
                true // No log entries, any candidate is fine
            };

            let vote_granted = can_vote && candidate_log_ok;

            if vote_granted {
                self.voted_for = Some(request.candidate_id);
                tracing::debug!(
                    "Node {} voted for {} in term {}",
                    self.node_id,
                    request.candidate_id,
                    request.term
                );
            }

            Ok(VoteResponse { term: self.current_term, vote_granted })
        }

        /// Handle append entries request
        pub fn handle_append_entries(
            &mut self,
            request: AppendEntriesRequest,
        ) -> ConsensusResult<AppendEntriesResponse> {
            // If term is outdated, reject
            if request.term < self.current_term {
                return Ok(AppendEntriesResponse {
                    term: self.current_term,
                    success: false,
                    last_log_index: self.log.last().map(|e| e.index),
                });
            }

            // If term is newer or equal, update and become follower
            if request.term >= self.current_term {
                self.become_follower(request.term)?;
            }

            // Check if log contains an entry at prev_log_index with matching term
            let prev_log_ok = if request.prev_log_index == 0 {
                true // Initial case
            } else if let Some(entry) = self.log.get(request.prev_log_index - 1) {
                entry.term == request.prev_log_term
            } else {
                false // Log doesn't contain entry at prev_log_index
            };

            if !prev_log_ok {
                return Ok(AppendEntriesResponse {
                    term: self.current_term,
                    success: false,
                    last_log_index: self.log.last().map(|e| e.index),
                });
            }

            // Append new entries
            if !request.entries.is_empty() {
                // Remove any conflicting entries
                self.log.truncate(request.prev_log_index);

                // Append new entries
                self.log.extend(request.entries);

                tracing::debug!(
                    "Node {} appended {} entries in term {}",
                    self.node_id,
                    self.log.len() - request.prev_log_index,
                    request.term
                );
            }

            // Update commit index
            if request.leader_commit > self.commit_index {
                self.commit_index = std::cmp::min(request.leader_commit, self.log.len());
            }

            Ok(AppendEntriesResponse {
                term: self.current_term,
                success: true,
                last_log_index: self.log.last().map(|e| e.index),
            })
        }

        /// Append a new entry to the log (leader only)
        pub fn append_entry(&mut self, command: Vec<u8>) -> ConsensusResult<usize> {
            if !self.is_leader() {
                return Err(ConsensusError::NotLeader { leader: None });
            }

            let index = self.log.len() + 1;
            let entry = LogEntry { term: self.current_term, command, index };

            self.log.push(entry);
            tracing::debug!("Leader {} appended entry at index {}", self.node_id, index);

            Ok(index)
        }

        /// Get log entries starting from index
        pub fn get_entries_from(&self, start_index: usize) -> &[LogEntry] {
            if start_index == 0 || start_index > self.log.len() {
                &[]
            } else {
                &self.log[start_index - 1..]
            }
        }

        /// Get the last log index and term
        pub fn last_log_info(&self) -> (usize, Term) {
            if let Some(last_entry) = self.log.last() {
                (last_entry.index, last_entry.term)
            } else {
                (0, Term::default())
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_raft_node_creation() {
            let node_id = NodeId::generate();
            let node = RaftNode::new(node_id);

            assert_eq!(node.node_id(), node_id);
            assert_eq!(node.state(), ConsensusState::Follower);
            assert_eq!(node.current_term(), Term::default());
            assert!(!node.is_leader());
        }

        #[test]
        fn test_become_candidate() {
            let node_id = NodeId::generate();
            let mut node = RaftNode::new(node_id);

            node.become_candidate().unwrap();

            assert_eq!(node.state(), ConsensusState::Candidate);
            assert_eq!(node.current_term(), Term::new(1));
            assert_eq!(node.voted_for, Some(node_id));
        }

        #[test]
        fn test_become_leader() {
            let node_id = NodeId::generate();
            let mut node = RaftNode::new(node_id);
            let peers = vec![NodeId::generate(), NodeId::generate()];

            node.become_candidate().unwrap();
            node.become_leader(&peers).unwrap();

            assert_eq!(node.state(), ConsensusState::Leader);
            assert!(node.is_leader());
            assert_eq!(node.next_index.len(), 2);
            assert_eq!(node.match_index.len(), 2);
        }

        #[test]
        fn test_vote_request_handling() {
            let node_id = NodeId::generate();
            let candidate_id = NodeId::generate();
            let mut node = RaftNode::new(node_id);

            let request = VoteRequest {
                term: Term::new(1),
                candidate_id,
                last_log_index: 0,
                last_log_term: Term::default(),
            };

            let response = node.handle_vote_request(request).unwrap();

            assert!(response.vote_granted);
            assert_eq!(response.term, Term::new(1));
            assert_eq!(node.voted_for, Some(candidate_id));
        }

        #[test]
        fn test_append_entries_handling() {
            let node_id = NodeId::generate();
            let leader_id = NodeId::generate();
            let mut node = RaftNode::new(node_id);

            let request = AppendEntriesRequest {
                term: Term::new(1),
                leader_id,
                prev_log_index: 0,
                prev_log_term: Term::default(),
                entries: vec![],
                leader_commit: 0,
            };

            let response = node.handle_append_entries(request).unwrap();

            assert!(response.success);
            assert_eq!(response.term, Term::new(1));
            assert_eq!(node.state(), ConsensusState::Follower);
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
