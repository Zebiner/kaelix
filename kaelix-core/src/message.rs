//! Message types and utilities for the streaming system.

use crate::types::{Offset, PartitionId, Timestamp};
use bytes::Bytes;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;
use zeroize::Zeroize;

/// Unique identifier for a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(Uuid);

impl MessageId {
    /// Generate a new unique message ID.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a message ID from a UUID.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Get the underlying UUID.
    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Topic name for message routing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Topic(String);

impl Topic {
    /// Create a new topic.
    ///
    /// # Errors
    /// Returns an error if the topic name is empty or contains invalid characters.
    pub fn new(name: impl Into<String>) -> crate::Result<Self> {
        let name = name.into();
        if name.is_empty() {
            return Err(crate::Error::InvalidMessage {
                message: "Topic name cannot be empty".to_string(),
            });
        }

        if name.len() > 255 {
            return Err(crate::Error::InvalidMessage {
                message: "Topic name cannot exceed 255 characters".to_string(),
            });
        }

        // Validate topic name characters (alphanumeric, hyphens, underscores, dots)
        if !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.') {
            return Err(crate::Error::InvalidMessage {
                message: "Topic name contains invalid characters".to_string(),
            });
        }

        Ok(Self(name))
    }

    /// Get the topic name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Convert to owned string.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for Topic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for Topic {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Core message structure with zero-copy semantics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// Unique message identifier
    pub id: MessageId,

    /// Topic for message routing
    pub topic: Topic,

    /// Message payload (zero-copy)
    pub payload: Bytes,

    /// Message timestamp
    pub timestamp: Timestamp,

    /// Optional partition assignment
    pub partition: Option<PartitionId>,

    /// Optional message offset
    pub offset: Option<Offset>,

    /// Optional message headers
    pub headers: Option<std::collections::HashMap<String, String>>,
}

impl Message {
    /// Create a new message with the given topic and payload.
    ///
    /// # Errors
    /// Returns an error if the topic name is invalid.
    pub fn new(topic: impl Into<String>, payload: Bytes) -> crate::Result<Self> {
        Ok(Self {
            id: MessageId::new(),
            topic: Topic::new(topic)?,
            payload,
            timestamp: Utc::now(),
            partition: None,
            offset: None,
            headers: None,
        })
    }

    /// Create a message builder for more complex construction.
    #[must_use]
    pub fn builder() -> MessageBuilder {
        MessageBuilder::default()
    }

    /// Get the payload size in bytes.
    #[must_use]
    pub fn payload_size(&self) -> usize {
        self.payload.len()
    }

    /// Get the total message size estimate in bytes.
    #[must_use]
    pub fn estimated_size(&self) -> usize {
        let mut size = 0;
        size += std::mem::size_of::<MessageId>();
        size += self.topic.as_str().len();
        size += self.payload.len();
        size += std::mem::size_of::<Timestamp>();
        size += std::mem::size_of::<Option<PartitionId>>();
        size += std::mem::size_of::<Option<Offset>>();
        
        if let Some(ref headers) = self.headers {
            for (key, value) in headers {
                size += key.len() + value.len();
            }
        }
        
        size
    }

    /// Check if the message has headers.
    #[must_use]
    pub fn has_headers(&self) -> bool {
        self.headers.is_some()
    }

    /// Get a header value by key.
    #[must_use]
    pub fn header(&self, key: &str) -> Option<&str> {
        self.headers.as_ref()?.get(key).map(String::as_str)
    }

    /// Add or update a header.
    pub fn set_header(&mut self, key: String, value: String) {
        self.headers.get_or_insert_with(std::collections::HashMap::new).insert(key, value);
    }

    /// Remove a header.
    pub fn remove_header(&mut self, key: &str) -> Option<String> {
        self.headers.as_mut()?.remove(key)
    }

    /// Clone the message with a new ID.
    #[must_use]
    pub fn clone_with_new_id(&self) -> Self {
        let mut cloned = self.clone();
        cloned.id = MessageId::new();
        cloned
    }

    /// Clone the message with a new timestamp.
    #[must_use]
    pub fn clone_with_new_timestamp(&self) -> Self {
        let mut cloned = self.clone();
        cloned.timestamp = Utc::now();
        cloned
    }

    /// Convert to a compact representation (without headers for efficiency).
    #[must_use]
    pub fn to_compact(&self) -> CompactMessage {
        CompactMessage {
            id: self.id,
            topic: self.topic.clone(),
            payload: self.payload.clone(),
            timestamp: self.timestamp,
            partition: self.partition,
            offset: self.offset,
        }
    }
}

