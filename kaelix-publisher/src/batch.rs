//! Message batching utilities for improved performance.

use kaelix_core::Message;

/// Message batch for efficient publishing.
#[derive(Debug)]
pub struct MessageBatch {
    messages: Vec<Message>,
    max_size: usize,
}

impl MessageBatch {
    /// Create a new message batch.
    #[must_use]
    pub fn new(max_size: usize) -> Self {
        Self { messages: Vec::with_capacity(max_size), max_size }
    }

    /// Add a message to the batch.
    pub fn add(&mut self, message: Message) -> bool {
        if self.messages.len() < self.max_size {
            self.messages.push(message);
            true
        } else {
            false
        }
    }

    /// Check if the batch is full.
    #[must_use]
    pub const fn is_full(&self) -> bool {
        self.messages.len() >= self.max_size
    }

    /// Check if the batch is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Get the number of messages in the batch.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.messages.len()
    }

    /// Clear the batch and return the messages.
    pub fn drain(&mut self) -> Vec<Message> {
        self.messages.drain(..).collect()
    }
}
