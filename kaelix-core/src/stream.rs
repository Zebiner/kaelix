//! Stream processing primitives and utilities.

use crate::{Message, Result};
use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

/// Trait for stream processors that can transform messages.
#[async_trait]
pub trait StreamProcessor: Send + Sync {
    /// Process a single message and return the transformed result.
    async fn process(&mut self, message: Message) -> Result<Vec<Message>>;

    /// Process a batch of messages for improved performance.
    async fn process_batch(&mut self, messages: Vec<Message>) -> Result<Vec<Message>> {
        let mut results = Vec::new();
        for message in messages {
            let mut processed = self.process(message).await?;
            results.append(&mut processed);
        }
        Ok(results)
    }
}

/// Stream that yields messages asynchronously.
pub type MessageStream = Pin<Box<dyn Stream<Item = Result<Message>> + Send>>;

/// Stream source that can produce messages.
#[async_trait]
pub trait StreamSource: Send + Sync {
    /// Create a new message stream.
    async fn create_stream(&self) -> Result<MessageStream>;

    /// Get the estimated number of pending messages.
    async fn pending_count(&self) -> Result<usize>;
}

/// Stream sink that can consume messages.
#[async_trait]
pub trait StreamSink: Send + Sync {
    /// Send a single message to the sink.
    async fn send(&mut self, message: Message) -> Result<()>;

    /// Send a batch of messages for improved performance.
    async fn send_batch(&mut self, messages: Vec<Message>) -> Result<()> {
        for message in messages {
            self.send(message).await?;
        }
        Ok(())
    }

    /// Flush any pending messages.
    async fn flush(&mut self) -> Result<()>;
}

/// Configuration for stream processing.
#[derive(Debug, Clone)]
pub struct StreamConfig {
    /// Maximum batch size for processing
    pub max_batch_size: usize,

    /// Buffer size for the stream
    pub buffer_size: usize,

    /// Processing timeout in milliseconds
    pub timeout_ms: u64,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self { max_batch_size: 1000, buffer_size: 10000, timeout_ms: 5000 }
    }
}
