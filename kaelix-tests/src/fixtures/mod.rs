//! Test fixtures management for comprehensive testing scenarios.
//!
//! This module provides pre-built test data sets, fixtures management,
//! and utilities for creating consistent test environments.

use kaelix_core::{Message, Topic, Result, Error};
use bytes::Bytes;
use std::path::Path;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use std::sync::Arc;
use parking_lot::RwLock;

pub mod datasets;
pub mod scenarios;

/// Comprehensive test fixtures for various testing scenarios
#[derive(Debug, Clone)]
pub struct TestFixtures {
    /// Pre-generated messages
    pub messages: Vec<Message>,
    /// Topic definitions
    pub topics: Vec<Topic>,
    /// User configurations for testing
    pub users: Vec<User>,
    /// System configurations
    pub configs: Vec<Config>,
    /// Schema definitions
    pub schemas: Vec<Schema>,
    /// Metadata about the fixture set
    pub metadata: FixtureMetadata,
}

/// User configuration for testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    /// User identifier
    pub id: Uuid,
    /// User name
    pub name: String,
    /// User permissions
    pub permissions: Vec<String>,
    /// User groups
    pub groups: Vec<String>,
    /// User metadata
    pub metadata: HashMap<String, String>,
}

/// Configuration object for system testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Configuration name
    pub name: String,
    /// Configuration values
    pub values: HashMap<String, serde_json::Value>,
    /// Configuration version
    pub version: String,
    /// Environment this config applies to
    pub environment: String,
}

/// Schema definition for structured data testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schema {
    /// Schema name
    pub name: String,
    /// Schema version
    pub version: String,
    /// Field definitions
    pub fields: Vec<SchemaField>,
    /// Schema constraints
    pub constraints: Vec<SchemaConstraint>,
}

/// Schema field definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaField {
    /// Field name
    pub name: String,
    /// Field type
    pub field_type: String,
    /// Whether field is required
    pub required: bool,
    /// Default value
    pub default: Option<serde_json::Value>,
    /// Field description
    pub description: Option<String>,
}

/// Schema constraint definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaConstraint {
    /// Constraint type
    pub constraint_type: String,
    /// Fields this constraint applies to
    pub fields: Vec<String>,
    /// Constraint parameters
    pub parameters: HashMap<String, serde_json::Value>,
}

/// Metadata about fixture sets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureMetadata {
    /// Fixture set name
    pub name: String,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Total message count
    pub message_count: usize,
    /// Total data size in bytes
    pub total_size: usize,
    /// Checksum for verification
    pub checksum: String,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Description
    pub description: String,
}

impl TestFixtures {
    /// Create new empty fixture set
    pub fn new(name: &str) -> Self {
        Self {
            messages: Vec::new(),
            topics: Vec::new(),
            users: Vec::new(),
            configs: Vec::new(),
            schemas: Vec::new(),
            metadata: FixtureMetadata {
                name: name.to_string(),
                created_at: chrono::Utc::now(),
                message_count: 0,
                total_size: 0,
                checksum: String::new(),
                tags: Vec::new(),
                description: String::new(),
            },
        }
    }

    // Size-based fixtures

    /// Create tiny fixture set (100 messages)
    pub fn tiny() -> Self {
        Self::create_size_based_fixture("tiny", 100, 64..512)
    }

    /// Create small fixture set (1K messages)
    pub fn small() -> Self {
        Self::create_size_based_fixture("small", 1_000, 128..1024)
    }

    /// Create medium fixture set (100K messages)
    pub fn medium() -> Self {
        Self::create_size_based_fixture("medium", 100_000, 256..2048)
    }

    /// Create large fixture set (10M messages)
    pub fn large() -> Self {
        Self::create_size_based_fixture("large", 10_000_000, 512..4096)
    }

    /// Create huge fixture set (100M messages)
    pub fn huge() -> Self {
        Self::create_size_based_fixture("huge", 100_000_000, 1024..8192)
    }

    // Scenario-based fixtures

    /// Create fixture set with encryption
    pub fn with_encryption() -> Self {
        let mut fixtures = Self::small();
        fixtures.metadata.name = "encrypted".to_string();
        fixtures.metadata.tags.push("encryption".to_string());
        
        // Add encrypted payloads (simulated)
        for message in &mut fixtures.messages {
            let encrypted_payload = Self::simulate_encryption(message.payload());
            message.set_payload(encrypted_payload).unwrap();
        }
        
        fixtures.update_metadata();
        fixtures
    }

    /// Create fixture set with compression
    pub fn with_compression() -> Self {
        let mut fixtures = Self::small();
        fixtures.metadata.name = "compressed".to_string();
        fixtures.metadata.tags.push("compression".to_string());
        
        // Add compressed payloads (simulated)
        for message in &mut fixtures.messages {
            let compressed_payload = Self::simulate_compression(message.payload());
            message.set_payload(compressed_payload).unwrap();
        }
        
        fixtures.update_metadata();
        fixtures
    }

