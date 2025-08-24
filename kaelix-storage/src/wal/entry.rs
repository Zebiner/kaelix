use std::mem;
use std::time::{SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use bitflags::bitflags;
use crc32fast::Hasher;

use super::{ClusterMessage, WalError};

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct EntryFlags: u32 {
        /// Entry is compressed
        const COMPRESSED = 0x01;
        /// Entry contains sensitive data
        const SENSITIVE = 0x02;
        /// Entry is part of a transaction
        const TRANSACTIONAL = 0x04;
        /// Entry requires ordered processing
        const ORDERED = 0x08;
        /// Entry is a system message
        const SYSTEM = 0x10;
        /// Entry has high priority
        const HIGH_PRIORITY = 0x20;
        /// Entry is encrypted
        const ENCRYPTED = 0x40;
        /// Entry is replicated
        const REPLICATED = 0x80;
    }
}

/// Log entry metadata
#[derive(Debug, Clone, PartialEq)]
pub struct EntryMetadata {
    /// Type of entry
    pub entry_type: EntryType,
    /// Entry flags
    pub flags: EntryFlags,
    /// Reserved for future use
    pub reserved: [u8; 2],
}

impl Default for EntryMetadata {
    fn default() -> Self {
        Self {
            entry_type: EntryType::Message,
            flags: EntryFlags::empty(),
            reserved: [0; 2],
        }
    }
}

/// Type of log entry
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EntryType {
    /// Regular message entry
    Message = 0,
    /// Checkpoint entry
    Checkpoint = 1,
    /// Tombstone entry (for deletions)
    Tombstone = 2,
}

impl EntryType {
    /// Convert from u8
    pub fn from_u8(value: u8) -> Result<Self, WalError> {
        match value {
            0 => Ok(Self::Message),
            1 => Ok(Self::Checkpoint),
            2 => Ok(Self::Tombstone),
            _ => Err(WalError::Corruption {
                sequence: 0,
                details: format!("invalid entry type: {}", value),
            }),
        }
    }
}

/// High-performance log entry with fixed-size header
/// 
/// Header structure (24 bytes total):
/// - Sequence number (8 bytes)
/// - Timestamp (8 bytes) 
/// - CRC32C checksum (4 bytes)
/// - Entry type (1 byte)
/// - Flags (1 byte)
/// - Reserved (2 bytes)
/// 
/// Payload follows immediately after header.
#[derive(Debug, Clone)]
pub struct LogEntry {
    /// Unique sequence number
    pub sequence_number: u64,
    /// Timestamp in nanoseconds since epoch
    pub timestamp: u64,
    /// CRC32C checksum of payload
    pub checksum: u32,
    /// Entry metadata
    pub metadata: EntryMetadata,
    /// Payload data
    pub payload: Bytes,
    /// Cached payload length for performance
    pub payload_length: u32,
}

// Internal header structure for serialization
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct EntryHeader {
    sequence: u64,
    timestamp: u64,
    checksum: u32,
    entry_type: u8,
    flags: u8,
    reserved: [u8; 2],
}

impl LogEntry {
    /// Create new log entry with message
    /// 
    /// # Arguments
    /// 
    /// * `sequence` - Unique sequence number
    /// * `timestamp` - Timestamp in nanoseconds
    /// * `message` - Cluster message to store
    /// 
    /// # Returns
    /// 
    /// New [`LogEntry`] with computed checksum.
    /// 
    /// # Errors
    /// 
    /// Returns [`WalError`] if serialization fails.
    pub fn new(sequence: u64, timestamp: u64, message: ClusterMessage) -> Result<Self, WalError> {
        let payload = bincode::serialize(&message).map_err(|e| WalError::Serialization {
            details: format!("failed to serialize cluster message: {}", e),
        })?;
        
        let payload_bytes = Bytes::from(payload);
        let checksum = Self::compute_checksum(&payload_bytes);
        
        Ok(Self {
            sequence_number: sequence,
            timestamp,
            checksum,
            metadata: EntryMetadata {
                entry_type: EntryType::Message,
                flags: EntryFlags::empty(),
                reserved: [0; 2],
            },
            payload_length: payload_bytes.len() as u32,
            payload: payload_bytes,
        })
    }
    
    /// Create checkpoint entry
    /// 
    /// # Arguments
    /// 
    /// * `sequence` - Unique sequence number
    /// * `timestamp` - Timestamp in nanoseconds
    /// 
    /// # Returns
    /// 
    /// New checkpoint [`LogEntry`].
    pub fn checkpoint(sequence: u64, timestamp: u64) -> Self {
        let payload = Bytes::new();
        
        Self {
            sequence_number: sequence,
            timestamp,
            checksum: 0, // Checkpoints have no payload
            metadata: EntryMetadata {
                entry_type: EntryType::Checkpoint,
                flags: EntryFlags::SYSTEM,
                reserved: [0; 2],
            },
            payload_length: 0,
            payload,
        }
    }
    
