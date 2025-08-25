//! Storage backend implementations
//!
//! This module provides various storage backend implementations optimized for
//! different deployment scenarios and performance requirements.

#[cfg(feature = "memory-backend")]
pub mod memory;

// File backend will be implemented in future phases
// #[cfg(feature = "file-backend")]
// pub mod file;

// Distributed backend will be implemented in future phases
// #[cfg(feature = "distributed-backend")]
// pub mod distributed;