    /// Create fixture set with complex topics
    pub fn with_complex_topics() -> Self {
        let mut fixtures = Self::new("complex_topics");
        fixtures.metadata.tags.push("topics".to_string());
        
        let complex_topics = vec![
            "events.user.{user_id}.profile_updated",
            "metrics.system.{node_id}.{metric_type}",
            "logs.application.{service}.{level}",
            "notifications.{channel}.{priority}",
            "analytics.{product}.{event_type}",
        ];
        
        for topic_pattern in complex_topics {
            fixtures.topics.push(Topic::new(topic_pattern).unwrap());
        }
        
        // Generate messages for each topic
        for topic in &fixtures.topics {
            for i in 0..200 {
                let payload = format!("{{\"id\":{},\"data\":\"test\"}}", i);
                let message = Message::new(topic.as_str(), Bytes::from(payload)).unwrap();
                fixtures.messages.push(message);
            }
        }
        
        fixtures.update_metadata();
        fixtures
    }

    /// Create fixture set with mixed payloads
    pub fn with_mixed_payloads() -> Self {
        let mut fixtures = Self::new("mixed_payloads");
        fixtures.metadata.tags.push("payloads".to_string());
        
        let payload_types = vec![
            ("json", Self::generate_json_payload),
            ("binary", Self::generate_binary_payload),
            ("text", Self::generate_text_payload),
            ("protobuf", Self::generate_protobuf_payload),
            ("avro", Self::generate_avro_payload),
        ];
        
        let topic = Topic::new("mixed_data").unwrap();
        fixtures.topics.push(topic.clone());
        
        for (payload_type, generator) in payload_types {
            for i in 0..1000 {
                let payload = generator(i);
                let mut message = Message::new(topic.as_str(), payload).unwrap();
                message.set_header("payload_type", payload_type).unwrap();
                fixtures.messages.push(message);
            }
        }
        
        fixtures.update_metadata();
        fixtures
    }

    // Performance fixtures

    /// Create fixture set optimized for throughput testing
    pub fn throughput_test() -> Self {
        let mut fixtures = Self::create_size_based_fixture("throughput", 1_000_000, 1024..1024);
        fixtures.metadata.tags.push("performance".to_string());
        fixtures.metadata.tags.push("throughput".to_string());
        
        // Optimize for throughput - consistent sizes, simple payloads
        fixtures
    }

    /// Create fixture set optimized for latency testing
    pub fn latency_test() -> Self {
        let mut fixtures = Self::create_size_based_fixture("latency", 10_000, 64..64);
        fixtures.metadata.tags.push("performance".to_string());
        fixtures.metadata.tags.push("latency".to_string());
        
        // Optimize for latency - small, consistent messages
        fixtures
    }

    /// Create fixture set for memory testing
    pub fn memory_test() -> Self {
        let mut fixtures = Self::create_size_based_fixture("memory", 100_000, 8192..16384);
        fixtures.metadata.tags.push("performance".to_string());
        fixtures.metadata.tags.push("memory".to_string());
        
        // Large messages to test memory handling
        fixtures
    }

    // Data management methods

    /// Load fixtures from file
    pub async fn load_from_file(&mut self, path: &Path) -> Result<()> {
        let contents = tokio::fs::read_to_string(path).await
            .map_err(|e| Error::IO { 
                message: format!("Failed to read fixture file: {}", e),
                source: Box::new(e),
            })?;
        
        let data: SerializedFixtures = serde_json::from_str(&contents)
            .map_err(|e| Error::Serialization {
                message: format!("Failed to deserialize fixtures: {}", e),
                source: Box::new(e),
            })?;
        
        *self = data.into_fixtures()?;
        Ok(())
    }

    /// Save fixtures to file
    pub async fn save_to_file(&self, path: &Path) -> Result<()> {
        let serialized = SerializedFixtures::from_fixtures(self)?;
        let contents = serde_json::to_string_pretty(&serialized)
            .map_err(|e| Error::Serialization {
                message: format!("Failed to serialize fixtures: {}", e),
                source: Box::new(e),
            })?;
        
        tokio::fs::write(path, contents).await
            .map_err(|e| Error::IO {
                message: format!("Failed to write fixture file: {}", e),
                source: Box::new(e),
            })?;
        
        Ok(())
    }

    /// Shuffle messages for randomization
    pub fn shuffle(&mut self) {
        use rand::seq::SliceRandom;
        let mut rng = rand::thread_rng();
        self.messages.shuffle(&mut rng);
        self.update_metadata();
    }

