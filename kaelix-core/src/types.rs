//! Common types used throughout the Kaelix system.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Timestamp type for message ordering and expiration.
pub type Timestamp = DateTime<Utc>;

/// Partition identifier for horizontal scaling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PartitionId(pub u32);

impl fmt::Display for PartitionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u32> for PartitionId {
    fn from(id: u32) -> Self {
        Self(id)
    }
}

impl Default for PartitionId {
    fn default() -> Self {
        Self(0)
    }
}

/// Message offset within a partition for ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Offset(pub u64);

impl fmt::Display for Offset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u64> for Offset {
    fn from(offset: u64) -> Self {
        Self(offset)
    }
}

impl Default for Offset {
    fn default() -> Self {
        Self(0)
    }
}

impl Offset {
    /// Create a new offset.
    pub const fn new(offset: u64) -> Self {
        Self(offset)
    }

    /// Get the next offset.
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }

    /// Get the raw offset value.
    pub const fn value(self) -> u64 {
        self.0
    }
}