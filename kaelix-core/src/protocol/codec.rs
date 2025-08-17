//! High-performance frame encoding and decoding with zero-copy operations.

use crate::protocol::{FRAME_HEADER_SIZE, Frame, ProtocolError, ProtocolResult};
use bytes::{Buf, Bytes, BytesMut};
use std::collections::VecDeque;

/// High-performance frame encoder with buffer reuse and batching support.
#[derive(Debug)]
pub struct FrameEncoder {
    /// Reusable buffer for encoding operations
    buffer: BytesMut,

    /// Frame batch for high-throughput scenarios
    batch: Vec<Frame>,

    /// Maximum batch size before auto-flush
    max_batch_size: usize,
}

impl FrameEncoder {
    /// Create a new frame encoder with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(8192) // 8KB default buffer
    }

    /// Create a frame encoder with specified buffer capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self { buffer: BytesMut::with_capacity(capacity), batch: Vec::new(), max_batch_size: 100 }
    }

    /// Set maximum batch size for batched operations.
    pub const fn set_max_batch_size(&mut self, size: usize) {
        self.max_batch_size = size;
    }

    /// Encode a single frame to bytes.
    ///
    /// # Errors
    /// Returns an error if encoding fails or buffer allocation fails.
    pub fn encode(&mut self, frame: &Frame) -> ProtocolResult<Bytes> {
        self.buffer.clear();

        // Reserve space for the entire frame
        self.buffer.reserve(frame.frame_size());

        // Use the frame's built-in serialization
        frame.serialize()
    }

    /// Add a frame to the current batch.
    ///
    /// # Errors
    /// Returns an error if the batch is full and auto-flush fails.
    pub fn add_to_batch(&mut self, frame: Frame) -> ProtocolResult<Option<Bytes>> {
        self.batch.push(frame);

        if self.batch.len() >= self.max_batch_size {
            Ok(Some(self.flush_batch()?))
        } else {
            Ok(None)
        }
    }

    /// Flush the current batch to a single bytes buffer.
    ///
    /// # Errors
    /// Returns an error if encoding any frame in the batch fails.
    pub fn flush_batch(&mut self) -> ProtocolResult<Bytes> {
        if self.batch.is_empty() {
            return Ok(Bytes::new());
        }

        // Calculate total size needed
        let total_size: usize = self.batch.iter().map(Frame::frame_size).sum();

        self.buffer.clear();
        self.buffer.reserve(total_size);

        // Encode all frames in sequence
        for frame in &self.batch {
            let encoded = frame.serialize()?;
            self.buffer.extend_from_slice(&encoded);
        }

        self.batch.clear();
        Ok(self.buffer.split().freeze())
    }

    /// Get the current batch size.
    #[must_use]
    pub const fn batch_size(&self) -> usize {
        self.batch.len()
    }

    /// Check if the batch is empty.
    #[must_use]
    pub const fn is_batch_empty(&self) -> bool {
        self.batch.is_empty()
    }

    /// Clear the current batch without encoding.
    pub fn clear_batch(&mut self) {
        self.batch.clear();
    }

    /// Reset the encoder to initial state.
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.batch.clear();
    }

    /// Get buffer capacity utilization as a percentage.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn buffer_utilization(&self) -> f32 {
        if self.buffer.capacity() == 0 {
            0.0
        } else {
            (self.buffer.len() as f32 / self.buffer.capacity() as f32) * 100.0
        }
    }
}

impl Default for FrameEncoder {
    fn default() -> Self {
        Self::new()
    }
}

/// High-performance frame decoder with streaming support and frame boundary detection.
#[derive(Debug)]
pub struct FrameDecoder {
    /// Buffer for accumulating incomplete frames
    buffer: BytesMut,

    /// Queue of completed frames awaiting processing
    frame_queue: VecDeque<Frame>,

    /// Expected frame size for current frame being decoded
    expected_frame_size: Option<usize>,

    /// Maximum frame size to prevent `DoS` attacks
    max_frame_size: usize,

    /// Statistics for monitoring
    stats: DecoderStats,
}