impl Default for Message {
    fn default() -> Self {
        Self {
            id: MessageId::new(),
            topic: Topic::new("default").expect("default topic should be valid"),
            payload: Bytes::new(),
            timestamp: Utc::now(),
            partition: None,
            offset: None,
            headers: None,
        }
    }
}

/// Compact message representation without headers for high-performance scenarios.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactMessage {
    /// Unique message identifier
    pub id: MessageId,

    /// Topic for message routing
    pub topic: Topic,

    /// Message payload (zero-copy)
    pub payload: Bytes,

    /// Message timestamp
    pub timestamp: Timestamp,

    /// Optional partition assignment
    pub partition: Option<PartitionId>,

    /// Optional message offset
    pub offset: Option<Offset>,
}

impl CompactMessage {
    /// Convert back to full message (without headers).
    #[must_use]
    pub fn to_full(self) -> Message {
        Message {
            id: self.id,
            topic: self.topic,
            payload: self.payload,
            timestamp: self.timestamp,
            partition: self.partition,
            offset: self.offset,
            headers: None,
        }
    }

    /// Get the payload size in bytes.
    #[must_use]
    pub fn payload_size(&self) -> usize {
        self.payload.len()
    }
}

/// Builder for constructing messages with various options.
#[derive(Debug, Default)]
pub struct MessageBuilder {
    topic: Option<String>,
    payload: Option<Bytes>,
    partition: Option<PartitionId>,
    offset: Option<Offset>,
    headers: Option<std::collections::HashMap<String, String>>,
}

impl MessageBuilder {
    /// Set the message topic.
    #[must_use]
    pub fn topic(mut self, topic: impl Into<String>) -> Self {
        self.topic = Some(topic.into());
        self
    }

    /// Set the message payload.
    #[must_use]
    pub fn payload(mut self, payload: impl Into<Bytes>) -> Self {
        self.payload = Some(payload.into());
        self
    }

    /// Set the partition assignment.
    #[must_use]
    pub fn partition(mut self, partition: PartitionId) -> Self {
        self.partition = Some(partition);
        self
    }

    /// Set the message offset.
    #[must_use]
    pub fn offset(mut self, offset: Offset) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Add a header.
    #[must_use]
    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers
            .get_or_insert_with(std::collections::HashMap::new)
            .insert(key.into(), value.into());
        self
    }

    /// Add multiple headers.
    #[must_use]
    pub fn headers(mut self, headers: std::collections::HashMap<String, String>) -> Self {
        match self.headers {
            Some(ref mut existing) => existing.extend(headers),
            None => self.headers = Some(headers),
        }
        self
    }

    /// Build the message.
    ///
    /// # Errors
    /// Returns an error if required fields are missing or invalid.
    pub fn build(self) -> crate::Result<Message> {
        let topic = self.topic.ok_or_else(|| crate::Error::InvalidMessage {
            message: "Topic is required".to_string(),
        })?;

        let payload = self.payload.unwrap_or_else(Bytes::new);

        Ok(Message {
            id: MessageId::new(),
            topic: Topic::new(topic)?,
            payload,
            timestamp: Utc::now(),
            partition: self.partition,
            offset: self.offset,
            headers: self.headers,
        })
    }
}

/// Message batch for efficient bulk operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageBatch {
    /// Messages in the batch
    pub messages: Vec<Message>,

    /// Batch metadata
    pub batch_id: MessageId,
    
    /// Batch creation timestamp
    pub created_at: Timestamp,
}

impl MessageBatch {
    /// Create a new message batch.
    #[must_use]
    pub fn new(messages: Vec<Message>) -> Self {
        Self {
            messages,
            batch_id: MessageId::new(),
            created_at: Utc::now(),
        }
    }

    /// Get the number of messages in the batch.
    #[must_use]
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Check if the batch is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Get the total payload size across all messages.
    #[must_use]
    pub fn total_payload_size(&self) -> usize {
        self.messages.iter().map(|m| m.payload_size()).sum()
    }

    /// Get estimated total size of the batch.
    #[must_use]
    pub fn estimated_size(&self) -> usize {
        let batch_overhead = std::mem::size_of::<MessageId>() + std::mem::size_of::<Timestamp>();
        let messages_size: usize = self.messages.iter().map(|m| m.estimated_size()).sum();
        batch_overhead + messages_size
    }

    /// Split the batch into smaller batches of the specified size.
    #[must_use]
    pub fn split(self, batch_size: usize) -> Vec<MessageBatch> {
        if batch_size == 0 {
            return vec![self];
        }

        self.messages
            .chunks(batch_size)
            .map(|chunk| MessageBatch::new(chunk.to_vec()))
            .collect()
    }

