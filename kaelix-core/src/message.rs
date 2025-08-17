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
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a message ID from a UUID.
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Get the underlying UUID.
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
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Convert to owned string.
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub fn builder() -> MessageBuilder {
        MessageBuilder::default()
    }

    /// Get the payload size in bytes.
    pub fn payload_size(&self) -> usize {
        self.payload.len()
    }

    /// Check if the message has headers.
    pub fn has_headers(&self) -> bool {
        self.headers.as_ref().map_or(false, |h| !h.is_empty())
    }

    /// Get a header value by key.
    pub fn get_header(&self, key: &str) -> Option<&str> {
        self.headers.as_ref()?.get(key).map(String::as_str)
    }

    /// Set a header value.
    pub fn set_header(&mut self, key: String, value: String) {
        self.headers.get_or_insert_with(Default::default).insert(key, value);
    }
}

/// Builder pattern for constructing messages.
#[derive(Debug, Default)]
pub struct MessageBuilder {
    id: Option<MessageId>,
    topic: Option<Topic>,
    payload: Option<Bytes>,
    timestamp: Option<Timestamp>,
    partition: Option<PartitionId>,
    offset: Option<Offset>,
    headers: Option<std::collections::HashMap<String, String>>,
}

impl MessageBuilder {
    /// Set the message ID.
    pub fn id(mut self, id: MessageId) -> Self {
        self.id = Some(id);
        self
    }

    /// Set the topic.
    pub fn topic(mut self, topic: impl Into<String>) -> Self {
        if let Ok(topic) = Topic::new(topic) {
            self.topic = Some(topic);
        }
        self
    }

    /// Set the payload.
    pub fn payload(mut self, payload: Bytes) -> Self {
        self.payload = Some(payload);
        self
    }

    /// Set the timestamp.
    pub fn timestamp(mut self, timestamp: Timestamp) -> Self {
        self.timestamp = Some(timestamp);
        self
    }

    /// Set the partition.
    pub fn partition(mut self, partition: PartitionId) -> Self {
        self.partition = Some(partition);
        self
    }

    /// Set the offset.
    pub fn offset(mut self, offset: Offset) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Add a header.
    pub fn header(mut self, key: String, value: String) -> Self {
        self.headers.get_or_insert_with(Default::default).insert(key, value);
        self
    }

    /// Build the message.
    ///
    /// # Errors
    /// Returns an error if required fields are missing.
    pub fn build(self) -> crate::Result<Message> {
        let topic = self.topic.ok_or_else(|| crate::Error::InvalidMessage {
            message: "Topic is required".to_string(),
        })?;

        let payload = self.payload.ok_or_else(|| crate::Error::InvalidMessage {
            message: "Payload is required".to_string(),
        })?;

        Ok(Message {
            id: self.id.unwrap_or_else(MessageId::new),
            topic,
            payload,
            timestamp: self.timestamp.unwrap_or_else(Utc::now),
            partition: self.partition,
            offset: self.offset,
            headers: self.headers,
        })
    }
}

impl Zeroize for Message {
    fn zeroize(&mut self) {
        // Note: We can't zeroize Bytes directly, but we can clear references
        self.payload = Bytes::new();
        if let Some(ref mut headers) = self.headers {
            headers.clear();
        }
    }
}