    /// Create tombstone entry
    /// 
    /// # Arguments
    /// 
    /// * `sequence` - Unique sequence number
    /// * `timestamp` - Timestamp in nanoseconds
    /// 
    /// # Returns
    /// 
    /// New tombstone [`LogEntry`].
    pub fn tombstone(sequence: u64, timestamp: u64) -> Self {
        let payload = Bytes::new();
        
        Self {
            sequence_number: sequence,
            timestamp,
            checksum: 0, // Tombstones have no payload
            metadata: EntryMetadata {
                entry_type: EntryType::Tombstone,
                flags: EntryFlags::SYSTEM,
                reserved: [0; 2],
            },
            payload_length: 0,
            payload,
        }
    }
    
    /// Serialize entry to bytes
    /// 
    /// Format: [24-byte header][variable payload]
    /// 
    /// # Returns
    /// 
    /// Serialized entry as [`Vec<u8>`].
    /// 
    /// # Errors
    /// 
    /// Returns [`WalError`] if serialization fails.
    pub fn serialize(&self) -> Result<Vec<u8>, WalError> {
        let header = EntryHeader {
            sequence: self.sequence_number,
            timestamp: self.timestamp,
            checksum: self.checksum,
            entry_type: self.metadata.entry_type as u8,
            flags: (self.metadata.flags.bits() & 0xFF) as u8, // Convert u32 to u8
            reserved: self.metadata.reserved,
        };
        
        let mut data = Vec::with_capacity(24 + self.payload.len());
        
        // Serialize header (unsafe but very fast)
        #[allow(unsafe_code)]
        unsafe {
            let header_bytes = std::slice::from_raw_parts(
                &header as *const EntryHeader as *const u8,
                mem::size_of::<EntryHeader>(),
            );
            data.extend_from_slice(header_bytes);
        }
        
        // Append payload
        data.extend_from_slice(&self.payload);
        
        Ok(data)
    }
    
    /// Get serialized size in bytes
    #[must_use]
    pub fn serialized_size(&self) -> usize {
        24 + self.payload.len()
    }
    
    /// Compute CRC32C checksum of payload
    fn compute_checksum(payload: &Bytes) -> u32 {
        let mut hasher = Hasher::new();
        hasher.update(payload);
        hasher.finalize()
    }
    
    /// Verify entry integrity
    #[must_use]
    pub fn verify_integrity(&self) -> bool {
        if self.metadata.entry_type == EntryType::Checkpoint || 
           self.metadata.entry_type == EntryType::Tombstone {
            // System entries don't need checksum verification
            true
        } else {
            let expected = Self::compute_checksum(&self.payload);
            self.checksum == expected
        }
    }
    
    /// Deserialize entry from bytes
    /// 
    /// # Arguments
    /// 
    /// * `data` - Serialized entry data
    /// 
    /// # Returns
    /// 
    /// Deserialized [`LogEntry`].
    /// 
    /// # Errors
    /// 
    /// Returns [`WalError`] if deserialization fails or data is corrupted.
    pub fn deserialize(data: &[u8]) -> Result<Self, WalError> {
        if data.len() < mem::size_of::<EntryHeader>() {
            return Err(WalError::Corruption {
                sequence: 0,
                details: format!(
                    "entry data too short: {} bytes, expected at least {}",
                    data.len(),
                    mem::size_of::<EntryHeader>()
                ),
            });
        }

        // Extract header (unsafe but very fast)
        #[allow(unsafe_code)]
        let header = unsafe {
            std::ptr::read_unaligned(data.as_ptr() as *const EntryHeader)
        };

        // Extract payload
        let payload_start = mem::size_of::<EntryHeader>();
        let payload = Bytes::copy_from_slice(&data[payload_start..]);
        let payload_length = payload.len() as u32;

        // Copy header fields to avoid alignment issues
        let flags_value = header.flags as u32; // Convert u8 to u32
        
        // Reconstruct entry
        let entry = Self {
            sequence_number: header.sequence,
            timestamp: header.timestamp,
            checksum: header.checksum,
            metadata: EntryMetadata {
                entry_type: EntryType::from_u8(header.entry_type)?,
                flags: EntryFlags::from_bits(flags_value).ok_or_else(|| {
                    WalError::Corruption {
                        sequence: header.sequence,
                        details: format!("invalid entry flags: 0x{:x}", flags_value),
                    }
                })?,
                reserved: header.reserved,
            },
            payload_length,
            payload,
        };

        // Verify integrity
        if !entry.verify_integrity() {
            return Err(WalError::Corruption {
                sequence: entry.sequence_number,
                details: "checksum verification failed".to_string(),
            });
        }

        Ok(entry)
    }

