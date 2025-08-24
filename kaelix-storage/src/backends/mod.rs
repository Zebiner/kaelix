//! Storage backend implementations
//!
//! This module provides various storage backend implementations optimized for
//! different deployment scenarios and performance requirements.

#[cfg(feature = "memory-backend")]
pub mod memory;

#[cfg(feature = "file-backend")]
pub mod file;

#[cfg(feature = "distributed-backend")]
pub mod distributed;