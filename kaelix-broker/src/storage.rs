//! Storage and persistence layer for messages.

use kaelix_core::{Message, Result};

/// Trait for message storage backends.
pub trait Storage: Send + Sync {
    /// Store a message.
    fn store(&self, message: &Message) -> Result<()>;
    
    /// Retrieve messages by topic and offset range.
    fn retrieve(&self, topic: &str, start_offset: u64, end_offset: u64) -> Result<Vec<Message>>;
}

/// In-memory storage implementation for testing.
#[derive(Debug, Default)]
pub struct MemoryStorage {
    // TODO: Implement in-memory storage
}

impl Storage for MemoryStorage {
    fn store(&self, _message: &Message) -> Result<()> {
        // TODO: Implement
        Ok(())
    }
    
    fn retrieve(&self, _topic: &str, _start_offset: u64, _end_offset: u64) -> Result<Vec<Message>> {
        // TODO: Implement
        Ok(Vec::new())
    }
}