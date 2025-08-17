//! Memory-efficient streaming data generation.
//!
//! This module provides generators that can create massive datasets
//! without storing everything in memory, using streaming and disk-based approaches.

use crate::generators::data::{MessageGenerator, GenerationConfig};
use kaelix_core::{Message, Result, Error};
use futures::{Stream, StreamExt};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::{AsyncWriteExt, BufWriter};
use bytes::Bytes;
use serde::{Serialize, Deserialize};

/// Configuration for memory-efficient generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationConfig {
    /// Total number of messages to generate
    pub message_count: u64,
    /// Buffer size for streaming (messages)
    pub buffer_size: usize,
    /// Chunk size for disk writes (bytes)
    pub chunk_size: usize,
    /// Enable compression for disk storage
    pub compress: bool,
    /// Generation parameters
    pub generator_config: crate::generators::data::PayloadDistribution,
    /// Topic patterns
    pub topic_patterns: Vec<String>,
    /// Output format
    pub output_format: OutputFormat,
}

/// Output format for generated data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputFormat {
    /// Binary format (most efficient)
    Binary,
    /// JSON Lines format (human readable)
    JsonLines,
    /// Apache Parquet (columnar, compressed)
    Parquet,
    /// Custom format with specified delimiter
    Delimited { delimiter: String },
}

/// Memory-efficient streaming generator
#[derive(Debug)]
pub struct StreamingGenerator {
    /// Generation configuration
    config: GenerationConfig,
    /// Buffer size for in-memory operations
    buffer_size: usize,
    /// Message generator
    message_generator: MessageGenerator,
    /// Progress counter
    progress: Arc<AtomicU64>,
}

/// Generation statistics
#[derive(Debug, Clone, Default)]
pub struct GenerationStats {
    /// Messages generated
    pub messages_generated: u64,
    /// Bytes written
    pub bytes_written: u64,
    /// Generation rate (messages/sec)
    pub generation_rate: f64,
    /// Write rate (bytes/sec)
    pub write_rate: f64,
    /// Peak memory usage (bytes)
    pub peak_memory_usage: usize,
    /// Compression ratio (if enabled)
    pub compression_ratio: Option<f64>,
}

/// Serializable message for output
#[derive(Serialize, Deserialize)]
struct SerializableMessage {
    topic: String,
    payload: Vec<u8>,
    timestamp: i64,
    headers: std::collections::HashMap<String, String>,
}