    /// Merge multiple batches into one.
    #[must_use]
    pub fn merge(batches: Vec<MessageBatch>) -> MessageBatch {
        let mut all_messages = Vec::new();
        for batch in batches {
            all_messages.extend(batch.messages);
        }
        MessageBatch::new(all_messages)
    }
}

impl Default for MessageBatch {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl IntoIterator for MessageBatch {
    type Item = Message;
    type IntoIter = std::vec::IntoIter<Message>;

    fn into_iter(self) -> Self::IntoIter {
        self.messages.into_iter()
    }
}

impl<'a> IntoIterator for &'a MessageBatch {
    type Item = &'a Message;
    type IntoIter = std::slice::Iter<'a, Message>;

    fn into_iter(self) -> Self::IntoIter {
        self.messages.iter()
    }
}

impl From<Vec<Message>> for MessageBatch {
    fn from(messages: Vec<Message>) -> Self {
        Self::new(messages)
    }
}

impl From<Message> for MessageBatch {
    fn from(message: Message) -> Self {
        Self::new(vec![message])
    }
}

// Implement Zeroize for secure memory cleanup
impl Zeroize for Message {
    fn zeroize(&mut self) {
        // Note: We can't zeroize Bytes directly as it's an immutable reference-counted type
        // In a real implementation, you'd need to handle this differently for security
        if let Some(ref mut headers) = self.headers {
            for (_key, value) in headers.iter_mut() {
                // We can only zeroize the values, not the keys due to HashMap constraints
                value.zeroize();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let payload = Bytes::from("test payload");
        let message = Message::new("test.topic", payload.clone()).unwrap();

        assert_eq!(message.topic.as_str(), "test.topic");
        assert_eq!(message.payload, payload);
        assert!(message.partition.is_none());
        assert!(message.offset.is_none());
        assert!(message.headers.is_none());
    }

    #[test]
    fn test_message_builder() {
        let message = Message::builder()
            .topic("test.topic")
            .payload(Bytes::from("test payload"))
            .partition(PartitionId::new(1))
            .header("content-type", "application/json")
            .build()
            .unwrap();

        assert_eq!(message.topic.as_str(), "test.topic");
        assert_eq!(message.partition, Some(PartitionId::new(1)));
        assert_eq!(message.header("content-type"), Some("application/json"));
    }

    #[test]
    fn test_message_headers() {
        let mut message = Message::default();
        
        assert!(!message.has_headers());
        assert!(message.header("key").is_none());

        message.set_header("key1".to_string(), "value1".to_string());
        message.set_header("key2".to_string(), "value2".to_string());

        assert!(message.has_headers());
        assert_eq!(message.header("key1"), Some("value1"));
        assert_eq!(message.header("key2"), Some("value2"));

        let removed = message.remove_header("key1");
        assert_eq!(removed, Some("value1".to_string()));
        assert!(message.header("key1").is_none());
    }

    #[test]
    fn test_topic_validation() {
        assert!(Topic::new("valid-topic_name.123").is_ok());
        assert!(Topic::new("").is_err());
        assert!(Topic::new("invalid topic with spaces").is_err());
        assert!(Topic::new("invalid@topic").is_err());
        
        let long_name = "a".repeat(256);
        assert!(Topic::new(long_name).is_err());
    }

    #[test]
    fn test_message_batch() {
        let messages = vec![
            Message::new("topic1", Bytes::from("payload1")).unwrap(),
            Message::new("topic2", Bytes::from("payload2")).unwrap(),
            Message::new("topic3", Bytes::from("payload3")).unwrap(),
        ];

        let batch = MessageBatch::new(messages.clone());
        assert_eq!(batch.len(), 3);
        assert!(!batch.is_empty());

        let split_batches = batch.split(2);
        assert_eq!(split_batches.len(), 2);
        assert_eq!(split_batches[0].len(), 2);
        assert_eq!(split_batches[1].len(), 1);
    }

    #[test]
    fn test_message_equality() {
        let message1 = Message::new("test", Bytes::from("data")).unwrap();
        let message2 = message1.clone();
        
        assert_eq!(message1, message2);
        
        let message3 = message1.clone_with_new_id();
        assert_ne!(message1, message3); // Different IDs
    }

    #[test]
    fn test_compact_message() {
        let mut message = Message::new("test", Bytes::from("data")).unwrap();
        message.set_header("key".to_string(), "value".to_string());

        let compact = message.to_compact();
        let restored = compact.to_full();

        assert_eq!(message.id, restored.id);
        assert_eq!(message.topic, restored.topic);
        assert_eq!(message.payload, restored.payload);
        assert!(restored.headers.is_none()); // Headers are lost in compact form
    }
}