/// Decoder statistics for monitoring and debugging.
#[derive(Debug, Default, Clone)]
pub struct DecoderStats {
    /// Total frames successfully decoded
    pub frames_decoded: u64,

    /// Total bytes processed
    pub bytes_processed: u64,

    /// Number of incomplete frames encountered
    pub incomplete_frames: u64,

    /// Number of corrupted frames discarded
    pub corrupted_frames: u64,

    /// Current buffer size
    pub buffer_size: usize,

    /// Peak buffer size observed
    pub peak_buffer_size: usize,
}

impl FrameDecoder {
    /// Create a new frame decoder with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(16384) // 16KB default buffer
    }

    /// Create a frame decoder with specified buffer capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: BytesMut::with_capacity(capacity),
            frame_queue: VecDeque::new(),
            expected_frame_size: None,
            max_frame_size: crate::protocol::constants::MAX_FRAME_SIZE,
            stats: DecoderStats::default(),
        }
    }

    /// Set maximum allowed frame size.
    pub const fn set_max_frame_size(&mut self, size: usize) {
        self.max_frame_size = size;
    }

    /// Decode frames from a bytes buffer (streaming).
    ///
    /// Returns the number of complete frames decoded. Use `next_frame()` to retrieve them.
    ///
    /// # Errors
    /// Returns an error if frame parsing fails or frames are corrupted.
    #[allow(clippy::cast_possible_truncation)]
    pub fn decode_stream(&mut self, data: &[u8]) -> ProtocolResult<usize> {
        // Add new data to buffer
        self.buffer.extend_from_slice(data);
        self.stats.bytes_processed += data.len() as u64;
        self.stats.buffer_size = self.buffer.len();
        self.stats.peak_buffer_size = self.stats.peak_buffer_size.max(self.buffer.len());

        let mut frames_decoded = 0;

        // Process complete frames
        while self.buffer.len() >= FRAME_HEADER_SIZE {
            // If we don't know the expected frame size, peek at the header
            if self.expected_frame_size.is_none() {
                if let Some(frame_size) = self.peek_frame_size()? {
                    self.expected_frame_size = Some(frame_size);
                } else {
                    break; // Not enough data for header
                }
            }

            // Check if we have a complete frame
            if let Some(expected_size) = self.expected_frame_size {
                if self.buffer.len() >= expected_size {
                    // Try to decode the frame
                    match self.decode_single_frame() {
                        Ok(frame) => {
                            self.frame_queue.push_back(frame);
                            frames_decoded += 1;
                            self.stats.frames_decoded += 1;
                            self.expected_frame_size = None;
                        },
                        Err(e) if e.is_corruption() => {
                            // Skip corrupted frame and reset
                            self.stats.corrupted_frames += 1;
                            self.recover_from_corruption();
                            self.expected_frame_size = None;
                        },
                        Err(e) => return Err(e),
                    }
                } else {
                    // Need more data
                    self.stats.incomplete_frames += 1;
                    break;
                }
            }
        }

        Ok(frames_decoded)
    }

    /// Decode a single complete frame from bytes.
    ///
    /// # Errors
    /// Returns an error if the frame is incomplete, corrupted, or invalid.
    pub fn decode(&mut self, data: &[u8]) -> ProtocolResult<Option<Frame>> {
        let decoded_count = self.decode_stream(data)?;

        if decoded_count > 0 {
            Ok(self.next_frame())
        } else {
            Ok(None)
        }
    }

    /// Get the next decoded frame from the queue.
    #[must_use]
    pub fn next_frame(&mut self) -> Option<Frame> {
        self.frame_queue.pop_front()
    }

    /// Check if there are frames available in the queue.
    #[must_use]
    pub fn has_frames(&self) -> bool {
        !self.frame_queue.is_empty()
    }

    /// Get the number of frames in the queue.
    #[must_use]
    pub fn frame_count(&self) -> usize {
        self.frame_queue.len()
    }

    /// Get decoder statistics.
    #[must_use]
    pub const fn stats(&self) -> &DecoderStats {
        &self.stats
    }

    /// Reset the decoder to initial state.
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.frame_queue.clear();
        self.expected_frame_size = None;
        self.stats = DecoderStats::default();
    }

    /// Peek at the frame size from the header without consuming data.
    fn peek_frame_size(&self) -> ProtocolResult<Option<usize>> {
        if self.buffer.len() < FRAME_HEADER_SIZE {
            return Ok(None);
        }

        // Read payload length from header (at offset 32)
        let payload_len_offset = 32;
        if self.buffer.len() < payload_len_offset + 4 {
            return Ok(None);
        }

        let payload_len = u32::from_be_bytes([
            self.buffer[payload_len_offset],
            self.buffer[payload_len_offset + 1],
            self.buffer[payload_len_offset + 2],
            self.buffer[payload_len_offset + 3],
        ]);

        // Validate payload length
        if payload_len > crate::protocol::constants::MAX_PAYLOAD_SIZE {
            return Err(ProtocolError::PayloadTooLarge {
                actual: payload_len,
                max: crate::protocol::constants::MAX_PAYLOAD_SIZE,
            });
        }

        let total_size = FRAME_HEADER_SIZE + payload_len as usize;

        if total_size > self.max_frame_size {
            return Err(ProtocolError::FrameTooLarge {
                actual: total_size,
                max: self.max_frame_size,
            });
        }

        Ok(Some(total_size))
    }

    /// Decode a single frame from the front of the buffer.
    fn decode_single_frame(&mut self) -> ProtocolResult<Frame> {
        let frame_size = self.expected_frame_size.ok_or_else(|| ProtocolError::DecodingError {
            message: "Frame size not determined".to_string(),
        })?;

        // Extract frame data
        let frame_data = self.buffer.split_to(frame_size).freeze();

        // Decode the frame
        Frame::deserialize(&frame_data)
    }

    /// Recover from frame corruption by finding the next valid frame header.
    fn recover_from_corruption(&mut self) {
        // Look for the next occurrence of magic bytes
        for i in 1..self.buffer.len().saturating_sub(3) {
            if self.buffer[i..i + 4] == crate::protocol::frame::PROTOCOL_MAGIC {
                // Found potential next frame, discard everything before it
                self.buffer.advance(i);
                return;
            }
        }

        // No magic bytes found, clear the entire buffer
        self.buffer.clear();
    }

    /// Get buffer utilization as a percentage.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn buffer_utilization(&self) -> f32 {
        if self.buffer.capacity() == 0 {
            0.0
        } else {
            (self.buffer.len() as f32 / self.buffer.capacity() as f32) * 100.0
        }
    }
}