    /// Deserialize only the header from bytes
    /// 
    /// This is useful for scanning segments without reading full payloads.
    /// 
    /// # Arguments
    /// 
    /// * `data` - Serialized entry header data (at least 24 bytes)
    /// 
    /// # Returns
    /// 
    /// [`LogEntry`] with header fields populated and empty payload.
    /// 
    /// # Errors
    /// 
    /// Returns [`WalError`] if header deserialization fails.
    pub fn deserialize_header(data: &[u8]) -> Result<Self, WalError> {
        if data.len() < mem::size_of::<EntryHeader>() {
            return Err(WalError::Corruption {
                sequence: 0,
                details: format!(
                    "header data too short: {} bytes, expected {}",
                    data.len(),
                    mem::size_of::<EntryHeader>()
                ),
            });
        }

        // Extract header (unsafe but very fast)
        #[allow(unsafe_code)]
        let header = unsafe {
            std::ptr::read_unaligned(data.as_ptr() as *const EntryHeader)
        };

        // Copy header fields to avoid alignment issues
        let flags_value = header.flags as u32; // Convert u8 to u32
        
        // Reconstruct entry with empty payload
        let entry = Self {
            sequence_number: header.sequence,
            timestamp: header.timestamp,
            checksum: header.checksum,
            metadata: EntryMetadata {
                entry_type: EntryType::from_u8(header.entry_type)?,
                flags: EntryFlags::from_bits(flags_value).ok_or_else(|| {
                    WalError::Corruption {
                        sequence: header.sequence,
                        details: format!("invalid entry flags: 0x{:x}", flags_value),
                    }
                })?,
                reserved: header.reserved,
            },
            payload_length: 0, // Will be set correctly when full entry is read
            payload: Bytes::new(),
        };

        Ok(entry)
    }

    /// Deserialize the stored cluster message
    /// 
    /// # Returns
    /// 
    /// The original [`ClusterMessage`].
    /// 
    /// # Errors
    /// 
    /// Returns [`WalError`] if deserialization fails.
    pub fn deserialize_message(&self) -> Result<ClusterMessage, WalError> {
        if self.metadata.entry_type != EntryType::Message {
            return Err(WalError::Corruption {
                sequence: self.sequence_number,
                details: format!("not a message entry: {:?}", self.metadata.entry_type),
            });
        }

        bincode::deserialize(&self.payload).map_err(|e| WalError::Serialization {
            details: format!("failed to deserialize cluster message: {}", e),
        })
    }

    /// Check if entry is a checkpoint
    #[must_use]
    pub fn is_checkpoint(&self) -> bool {
        self.metadata.entry_type == EntryType::Checkpoint
    }
    
    /// Check if entry is a tombstone
    #[must_use]
    pub fn is_tombstone(&self) -> bool {
        self.metadata.entry_type == EntryType::Tombstone
    }
    
    /// Check if entry is a regular message
    #[must_use]
    pub fn is_message(&self) -> bool {
        self.metadata.entry_type == EntryType::Message
    }
    
    /// Get entry age
    #[must_use]
    pub fn age(&self) -> std::time::Duration {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        
        if now > self.timestamp {
            std::time::Duration::from_nanos(now - self.timestamp)
        } else {
            std::time::Duration::ZERO
        }
    }
    
    /// Check if entry has flag
    #[must_use]
    pub fn has_flag(&self, flag: EntryFlags) -> bool {
        self.metadata.flags.contains(flag)
    }
    
    /// Set flag on entry
    pub fn set_flag(&mut self, flag: EntryFlags) {
        self.metadata.flags.insert(flag);
    }
    
