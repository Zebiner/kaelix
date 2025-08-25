//! # Core Types
//!
//! Fundamental data types used throughout the cluster system.

use serde::{Deserialize, Serialize};
use std::{
    fmt::{self, Display},
    sync::atomic::{AtomicU64, Ordering},
};
use uuid::Uuid;

/// Unique identifier for a node in the cluster
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(Uuid);

impl NodeId {
    /// Generate a new unique node ID
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a node ID from a UUID
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Get the underlying UUID
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }

    /// Convert to bytes
    pub fn to_bytes(&self) -> [u8; 16] {
        *self.0.as_bytes()
    }

    /// Create from bytes
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(Uuid::from_bytes(bytes))
    }
}

impl Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for NodeId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl From<NodeId> for Uuid {
    fn from(node_id: NodeId) -> Self {
        node_id.0
    }
}

/// Session identifier for tracking client sessions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(u64);

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);

impl SessionId {
    /// Generate a new unique session ID
    pub fn generate() -> Self {
        Self(SESSION_COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    /// Create a session ID from a u64
    pub fn from_u64(id: u64) -> Self {
        Self(id)
    }

    /// Get the underlying u64
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u64> for SessionId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

impl From<SessionId> for u64 {
    fn from(session_id: SessionId) -> Self {
        session_id.0
    }
}

/// Term number used in consensus algorithms
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Term(u64);

impl Term {
    /// Create a new term with the given value
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    /// Get the term value
    pub fn value(&self) -> u64 {
        self.0
    }

    /// Increment the term
    pub fn increment(&mut self) {
        self.0 += 1;
    }

    /// Get the next term without modifying this one
    pub fn next(&self) -> Self {
        Self(self.0 + 1)
    }
}

impl Default for Term {
    fn default() -> Self {
        Self(0)
    }
}

impl Display for Term {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u64> for Term {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<Term> for u64 {
    fn from(term: Term) -> Self {
        term.0
    }
}

/// Log index type for consensus algorithms
pub type LogIndex = u64;

/// Priority levels for message handling
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    /// Critical system messages (highest priority)
    Critical = 0,
    /// High priority messages
    High = 1,
    /// Normal priority messages
    Normal = 2,
    /// Low priority messages (lowest priority)
    Low = 3,
}

impl Default for Priority {
    fn default() -> Self {
        Priority::Normal
    }
}

impl Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Priority::Critical => write!(f, "critical"),
            Priority::High => write!(f, "high"),
            Priority::Normal => write!(f, "normal"),
            Priority::Low => write!(f, "low"),
        }
    }
}

/// Message sequence number for ordering
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SequenceNumber(u64);

impl SequenceNumber {
    /// Create a new sequence number
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    /// Get the sequence number value
    pub fn value(&self) -> u64 {
        self.0
    }

    /// Get the next sequence number
    pub fn next(&self) -> Self {
        Self(self.0 + 1)
    }
}

impl Default for SequenceNumber {
    fn default() -> Self {
        Self(0)
    }
}

impl Display for SequenceNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u64> for SequenceNumber {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<SequenceNumber> for u64 {
    fn from(seq: SequenceNumber) -> Self {
        seq.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_id_generation() {
        let id1 = NodeId::generate();
        let id2 = NodeId::generate();
        
        assert_ne!(id1, id2);
        
        // Test round-trip conversion
        let bytes = id1.to_bytes();
        let id1_restored = NodeId::from_bytes(bytes);
        assert_eq!(id1, id1_restored);
    }

    #[test]
    fn test_session_id_generation() {
        let id1 = SessionId::generate();
        let id2 = SessionId::generate();
        
        assert_ne!(id1, id2);
        assert!(id2.as_u64() > id1.as_u64());
    }

    #[test]
    fn test_term_operations() {
        let mut term = Term::new(5);
        assert_eq!(term.value(), 5);
        
        let next_term = term.next();
        assert_eq!(next_term.value(), 6);
        assert_eq!(term.value(), 5); // Original unchanged
        
        term.increment();
        assert_eq!(term.value(), 6);
    }

    #[test]
    fn test_term_ordering() {
        let term1 = Term::new(1);
        let term2 = Term::new(2);
        let term3 = Term::new(1);
        
        assert!(term1 < term2);
        assert!(term2 > term1);
        assert_eq!(term1, term3);
    }

    #[test]
    fn test_priority_ordering() {
        assert!(Priority::Critical < Priority::High);
        assert!(Priority::High < Priority::Normal);
        assert!(Priority::Normal < Priority::Low);
    }

    #[test]
    fn test_sequence_number() {
        let seq = SequenceNumber::new(10);
        assert_eq!(seq.value(), 10);
        
        let next_seq = seq.next();
        assert_eq!(next_seq.value(), 11);
    }
}