impl StreamingGenerator {
    /// Create new streaming generator
    pub fn new(config: GenerationConfig) -> Self {
        let message_generator = MessageGenerator::new()
            .with_distribution(config.generator_config.clone())
            .with_topic_patterns(config.topic_patterns.clone());

        Self {
            buffer_size: config.buffer_size,
            config,
            message_generator,
            progress: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Generate stream of messages without storing in memory
    pub fn stream_messages(&self) -> impl Stream<Item = Message> + '_ {
        let total_messages = self.config.message_count;
        let progress = Arc::clone(&self.progress);

        async_stream::stream! {
            let mut generated = 0u64;
            
            while generated < total_messages {
                // Generate message on-demand
                if let Ok(message) = self.message_generator.generate_single().await {
                    progress.store(generated, Ordering::Relaxed);
                    generated += 1;
                    yield message;
                }
            }
        }
    }

    /// Generate data directly to disk without memory accumulation
    pub async fn generate_to_disk(&self, path: &Path, count: u64) -> Result<GenerationStats> {
        let start_time = std::time::Instant::now();
        let file = File::create(path).await?;
        let mut writer = BufWriter::with_capacity(self.config.chunk_size, file);
        
        let mut stats = GenerationStats::default();
        let mut buffer = Vec::with_capacity(self.buffer_size);
        let mut messages_written = 0u64;

        // Generate and write in chunks
        let stream = self.stream_messages().take(count as usize);
        tokio::pin!(stream);

        while let Some(message) = stream.next().await {
            buffer.push(message);

            // Write buffer when full
            if buffer.len() >= self.buffer_size {
                let chunk_bytes = self.write_chunk(&mut writer, &buffer).await?;
                stats.bytes_written += chunk_bytes;
                messages_written += buffer.len() as u64;
                buffer.clear();

                // Update progress
                self.progress.store(messages_written, Ordering::Relaxed);
            }
        }

        // Write remaining messages
        if !buffer.is_empty() {
            let chunk_bytes = self.write_chunk(&mut writer, &buffer).await?;
            stats.bytes_written += chunk_bytes;
            messages_written += buffer.len() as u64;
        }

        writer.flush().await?;
        let elapsed = start_time.elapsed();

        stats.messages_generated = messages_written;
        stats.generation_rate = messages_written as f64 / elapsed.as_secs_f64();
        stats.write_rate = stats.bytes_written as f64 / elapsed.as_secs_f64();
        stats.peak_memory_usage = self.estimate_memory_usage_internal();

        Ok(stats)
    }

    /// Estimate memory usage for given message count
    pub fn estimate_memory_usage(&self, count: u64) -> usize {
        let avg_message_size = self.estimate_avg_message_size();
        let buffer_memory = self.buffer_size * avg_message_size;
        let generator_overhead = 1024 * 1024; // 1MB overhead
        let streaming_overhead = self.config.chunk_size;
        
        buffer_memory + generator_overhead + streaming_overhead
    }

    /// Generate data in parallel chunks for maximum efficiency
    pub async fn generate_parallel_chunks(&self, path: &Path, count: u64, parallelism: usize) -> Result<GenerationStats> {
        let chunk_size = count / parallelism as u64;
        let mut handles = Vec::new();
        let start_time = std::time::Instant::now();

        // Spawn parallel generation tasks
        for i in 0..parallelism {
            let chunk_path = path.with_extension(&format!("chunk_{}", i));
            let generator = StreamingGenerator::new(self.config.clone());
            let chunk_count = if i == parallelism - 1 {
                count - (chunk_size * (parallelism - 1) as u64) // Last chunk gets remainder
            } else {
                chunk_size
            };

            let handle = tokio::spawn(async move {
                generator.generate_to_disk(&chunk_path, chunk_count).await
            });
            handles.push(handle);
        }

        // Collect results and merge statistics
        let mut total_stats = GenerationStats::default();
        for handle in handles {
            let chunk_stats = handle.await??;
            total_stats.messages_generated += chunk_stats.messages_generated;
            total_stats.bytes_written += chunk_stats.bytes_written;
            total_stats.peak_memory_usage = total_stats.peak_memory_usage.max(chunk_stats.peak_memory_usage);
        }

        let elapsed = start_time.elapsed();
        total_stats.generation_rate = total_stats.messages_generated as f64 / elapsed.as_secs_f64();
        total_stats.write_rate = total_stats.bytes_written as f64 / elapsed.as_secs_f64();

        // Merge chunk files into single output
        self.merge_chunk_files(path, parallelism).await?;

        Ok(total_stats)
    }

    /// Write a chunk of messages to the writer
    async fn write_chunk(&self, writer: &mut BufWriter<File>, messages: &[Message]) -> Result<u64> {
        let mut bytes_written = 0u64;

        match &self.config.output_format {
            OutputFormat::Binary => {
                bytes_written += self.write_binary_chunk(writer, messages).await?;
            }
            OutputFormat::JsonLines => {
                bytes_written += self.write_jsonl_chunk(writer, messages).await?;
            }
            OutputFormat::Parquet => {
                // Parquet would require specialized handling
                return Err(Error::NotImplemented {
                    message: "Parquet output not implemented".to_string(),
                });
            }
            OutputFormat::Delimited { delimiter } => {
                bytes_written += self.write_delimited_chunk(writer, messages, delimiter).await?;
            }
        }

        Ok(bytes_written)
    }

    /// Write messages in binary format
    async fn write_binary_chunk(&self, writer: &mut BufWriter<File>, messages: &[Message]) -> Result<u64> {
        let mut bytes_written = 0u64;

        for message in messages {
            // Write message length
            let topic_bytes = message.topic().as_str().as_bytes();
            let payload_len = message.payload().len() as u32;
            let topic_len = topic_bytes.len() as u32;

            writer.write_all(&topic_len.to_le_bytes()).await?;
            writer.write_all(topic_bytes).await?;
            writer.write_all(&payload_len.to_le_bytes()).await?;
            writer.write_all(message.payload()).await?;

            bytes_written += 4 + topic_len as u64 + 4 + payload_len as u64;
        }

        Ok(bytes_written)
    }

    /// Write messages in JSON Lines format
    async fn write_jsonl_chunk(&self, writer: &mut BufWriter<File>, messages: &[Message]) -> Result<u64> {
        let mut bytes_written = 0u64;

        for message in messages {
            let serializable = SerializableMessage {
                topic: message.topic().as_str().to_string(),
                payload: message.payload().to_vec(),
                timestamp: chrono::Utc::now().timestamp(),
                headers: message.headers().clone(),
            };

            let json_line = serde_json::to_string(&serializable)?;
            let line_with_newline = format!("{}\n", json_line);
            
            writer.write_all(line_with_newline.as_bytes()).await?;
            bytes_written += line_with_newline.len() as u64;
        }

        Ok(bytes_written)
    }

    /// Write messages in delimited format
    async fn write_delimited_chunk(&self, writer: &mut BufWriter<File>, messages: &[Message], delimiter: &str) -> Result<u64> {
        let mut bytes_written = 0u64;

        for message in messages {
            let line = format!(
                "{}{}{}{}{}{}{}{}{}{}{}",
                message.topic().as_str(),
                delimiter,
                chrono::Utc::now().timestamp(),
                delimiter,
                message.payload().len(),
                delimiter,
                base64::encode(message.payload()),
                delimiter,
                message.headers().len(),
                delimiter,
                serde_json::to_string(message.headers()).unwrap_or_default()
            );
            
            writer.write_all(line.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            bytes_written += line.len() as u64 + 1;
        }

        Ok(bytes_written)
    }

    /// Merge parallel chunk files into single output
    async fn merge_chunk_files(&self, output_path: &Path, num_chunks: usize) -> Result<()> {
        use tokio::io::{AsyncReadExt, BufReader};

        let mut output_file = File::create(output_path).await?;
        
        for i in 0..num_chunks {
            let chunk_path = output_path.with_extension(&format!("chunk_{}", i));
            let chunk_file = File::open(&chunk_path).await?;
            let mut reader = BufReader::new(chunk_file);
            
            let mut buffer = vec![0u8; self.config.chunk_size];
            loop {
                let bytes_read = reader.read(&mut buffer).await?;
                if bytes_read == 0 {
                    break;
                }
                output_file.write_all(&buffer[..bytes_read]).await?;
            }
            
            // Clean up chunk file
            tokio::fs::remove_file(&chunk_path).await?;
        }

        Ok(())
    }

    /// Estimate average message size based on distribution
    fn estimate_avg_message_size(&self) -> usize {
        match &self.config.generator_config {
            crate::generators::data::PayloadDistribution::Uniform { min, max } => {
                (min + max) / 2
            }
            crate::generators::data::PayloadDistribution::Normal { mean, .. } => {
                *mean as usize
            }
            crate::generators::data::PayloadDistribution::Exponential { lambda } => {
                (1.0 / lambda) as usize
            }
            crate::generators::data::PayloadDistribution::Pareto { scale, .. } => {
                *scale as usize * 2 // Rough estimate
            }
            crate::generators::data::PayloadDistribution::Realistic => {
                1024 // Default realistic size
            }
        }
    }

    /// Internal memory usage estimation
    fn estimate_memory_usage_internal(&self) -> usize {
        let avg_message_size = self.estimate_avg_message_size();
        self.buffer_size * avg_message_size + 1024 * 1024 // Add 1MB overhead
    }

    /// Get current progress (messages generated)
    pub fn progress(&self) -> u64 {
        self.progress.load(Ordering::Relaxed)
    }

    /// Calculate progress percentage
    pub fn progress_percentage(&self) -> f64 {
        let current = self.progress() as f64;
        let total = self.config.message_count as f64;
        if total > 0.0 {
            (current / total) * 100.0
        } else {
            0.0
        }
    }
}

/// Utility for creating streaming generators with common configurations
pub struct StreamingGeneratorBuilder {
    config: GenerationConfig,
}

impl StreamingGeneratorBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            config: GenerationConfig {
                message_count: 1_000_000,
                buffer_size: 1000,
                chunk_size: 64 * 1024, // 64KB chunks
                compress: false,
                generator_config: crate::generators::data::PayloadDistribution::Uniform { min: 256, max: 1024 },
                topic_patterns: vec!["streaming-data".to_string()],
                output_format: OutputFormat::Binary,
            },
        }
    }

    /// Set message count
    pub fn message_count(mut self, count: u64) -> Self {
        self.config.message_count = count;
        self
    }

    /// Set buffer size
    pub fn buffer_size(mut self, size: usize) -> Self {
        self.config.buffer_size = size;
        self
    }

    /// Set chunk size for disk writes
    pub fn chunk_size(mut self, size: usize) -> Self {
        self.config.chunk_size = size;
        self
    }

    /// Enable compression
    pub fn with_compression(mut self) -> Self {
        self.config.compress = true;
        self
    }

    /// Set payload distribution
    pub fn payload_distribution(mut self, distribution: crate::generators::data::PayloadDistribution) -> Self {
        self.config.generator_config = distribution;
        self
    }

    /// Set topic patterns
    pub fn topic_patterns(mut self, patterns: Vec<String>) -> Self {
        self.config.topic_patterns = patterns;
        self
    }

    /// Set output format
    pub fn output_format(mut self, format: OutputFormat) -> Self {
        self.config.output_format = format;
        self
    }

    /// Build the generator
    pub fn build(self) -> StreamingGenerator {
        StreamingGenerator::new(self.config)
    }
}