    /// Clear flag on entry
    pub fn clear_flag(&mut self, flag: EntryFlags) {
        self.metadata.flags.remove(flag);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaelix_core::message::Message;
    use kaelix_core::types::NodeId;
    
    fn create_test_message() -> ClusterMessage {
        Message::new(
            "test-123".to_string(),
            NodeId::from("node1"),
            NodeId::from("node2"),
            b"test payload data".to_vec(),
        )
    }
    
    #[test]
    fn test_log_entry_creation() {
        let message = create_test_message();
        let entry = LogEntry::new(42, 1234567890, message).unwrap();
        
        assert_eq!(entry.sequence_number, 42);
        assert_eq!(entry.timestamp, 1234567890);
        assert_eq!(entry.metadata.entry_type, EntryType::Message);
        assert!(!entry.payload.is_empty());
        assert!(entry.verify_integrity());
    }
    
    #[test]
    fn test_checkpoint_creation() {
        let checkpoint = LogEntry::checkpoint(100, 9876543210);
        
        assert_eq!(checkpoint.sequence_number, 100);
        assert_eq!(checkpoint.timestamp, 9876543210);
        assert_eq!(checkpoint.metadata.entry_type, EntryType::Checkpoint);
        assert!(checkpoint.has_flag(EntryFlags::SYSTEM));
        assert!(checkpoint.payload.is_empty());
    }
    
    #[test]
    fn test_tombstone_creation() {
        let tombstone = LogEntry::tombstone(200, 1111111111);
        
        assert_eq!(tombstone.sequence_number, 200);
        assert_eq!(tombstone.timestamp, 1111111111);
        assert_eq!(tombstone.metadata.entry_type, EntryType::Tombstone);
        assert!(tombstone.has_flag(EntryFlags::SYSTEM));
        assert!(tombstone.payload.is_empty());
    }
    
    #[test]
    fn test_serialization_roundtrip() {
        let original_message = create_test_message();
        let original = LogEntry::new(42, 1234567890, original_message).unwrap();
        
        let serialized = original.serialize().unwrap();
        let deserialized = LogEntry::deserialize(&serialized).unwrap();
        
        assert_eq!(original.sequence_number, deserialized.sequence_number);
        assert_eq!(original.timestamp, deserialized.timestamp);
        assert_eq!(original.checksum, deserialized.checksum);
        assert_eq!(original.metadata.entry_type, deserialized.metadata.entry_type);
        assert_eq!(original.metadata.flags, deserialized.metadata.flags);
        assert_eq!(original.payload, deserialized.payload);
    }
    
    #[test]
    fn test_header_deserialization() {
        let original_message = create_test_message();
        let original = LogEntry::new(42, 1234567890, original_message).unwrap();
        
        let serialized = original.serialize().unwrap();
        let header_only = LogEntry::deserialize_header(&serialized[..24]).unwrap();
        
        assert_eq!(original.sequence_number, header_only.sequence_number);
        assert_eq!(original.timestamp, header_only.timestamp);
        assert_eq!(original.checksum, header_only.checksum);
        assert_eq!(original.metadata.entry_type, header_only.metadata.entry_type);
        assert_eq!(original.metadata.flags, header_only.metadata.flags);
        assert!(header_only.payload.is_empty()); // Header-only should have empty payload
    }
    
    #[test]
    fn test_message_deserialization() {
        let original_message = create_test_message();
        let entry = LogEntry::new(42, 1234567890, original_message.clone()).unwrap();
        
        let deserialized_message = entry.deserialize_message().unwrap();
        assert_eq!(original_message.id(), deserialized_message.id());
        assert_eq!(original_message.source(), deserialized_message.source());
        assert_eq!(original_message.destination(), deserialized_message.destination());
    }
    
    #[test]
    fn test_flag_operations() {
        let message = create_test_message();
        let mut entry = LogEntry::new(42, 1234567890, message).unwrap();
        
        assert!(!entry.has_flag(EntryFlags::COMPRESSED));
        
        entry.set_flag(EntryFlags::COMPRESSED);
        assert!(entry.has_flag(EntryFlags::COMPRESSED));
        
        entry.clear_flag(EntryFlags::COMPRESSED);
        assert!(!entry.has_flag(EntryFlags::COMPRESSED));
    }
    
    #[test]
    fn test_type_checks() {
        let message = create_test_message();
        let entry = LogEntry::new(42, 1234567890, message).unwrap();
        let checkpoint = LogEntry::checkpoint(100, 9876543210);
        let tombstone = LogEntry::tombstone(200, 1111111111);
        
        assert!(entry.is_message());
        assert!(!entry.is_checkpoint());
        assert!(!entry.is_tombstone());
        
        assert!(!checkpoint.is_message());
        assert!(checkpoint.is_checkpoint());
        assert!(!checkpoint.is_tombstone());
        
        assert!(!tombstone.is_message());
        assert!(!tombstone.is_checkpoint());
        assert!(tombstone.is_tombstone());
    }
    
    #[test]
    fn test_corrupted_data_detection() {
        let message = create_test_message();
        let entry = LogEntry::new(42, 1234567890, message).unwrap();
        let mut serialized = entry.serialize().unwrap();
        
        // Corrupt the checksum
        serialized[16] = 0xFF;
        
        let result = LogEntry::deserialize(&serialized);
        assert!(result.is_err());
        match result.unwrap_err() {
            WalError::Corruption { sequence, details } => {
                assert_eq!(sequence, 42);
                assert!(details.contains("checksum verification failed"));
            }
            _ => panic!("Expected corruption error"),
        }
    }
    
    #[test]
    fn test_insufficient_data() {
        // Too short for header
        let short_data = vec![0u8; 10];
        let result = LogEntry::deserialize(&short_data);
        assert!(result.is_err());
        
        // Header deserialization with insufficient data
        let result = LogEntry::deserialize_header(&short_data);
        assert!(result.is_err());
    }
}