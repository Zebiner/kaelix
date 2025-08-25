/// High-Performance Storage Entry Implementation
/// 
/// Optimized for ultra-low latency operations in distributed streaming systems.

use std::time::SystemTime;

use crate::{StorageError, Result};
use bytes::Bytes;
use serde::{Deserialize, Serialize};

/// Metadata associated with a storage entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageEntryMetadata {
    /// Entry creation timestamp (microseconds since Unix epoch)
    pub timestamp: u64,

    /// Optional compression algorithm used
    pub compression: Option<CompressionType>,

    /// Entry flags for future extensions
    pub flags: u32,

    /// Custom attributes
    pub attributes: std::collections::HashMap<String, String>,
}

/// Supported compression types
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CompressionType {
    /// No compression
    None,
    /// LZ4 compression (fastest)
    Lz4,
    /// Zstd compression (balanced)
    Zstd,
    /// Gzip compression (higher ratio)
    Gzip,
}

impl Default for StorageEntryMetadata {
    fn default() -> Self {
        Self {
            timestamp: SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_micros() as u64,
            compression: None,
            flags: 0,
            attributes: std::collections::HashMap::new(),
        }
    }
}

/// A storage entry containing data and metadata
#[derive(Debug, Clone)]
pub struct StorageEntry {
    /// Unique sequence number for ordering
    pub sequence: u64,

    /// Entry metadata
    pub metadata: StorageEntryMetadata,

    /// Entry payload data
    pub payload: Bytes,
}

impl StorageEntry {
    /// Create a new storage entry with the given data
    pub fn new(sequence: u64, timestamp: u64, payload: Bytes) -> Self {
        Self {
            sequence,
            metadata: StorageEntryMetadata {
                timestamp,
                compression: None,
                flags: 0,
                attributes: std::collections::HashMap::new(),
            },
            payload,
        }
    }

    /// Create a new storage entry with custom metadata
    pub fn with_metadata(
        sequence: u64,
        metadata: StorageEntryMetadata,
        payload: Bytes,
    ) -> Self {
        Self {
            sequence,
            metadata,
            payload,
        }
    }

    /// Get the size of this entry in bytes
    pub fn size(&self) -> usize {
        // Base size: sequence (8) + timestamp (8) + payload length
        let base_size = 16 + self.payload.len();
        
        // Add metadata overhead (estimated)
        let metadata_overhead = 32; // Rough estimate for flags, compression type, etc.
        
        // Add attributes overhead
        let attributes_overhead: usize = self.metadata.attributes
            .iter()
            .map(|(k, v)| k.len() + v.len() + 8) // 8 bytes overhead per key-value pair
            .sum();
            
        base_size + metadata_overhead + attributes_overhead
    }

    /// Check if this entry is compressed
    pub fn is_compressed(&self) -> bool {
        matches!(
            self.metadata.compression,
            Some(CompressionType::Lz4 | CompressionType::Zstd | CompressionType::Gzip)
        )
    }

    /// Get the compression ratio if this entry is compressed
    pub fn compression_ratio(&self) -> Option<f64> {
        if self.is_compressed() {
            // In a real implementation, this would track the original size
            // For now, return a placeholder
            Some(0.7) // Assume 30% compression
        } else {
            None
        }
    }

    /// Create an entry with LZ4 compression
    pub fn with_lz4_compression(sequence: u64, timestamp: u64, payload: Bytes) -> Self {
        let metadata = StorageEntryMetadata {
            timestamp,
            compression: Some(CompressionType::Lz4),
            flags: 0,
            attributes: std::collections::HashMap::new(),
        };

        Self {
            sequence,
            metadata,
            payload, // In real implementation, this would be compressed
        }
    }

    /// Add a custom attribute
    pub fn add_attribute(&mut self, key: String, value: String) {
        self.metadata.attributes.insert(key, value);
    }

    /// Get a custom attribute
    pub fn get_attribute(&self, key: &str) -> Option<&String> {
        self.metadata.attributes.get(key)
    }

    /// Set compression type
    pub fn set_compression(&mut self, compression: CompressionType) {
        self.metadata.compression = Some(compression);
    }

    /// Get the age of this entry
    pub fn age(&self) -> std::time::Duration {
        let now = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64;
            
        if now >= self.metadata.timestamp {
            std::time::Duration::from_micros(now - self.metadata.timestamp)
        } else {
            std::time::Duration::ZERO
        }
    }

    /// Check if this entry is older than the specified duration
    pub fn is_older_than(&self, duration: std::time::Duration) -> bool {
        self.age() > duration
    }

    /// Serialize this entry to bytes (placeholder implementation)
    pub fn serialize(&self) -> Result<Vec<u8>> {
        // In a real implementation, this would use an efficient binary format
        // For now, use a simple approach
        let mut buffer = Vec::new();
        
        // Write sequence number
        buffer.extend_from_slice(&self.sequence.to_le_bytes());
        
        // Write timestamp
        buffer.extend_from_slice(&self.metadata.timestamp.to_le_bytes());
        
        // Write payload length
        buffer.extend_from_slice(&(self.payload.len() as u64).to_le_bytes());
        
        // Write payload
        buffer.extend_from_slice(&self.payload);
        
        Ok(buffer)
    }