    /// Create subset of fixtures
    pub fn subset(&self, count: usize) -> Self {
        let subset_count = count.min(self.messages.len());
        let mut subset = Self::new(&format!("{}_subset", self.metadata.name));
        
        subset.messages = self.messages[..subset_count].to_vec();
        subset.topics = self.topics.clone();
        subset.users = self.users.clone();
        subset.configs = self.configs.clone();
        subset.schemas = self.schemas.clone();
        
        subset.update_metadata();
        subset
    }

    // Helper methods

    /// Create size-based fixture
    fn create_size_based_fixture(name: &str, count: usize, size_range: std::ops::Range<usize>) -> Self {
        let mut fixtures = Self::new(name);
        
        // Create default topic
        let topic = Topic::new("test-topic").unwrap();
        fixtures.topics.push(topic.clone());
        
        // Generate messages
        for i in 0..count {
            let size = rand::random::<usize>() % (size_range.end - size_range.start) + size_range.start;
            let payload = vec![0u8; size];
            let message = Message::new(topic.as_str(), Bytes::from(payload)).unwrap();
            fixtures.messages.push(message);
        }
        
        fixtures.update_metadata();
        fixtures
    }

    /// Update fixture metadata
    fn update_metadata(&mut self) {
        self.metadata.message_count = self.messages.len();
        self.metadata.total_size = self.messages.iter()
            .map(|m| m.payload().len())
            .sum();
        self.metadata.checksum = self.calculate_checksum();
    }

    /// Calculate checksum for verification
    fn calculate_checksum(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        
        for message in &self.messages {
            message.topic().as_str().hash(&mut hasher);
            message.payload().hash(&mut hasher);
        }
        
        format!("{:x}", hasher.finish())
    }

    /// Simulate encryption (for testing)
    fn simulate_encryption(payload: &Bytes) -> Bytes {
        // Simple XOR "encryption" for testing
        let key = 0xAA;
        let encrypted: Vec<u8> = payload.iter().map(|&b| b ^ key).collect();
        Bytes::from(encrypted)
    }

    /// Simulate compression (for testing)
    fn simulate_compression(payload: &Bytes) -> Bytes {
        // Simulate compression by reducing size
        let compressed_size = (payload.len() as f64 * 0.7) as usize;
        Bytes::from(payload[..compressed_size.min(payload.len())].to_vec())
    }

    /// Generate JSON payload
    fn generate_json_payload(id: usize) -> Bytes {
        let json = serde_json::json!({
            "id": id,
            "timestamp": chrono::Utc::now(),
            "type": "json",
            "data": {
                "value": id * 10,
                "description": format!("JSON payload {}", id)
            }
        });
        Bytes::from(json.to_string())
    }

    /// Generate binary payload
    fn generate_binary_payload(id: usize) -> Bytes {
        let mut data = Vec::new();
        data.extend_from_slice(&(id as u64).to_be_bytes());
        data.extend_from_slice(&chrono::Utc::now().timestamp().to_be_bytes());
        data.extend_from_slice(b"BINARY_DATA");
        data.resize(256, id as u8);
        Bytes::from(data)
    }

    /// Generate text payload
    fn generate_text_payload(id: usize) -> Bytes {
        let text = format!(
            "Text message #{}\nTimestamp: {}\nThis is a text payload for testing purposes.\n{}",
            id,
            chrono::Utc::now(),
            "Lorem ipsum dolor sit amet, consectetur adipiscing elit.".repeat(id % 5 + 1)
        );
        Bytes::from(text)
    }

    /// Generate protobuf-like payload (simulated)
    fn generate_protobuf_payload(id: usize) -> Bytes {
        // Simulate protobuf binary format
        let mut data = Vec::new();
        data.push(0x08); // Field 1, varint
        data.extend_from_slice(&Self::encode_varint(id as u64));
        data.push(0x12); // Field 2, length-delimited
        let timestamp = chrono::Utc::now().timestamp();
        data.push(8); // Length
        data.extend_from_slice(&timestamp.to_be_bytes());
        Bytes::from(data)
    }

    /// Generate Avro-like payload (simulated)
    fn generate_avro_payload(id: usize) -> Bytes {
        // Simulate Avro binary format
        let mut data = Vec::new();
        data.extend_from_slice(&Self::encode_varint(id as u64));
        data.extend_from_slice(&Self::encode_varint(chrono::Utc::now().timestamp() as u64));
        let text = format!("avro_data_{}", id);
        data.extend_from_slice(&Self::encode_varint(text.len() as u64));
        data.extend_from_slice(text.as_bytes());
        Bytes::from(data)
    }

    /// Encode varint (simplified)
    fn encode_varint(mut value: u64) -> Vec<u8> {
        let mut bytes = Vec::new();
        while value >= 0x80 {
            bytes.push((value & 0x7F | 0x80) as u8);
            value >>= 7;
        }
        bytes.push(value as u8);
        bytes
    }
}

