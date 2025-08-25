use std::time::SystemTime;

use kaelix_cluster::messages::ClusterMessage;
use serde::{Deserialize, Serialize};

use super::WalError;

/// WAL entry types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryType {
    /// Regular message entry
    Message,
    /// Control entry (checkpoints, metadata, etc.)
    Control,
    /// Heartbeat entry for liveness detection
    Heartbeat,
}

/// Entry metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryMetadata {
    /// Entry type
    pub entry_type: EntryType,
    /// Entry version for backward compatibility
    pub version: u8,
    /// Entry flags for future extensions
    pub flags: u16,
}

impl Default for EntryMetadata {
    fn default() -> Self {
        Self { entry_type: EntryType::Message, version: 1, flags: 0 }
    }
}

impl EntryMetadata {
    /// Create new message entry metadata
    pub fn message() -> Self {
        Self { entry_type: EntryType::Message, version: 1, flags: 0 }
    }

    /// Create new control entry metadata
    pub fn control() -> Self {
        Self { entry_type: EntryType::Control, version: 1, flags: 0 }
    }

    /// Create new heartbeat entry metadata
    pub fn heartbeat() -> Self {
        Self { entry_type: EntryType::Heartbeat, version: 1, flags: 0 }
    }
}

/// A single log entry in the WAL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Globally unique sequence number
    pub sequence_number: u64,

    /// Timestamp (nanoseconds since Unix epoch)
    pub timestamp: u64,

    /// Entry metadata
    pub metadata: EntryMetadata,

    /// Payload size in bytes
    pub payload_length: u32,

    /// CRC32 checksum of the payload
    pub checksum: u32,

    /// Serialized payload
    pub payload: Vec<u8>,
}

impl LogEntry {
    /// Create a new log entry
    pub fn new(
        sequence_number: u64,
        timestamp: u64,
        message: ClusterMessage,
    ) -> Result<Self, WalError> {
        let payload = bincode::serialize(&message)
            .map_err(|e| WalError::Serialization(format!("Failed to serialize message: {}", e)))?;

        // Calculate checksum
        let checksum = crc32fast::hash(&payload);

        Ok(Self {
            sequence_number,
            timestamp,
            metadata: EntryMetadata::message(),
            payload_length: payload.len() as u32,
            checksum,
            payload,
        })
    }

    /// Create a control entry
    pub fn control(
        sequence_number: u64,
        timestamp: u64,
        control_data: Vec<u8>,
    ) -> Result<Self, WalError> {
        let checksum = crc32fast::hash(&control_data);

        Ok(Self {
            sequence_number,
            timestamp,
            metadata: EntryMetadata::control(),
            payload_length: control_data.len() as u32,
            checksum,
            payload: control_data,
        })
    }

    /// Create a heartbeat entry
    pub fn heartbeat(sequence_number: u64, timestamp: u64) -> Result<Self, WalError> {
        let payload = Vec::new();
        let checksum = crc32fast::hash(&payload);

        Ok(Self {
            sequence_number,
            timestamp,
            metadata: EntryMetadata::heartbeat(),
            payload_length: 0,
            checksum,
            payload,
        })
    }

    /// Create entry with current timestamp
    pub fn now(sequence_number: u64, message: ClusterMessage) -> Result<Self, WalError> {
        let timestamp = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| WalError::Io(format!("Failed to get current time: {}", e)))?
            .as_nanos() as u64;