impl Default for StreamingGeneratorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_streaming_generator_basic() {
        let generator = StreamingGeneratorBuilder::new()
            .message_count(100)
            .buffer_size(10)
            .build();

        let messages: Vec<_> = generator.stream_messages().take(50).collect().await;
        assert_eq!(messages.len(), 50);
    }

    #[tokio::test]
    async fn test_generate_to_disk() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("test_output.bin");

        let generator = StreamingGeneratorBuilder::new()
            .message_count(1000)
            .buffer_size(100)
            .output_format(OutputFormat::Binary)
            .build();

        let stats = generator.generate_to_disk(&output_path, 1000).await.unwrap();
        
        assert_eq!(stats.messages_generated, 1000);
        assert!(stats.bytes_written > 0);
        assert!(output_path.exists());
    }

    #[tokio::test]
    async fn test_jsonl_output() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("test_output.jsonl");

        let generator = StreamingGeneratorBuilder::new()
            .message_count(100)
            .buffer_size(20)
            .output_format(OutputFormat::JsonLines)
            .build();

        let stats = generator.generate_to_disk(&output_path, 100).await.unwrap();
        
        assert_eq!(stats.messages_generated, 100);
        assert!(stats.bytes_written > 0);

        // Verify JSON Lines format
        let content = tokio::fs::read_to_string(&output_path).await.unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 100);

        // Each line should be valid JSON
        for line in lines.iter().take(5) {
            let _: SerializableMessage = serde_json::from_str(line).unwrap();
        }
    }

    #[tokio::test]
    async fn test_parallel_generation() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("parallel_output.bin");

        let generator = StreamingGeneratorBuilder::new()
            .message_count(1000)
            .buffer_size(50)
            .build();

        let stats = generator.generate_parallel_chunks(&output_path, 1000, 4).await.unwrap();
        
        assert_eq!(stats.messages_generated, 1000);
        assert!(stats.bytes_written > 0);
        assert!(output_path.exists());
    }

    #[test]
    fn test_memory_estimation() {
        let generator = StreamingGeneratorBuilder::new()
            .message_count(1_000_000)
            .buffer_size(1000)
            .payload_distribution(crate::generators::data::PayloadDistribution::Uniform { min: 512, max: 1024 })
            .build();

        let estimated = generator.estimate_memory_usage(1_000_000);
        
        // Should be reasonable - not storing all messages
        assert!(estimated < 100 * 1024 * 1024); // Less than 100MB
        assert!(estimated > 1024 * 1024); // More than 1MB
    }

    #[test]
    fn test_progress_tracking() {
        let generator = StreamingGeneratorBuilder::new()
            .message_count(100)
            .build();

        assert_eq!(generator.progress(), 0);
        assert_eq!(generator.progress_percentage(), 0.0);
    }

    #[test]
    fn test_builder_pattern() {
        let generator = StreamingGeneratorBuilder::new()
            .message_count(500_000)
            .buffer_size(2000)
            .chunk_size(128 * 1024)
            .with_compression()
            .topic_patterns(vec!["test.topic".to_string()])
            .output_format(OutputFormat::JsonLines)
            .build();

        assert_eq!(generator.config.message_count, 500_000);
        assert_eq!(generator.config.buffer_size, 2000);
        assert_eq!(generator.config.chunk_size, 128 * 1024);
        assert!(generator.config.compress);
    }
}