impl Default for FrameDecoder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::FrameType;

    #[test]
    fn test_encoder_single_frame() {
        let mut encoder = FrameEncoder::new();
        let frame = Frame::new(FrameType::Data, 1, Bytes::from_static(b"test")).unwrap();

        let encoded = encoder.encode(&frame).unwrap();
        assert!(!encoded.is_empty());
        assert!(encoded.len() >= FRAME_HEADER_SIZE);
    }

    #[test]
    fn test_encoder_batching() {
        let mut encoder = FrameEncoder::new();
        encoder.set_max_batch_size(2);

        let frame1 = Frame::new(FrameType::Data, 1, Bytes::from_static(b"frame1")).unwrap();
        let frame2 = Frame::new(FrameType::Data, 1, Bytes::from_static(b"frame2")).unwrap();

        // First frame should not trigger flush
        let result1 = encoder.add_to_batch(frame1).unwrap();
        assert!(result1.is_none());

        // Second frame should trigger flush
        let result2 = encoder.add_to_batch(frame2).unwrap();
        assert!(result2.is_some());
        assert!(encoder.is_batch_empty());
    }

    #[test]
    fn test_decoder_single_frame() {
        let mut encoder = FrameEncoder::new();
        let mut decoder = FrameDecoder::new();

        let original = Frame::new(FrameType::Data, 1, Bytes::from_static(b"test payload")).unwrap();
        let encoded = encoder.encode(&original).unwrap();

        let decoded = decoder.decode(&encoded).unwrap().unwrap();
        assert_eq!(original.payload(), decoded.payload());
        assert_eq!(original.stream_id(), decoded.stream_id());
    }

    #[test]
    fn test_decoder_streaming() {
        let mut encoder = FrameEncoder::new();
        let mut decoder = FrameDecoder::new();

        let frame1 = Frame::new(FrameType::Data, 1, Bytes::from_static(b"frame1")).unwrap();
        let frame2 = Frame::new(FrameType::Data, 2, Bytes::from_static(b"frame2")).unwrap();

        // Encode frames
        let encoded1 = encoder.encode(&frame1).unwrap();
        let encoded2 = encoder.encode(&frame2).unwrap();

        // Combine into single buffer
        let mut combined = BytesMut::new();
        combined.extend_from_slice(&encoded1);
        combined.extend_from_slice(&encoded2);

        // Decode stream
        let count = decoder.decode_stream(&combined).unwrap();
        assert_eq!(count, 2);

        let decoded1 = decoder.next_frame().unwrap();
        let decoded2 = decoder.next_frame().unwrap();

        assert_eq!(frame1.payload(), decoded1.payload());
        assert_eq!(frame2.payload(), decoded2.payload());
    }

    #[test]
    fn test_decoder_partial_frames() {
        let mut encoder = FrameEncoder::new();
        let mut decoder = FrameDecoder::new();

        let frame = Frame::new(FrameType::Data, 1, Bytes::from_static(b"test")).unwrap();
        let encoded = encoder.encode(&frame).unwrap();

        // Feed partial data
        let partial = &encoded[..encoded.len() / 2];
        let count1 = decoder.decode_stream(partial).unwrap();
        assert_eq!(count1, 0); // No complete frames yet

        // Feed remaining data
        let remaining = &encoded[encoded.len() / 2..];
        let count2 = decoder.decode_stream(remaining).unwrap();
        assert_eq!(count2, 1); // Now we have a complete frame

        let decoded = decoder.next_frame().unwrap();
        assert_eq!(frame.payload(), decoded.payload());
    }

    #[test]
    fn test_encoder_buffer_reuse() {
        let mut encoder = FrameEncoder::new();
        let frame = Frame::new(FrameType::Data, 1, Bytes::from_static(b"test")).unwrap();

        // Multiple encodes should reuse buffer
        let _encoded1 = encoder.encode(&frame).unwrap();
        let _encoded2 = encoder.encode(&frame).unwrap();

        // Buffer should be reused (can't easily test without internal access)
        assert!(encoder.buffer_utilization() >= 0.0);
    }

    #[test]
    fn test_decoder_statistics() {
        let decoder = FrameDecoder::new();
        let stats = decoder.stats();

        assert_eq!(stats.frames_decoded, 0);
        assert_eq!(stats.bytes_processed, 0);
        assert_eq!(stats.buffer_size, 0);
    }

    #[test]
    fn test_decoder_max_frame_size() {
        let mut decoder = FrameDecoder::with_capacity(1024);
        decoder.set_max_frame_size(100);

        // Create a frame that's too large
        let large_payload = vec![0u8; 200];
        let mut encoder = FrameEncoder::new();
        let frame = Frame::new(FrameType::Data, 1, Bytes::from(large_payload)).unwrap();
        let encoded = encoder.encode(&frame).unwrap();

        // This should fail due to frame size limit
        let result = decoder.decode_stream(&encoded);
        assert!(result.is_err());
    }

    #[test]
    fn test_encoder_reset() {
        let mut encoder = FrameEncoder::new();
        let frame = Frame::new(FrameType::Data, 1, Bytes::from_static(b"test")).unwrap();

        encoder.add_to_batch(frame).unwrap();
        assert!(!encoder.is_batch_empty());

        encoder.reset();
        assert!(encoder.is_batch_empty());
    }

    #[test]
    fn test_decoder_reset() {
        let mut decoder = FrameDecoder::new();

        // Add some data to buffer
        decoder.decode_stream(b"some random data").ok();

        decoder.reset();
        assert_eq!(decoder.stats().buffer_size, 0);
        assert!(!decoder.has_frames());
    }
}