    /// Deserialize an entry from bytes (placeholder implementation)
    pub fn deserialize(data: &[u8]) -> Result<Self> {
        if data.len() < 24 {
            return Err(StorageError::Io("Insufficient data for entry header".to_string()));
        }
        
        // Read sequence number
        let sequence = u64::from_le_bytes(
            data[0..8].try_into()
                .map_err(|_| StorageError::Io("Invalid sequence number".to_string()))?
        );
        
        // Read timestamp  
        let timestamp = u64::from_le_bytes(
            data[8..16].try_into()
                .map_err(|_| StorageError::Io("Invalid timestamp".to_string()))?
        );
        
        // Read payload length
        let payload_len = u64::from_le_bytes(
            data[16..24].try_into()
                .map_err(|_| StorageError::Io("Invalid payload length".to_string()))?
        ) as usize;
        
        if data.len() < 24 + payload_len {
            return Err(StorageError::Io("Insufficient data for payload".to_string()));
        }
        
        // Read payload
        let payload = Bytes::copy_from_slice(&data[24..24 + payload_len]);
        
        Ok(Self::new(sequence, timestamp, payload))
    }

    /// Validate this entry's integrity
    pub fn validate(&self) -> Result<()> {
        // Check sequence number
        if self.sequence == 0 {
            return Err(StorageError::InvalidOperation("Invalid sequence number: 0".to_string()));
        }
        
        // Check timestamp
        if self.metadata.timestamp == 0 {
            return Err(StorageError::InvalidOperation("Invalid timestamp: 0".to_string()));
        }
        
        // Check payload size (arbitrary limit for safety)
        if self.payload.len() > 100 * 1024 * 1024 { // 100MB
            return Err(StorageError::InvalidOperation("Payload too large".to_string()));
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_entry_creation() {
        let payload = Bytes::from("test data");
        let entry = StorageEntry::new(1, 123456, payload.clone());
        
        assert_eq!(entry.sequence, 1);
        assert_eq!(entry.metadata.timestamp, 123456);
        assert_eq!(entry.payload, payload);
    }

    #[test]
    fn test_storage_entry_size() {
        let payload = Bytes::from("test data");
        let entry = StorageEntry::new(1, 123456, payload);
        
        let size = entry.size();
        assert!(size > 0);
        // Size should be roughly: 16 (base) + 9 (payload) + 32 (metadata overhead)
        assert!(size >= 57);
    }

    #[test]
    fn test_compression_detection() {
        let payload = Bytes::from("test data");
        let mut entry = StorageEntry::new(1, 123456, payload);
        
        assert!(!entry.is_compressed());
        
        entry.set_compression(CompressionType::Lz4);
        assert!(entry.is_compressed());
    }

    #[test]
    fn test_attributes() {
        let payload = Bytes::from("test data");
        let mut entry = StorageEntry::new(1, 123456, payload);
        
        entry.add_attribute("source".to_string(), "test".to_string());
        assert_eq!(entry.get_attribute("source"), Some(&"test".to_string()));
        assert_eq!(entry.get_attribute("missing"), None);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let payload = Bytes::from("test data");
        let entry = StorageEntry::new(1, 123456, payload.clone());
        
        let serialized = entry.serialize().unwrap();
        let deserialized = StorageEntry::deserialize(&serialized).unwrap();
        
        assert_eq!(entry.sequence, deserialized.sequence);
        assert_eq!(entry.metadata.timestamp, deserialized.metadata.timestamp);
        assert_eq!(entry.payload, deserialized.payload);
    }

    #[test]
    fn test_entry_validation() {
        let payload = Bytes::from("test data");
        let entry = StorageEntry::new(1, 123456, payload);
        
        assert!(entry.validate().is_ok());
        
        // Test invalid sequence
        let invalid_entry = StorageEntry::new(0, 123456, Bytes::from("test"));
        assert!(invalid_entry.validate().is_err());
        
        // Test invalid timestamp
        let invalid_entry = StorageEntry::new(1, 0, Bytes::from("test"));
        assert!(invalid_entry.validate().is_err());
    }

    #[test]
    fn test_entry_age() {
        let now = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;
            
        let payload = Bytes::from("test data");
        let entry = StorageEntry::new(1, now - 1000000, payload); // 1 second ago
        
        let age = entry.age();
        assert!(age >= std::time::Duration::from_secs(1));
        assert!(entry.is_older_than(std::time::Duration::from_millis(500)));
    }

    #[test]
    fn test_lz4_compression_entry() {
        let payload = Bytes::from("test data");
        let entry = StorageEntry::with_lz4_compression(1, 123456, payload);
        
        assert!(entry.is_compressed());
        assert!(matches!(entry.metadata.compression, Some(CompressionType::Lz4)));
    }
}