/// Serializable version of fixtures for file I/O
#[derive(Serialize, Deserialize)]
struct SerializedFixtures {
    messages: Vec<SerializedMessage>,
    topics: Vec<String>,
    users: Vec<User>,
    configs: Vec<Config>,
    schemas: Vec<Schema>,
    metadata: FixtureMetadata,
}

#[derive(Serialize, Deserialize)]
struct SerializedMessage {
    topic: String,
    payload: Vec<u8>,
    headers: HashMap<String, String>,
}

impl SerializedFixtures {
    fn from_fixtures(fixtures: &TestFixtures) -> Result<Self> {
        let messages = fixtures.messages.iter()
            .map(|m| SerializedMessage {
                topic: m.topic().as_str().to_string(),
                payload: m.payload().to_vec(),
                headers: m.headers().iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            })
            .collect();
        
        Ok(Self {
            messages,
            topics: fixtures.topics.iter().map(|t| t.as_str().to_string()).collect(),
            users: fixtures.users.clone(),
            configs: fixtures.configs.clone(),
            schemas: fixtures.schemas.clone(),
            metadata: fixtures.metadata.clone(),
        })
    }

    fn into_fixtures(self) -> Result<TestFixtures> {
        let topics: Result<Vec<_>> = self.topics.into_iter()
            .map(|t| Topic::new(t))
            .collect();
        
        let messages: Result<Vec<_>> = self.messages.into_iter()
            .map(|m| {
                let mut message = Message::new(&m.topic, Bytes::from(m.payload))?;
                for (key, value) in m.headers {
                    message.set_header(&key, &value)?;
                }
                Ok(message)
            })
            .collect();
        
        Ok(TestFixtures {
            messages: messages?,
            topics: topics?,
            users: self.users,
            configs: self.configs,
            schemas: self.schemas,
            metadata: self.metadata,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_size_based_fixtures() {
        let fixtures = TestFixtures::tiny();
        assert_eq!(fixtures.messages.len(), 100);
        assert_eq!(fixtures.metadata.message_count, 100);
        assert!(!fixtures.metadata.checksum.is_empty());
    }

    #[test]
    fn test_scenario_based_fixtures() {
        let fixtures = TestFixtures::with_complex_topics();
        assert!(fixtures.topics.len() > 1);
        assert!(fixtures.messages.len() > 100);
        assert!(fixtures.metadata.tags.contains(&"topics".to_string()));
    }

    #[test]
    fn test_mixed_payloads() {
        let fixtures = TestFixtures::with_mixed_payloads();
        
        // Should have different payload types
        let payload_types: std::collections::HashSet<_> = fixtures.messages.iter()
            .filter_map(|m| m.headers().get("payload_type"))
            .collect();
        
        assert!(payload_types.len() > 1);
    }

    #[test]
    fn test_fixture_subset() {
        let fixtures = TestFixtures::small();
        let subset = fixtures.subset(100);
        
        assert_eq!(subset.messages.len(), 100);
        assert_eq!(subset.topics.len(), fixtures.topics.len());
    }

    #[test]
    fn test_shuffle() {
        let mut fixtures = TestFixtures::tiny();
        let original_first = fixtures.messages[0].clone();
        
        fixtures.shuffle();
        
        // After shuffle, first message might be different
        // (this test might occasionally fail due to randomness)
        let shuffled_messages = fixtures.messages.clone();
        fixtures.shuffle();
        
        // Multiple shuffles should produce different orders
        assert_ne!(shuffled_messages, fixtures.messages);
    }

    #[tokio::test]
    async fn test_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test_fixtures.json");
        
        let original = TestFixtures::tiny();
        original.save_to_file(&file_path).await.unwrap();
        
        let mut loaded = TestFixtures::new("empty");
        loaded.load_from_file(&file_path).await.unwrap();
        
        assert_eq!(loaded.messages.len(), original.messages.len());
        assert_eq!(loaded.metadata.checksum, original.metadata.checksum);
    }

    #[test]
    fn test_compression_simulation() {
        let original_payload = Bytes::from(vec![1, 2, 3, 4, 5]);
        let compressed = TestFixtures::simulate_compression(&original_payload);
        
        assert!(compressed.len() <= original_payload.len());
    }

    #[test]
    fn test_encryption_simulation() {
        let original_payload = Bytes::from(vec![1, 2, 3, 4, 5]);
        let encrypted = TestFixtures::simulate_encryption(&original_payload);
        
        assert_eq!(encrypted.len(), original_payload.len());
        assert_ne!(encrypted, original_payload);
        
        // Double encryption should return original
        let double_encrypted = TestFixtures::simulate_encryption(&encrypted);
        assert_eq!(double_encrypted, original_payload);
    }
}