        Self::new(sequence_number, timestamp, message)
    }

    /// Get entry size when serialized
    pub fn serialized_size(&self) -> usize {
        // Fixed header size: 8 + 8 + metadata_size + 4 + 4 = at least 24 bytes
        // Plus payload length and some overhead for metadata serialization
        24 + self.payload.len()
    }

    /// Serialize entry to bytes
    pub fn serialize(&self) -> Result<Vec<u8>, WalError> {
        bincode::serialize(self)
            .map_err(|e| WalError::Serialization(format!("Failed to serialize entry: {}", e)))
    }

    /// Deserialize entry from bytes
    pub fn deserialize(data: &[u8]) -> Result<Self, WalError> {
        bincode::deserialize(data)
            .map_err(|e| WalError::Serialization(format!("Failed to deserialize entry: {}", e)))
    }

    /// Deserialize only the header (first 24 bytes) to get basic info
    pub fn deserialize_header(data: &[u8]) -> Result<Self, WalError> {
        if data.len() < 24 {
            return Err(WalError::Io("Insufficient data for entry header".to_string()));
        }

        // For simplicity, we'll deserialize the full entry but this could be optimized
        // to only deserialize the header fields
        Self::deserialize(data)
    }

    /// Verify entry integrity using CRC32 checksum
    pub fn verify_integrity(&self) -> bool {
        let calculated_checksum = crc32fast::hash(&self.payload);
        calculated_checksum == self.checksum
    }

    /// Get entry age in nanoseconds
    pub fn age_nanos(&self) -> u64 {
        let now = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        now.saturating_sub(self.timestamp)
    }

    /// Check if entry is older than specified duration
    pub fn is_older_than(&self, nanos: u64) -> bool {
        self.age_nanos() > nanos
    }

    /// Get human-readable timestamp
    pub fn timestamp_as_system_time(&self) -> SystemTime {
        std::time::UNIX_EPOCH + std::time::Duration::from_nanos(self.timestamp)
    }

    /// Check if entry is a message
    pub fn is_message(&self) -> bool {
        self.metadata.entry_type == EntryType::Message
    }

    /// Check if entry is a control entry
    pub fn is_control(&self) -> bool {
        self.metadata.entry_type == EntryType::Control
    }

    /// Check if entry is a heartbeat
    pub fn is_heartbeat(&self) -> bool {
        self.metadata.entry_type == EntryType::Heartbeat
    }

    /// Deserialize the payload as a cluster message
    ///
    /// Returns [`WalError`] if the entry is not a message type or deserialization fails.
    /// Returns [`WalError`] if deserialization fails.
    pub fn deserialize_message(&self) -> Result<ClusterMessage, WalError> {
        if self.metadata.entry_type != EntryType::Message {
            return Err(WalError::Corruption(format!(
                "not a message entry (sequence {}): {:?}",
                self.sequence_number, self.metadata.entry_type
            )));
        }

        bincode::deserialize(&self.payload).map_err(|e| {
            WalError::Serialization(format!(
                "failed to deserialize cluster message: {}",
                e
            ))
        })
    }

    /// Check if entry is a checkpoint
    pub fn is_checkpoint(&self) -> bool {
        self.metadata.entry_type == EntryType::Control && self.metadata.flags & 0x01 != 0
    }

    /// Create checkpoint entry
    pub fn checkpoint(
        sequence_number: u64,
        timestamp: u64,
        checkpoint_data: Vec<u8>,
    ) -> Result<Self, WalError> {
        let checksum = crc32fast::hash(&checkpoint_data);

        Ok(Self {
            sequence_number,
            timestamp,
            metadata: EntryMetadata {
                entry_type: EntryType::Control,
                version: 1,
                flags: 0x01, // Checkpoint flag
            },
            payload_length: checkpoint_data.len() as u32,
            checksum,
            payload: checkpoint_data,
        })
    }

    /// Update checksum after payload modification
    pub fn update_checksum(&mut self) {
        self.checksum = crc32fast::hash(&self.payload);
        self.payload_length = self.payload.len() as u32;
    }

    /// Validate entry invariants
    pub fn validate(&self) -> Result<(), WalError> {
        // Check payload length matches actual payload size
        if self.payload_length as usize != self.payload.len() {
            return Err(WalError::Corruption(format!(
                "payload length mismatch (sequence {}): expected {}, got {}",
                self.sequence_number,
                self.payload_length,
                self.payload.len()
            )));
        }

        // Verify checksum
        if !self.verify_integrity() {
            return Err(WalError::Corruption(format!(
                "checksum mismatch (sequence {})",
                self.sequence_number
            )));
        }

        // Validate sequence number
        if self.sequence_number == 0 {
            return Err(WalError::Corruption(format!(
                "invalid sequence number: {}",
                self.sequence_number
            )));
        }

        // Validate timestamp
        if self.timestamp == 0 {
            return Err(WalError::Corruption(format!(
                "invalid timestamp (sequence {}): {}",
                self.sequence_number, self.timestamp
            )));
        }

        Ok(())
    }

    /// Create a copy with updated sequence number
    pub fn with_sequence(&self, sequence_number: u64) -> Self {
        let mut entry = self.clone();
        entry.sequence_number = sequence_number;
        entry
    }

    /// Create a copy with updated timestamp
    pub fn with_timestamp(&self, timestamp: u64) -> Self {
        let mut entry = self.clone();
        entry.timestamp = timestamp;
        entry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entry_creation() {
        let message = ClusterMessage::default();
        let entry = LogEntry::new(1, 12345, message).unwrap();

        assert_eq!(entry.sequence_number, 1);
        assert_eq!(entry.timestamp, 12345);
        assert!(entry.is_message());
        assert!(!entry.is_control());
        assert!(!entry.is_heartbeat());
    }

    #[test]
    fn test_entry_serialization() {
        let message = ClusterMessage::default();
        let entry = LogEntry::new(1, 12345, message).unwrap();

        let serialized = entry.serialize().unwrap();
        let deserialized = LogEntry::deserialize(&serialized).unwrap();

        assert_eq!(entry.sequence_number, deserialized.sequence_number);
        assert_eq!(entry.timestamp, deserialized.timestamp);
        assert_eq!(entry.payload, deserialized.payload);
    }

    #[test]
    fn test_entry_integrity() {
        let message = ClusterMessage::default();
        let mut entry = LogEntry::new(1, 12345, message).unwrap();

        assert!(entry.verify_integrity());

        // Corrupt the payload
        entry.payload[0] = entry.payload[0].wrapping_add(1);
        assert!(!entry.verify_integrity());

        // Fix checksum
        entry.update_checksum();
        assert!(entry.verify_integrity());
    }

    #[test]
    fn test_control_entry() {
        let control_data = b"checkpoint data".to_vec();
        let entry = LogEntry::control(1, 12345, control_data).unwrap();

        assert!(entry.is_control());
        assert!(!entry.is_message());
        assert!(!entry.is_heartbeat());
    }

    #[test]
    fn test_heartbeat_entry() {
        let entry = LogEntry::heartbeat(1, 12345).unwrap();

        assert!(entry.is_heartbeat());
        assert!(!entry.is_message());
        assert!(!entry.is_control());
        assert_eq!(entry.payload.len(), 0);
    }

    #[test]
    fn test_checkpoint_entry() {
        let checkpoint_data = b"checkpoint".to_vec();
        let entry = LogEntry::checkpoint(1, 12345, checkpoint_data).unwrap();

        assert!(entry.is_checkpoint());
        assert!(entry.is_control());
        assert!(!entry.is_message());
    }

    #[test]
    fn test_entry_validation() {
        let message = ClusterMessage::default();
        let entry = LogEntry::new(1, 12345, message).unwrap();

        assert!(entry.validate().is_ok());

        // Test invalid sequence number
        let mut invalid_entry = entry.clone();
        invalid_entry.sequence_number = 0;
        assert!(invalid_entry.validate().is_err());

        // Test invalid timestamp
        let mut invalid_entry = entry.clone();
        invalid_entry.timestamp = 0;
        assert!(invalid_entry.validate().is_err());
    }

    #[test]
    fn test_entry_age() {
        let message = ClusterMessage::default();
        let entry = LogEntry::now(1, message).unwrap();

        // Entry should be very new
        assert!(entry.age_nanos() < 1_000_000_000); // Less than 1 second
    }

    #[test]
    fn test_message_deserialization() {
        let message = ClusterMessage::default();
        let entry = LogEntry::new(1, 12345, message.clone()).unwrap();

        let deserialized_message = entry.deserialize_message().unwrap();
        assert_eq!(message, deserialized_message);

        // Test error for non-message entry
        let control_entry = LogEntry::control(1, 12345, b"data".to_vec()).unwrap();
        assert!(control_entry.deserialize_message().is_err());
    }
}