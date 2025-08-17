//! Message Validation Test for MemoryStreamer

const MAX_MESSAGE_SIZE: usize = 1_048_576; // 1 MB

#[derive(Debug, Clone)]
struct Message {
    topic: String,
    payload: Vec<u8>,
    timestamp: u64,
}

impl Message {
    fn new(topic: String, payload: Vec<u8>) -> Result<Self, &'static str> {
        if topic.is_empty() {
            return Err("Empty topic not allowed");
        }

        if payload.len() > MAX_MESSAGE_SIZE {
            return Err("Payload too large");
        }

        Ok(Self {
            topic,
            payload,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        })
    }

    /// Get the topic
    #[must_use]
    pub fn topic(&self) -> &str {
        &self.topic
    }

    /// Get the payload
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Get the timestamp
    #[must_use]
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation_success() {
        let msg = Message::new("test_topic".to_string(), vec![1, 2, 3, 4]);
        assert!(msg.is_ok());

        let msg = msg.unwrap();
        assert_eq!(msg.topic(), "test_topic");
        assert_eq!(msg.payload(), &[1, 2, 3, 4]);
        assert!(msg.timestamp() > 0);
    }

    #[test]
    fn test_message_validation_failure() {
        // Empty topic
        let empty_topic_result = Message::new("".to_string(), vec![1, 2, 3]);
        assert!(empty_topic_result.is_err());

        // Oversized payload
        let large_payload = vec![0; MAX_MESSAGE_SIZE + 1];
        let large_payload_result = Message::new("test_topic".to_string(), large_payload);
        assert!(large_payload_result.is_err());
    }

    #[test]
    fn performance_test_message_creation() {
        let start = std::time::Instant::now();
        let iterations = 100_000;

        for _ in 0..iterations {
            let _msg = Message::new("bench_topic".to_string(), vec![42; 128]).unwrap();
        }

        let duration = start.elapsed();
        println!(
            "Created {} messages in {:?} ({:.2} messages/sec)",
            iterations,
            duration,
            iterations as f64 / duration.as_secs_f64()
        );
    }
}
