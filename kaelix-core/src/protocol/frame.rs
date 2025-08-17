//! Frame structure and operations for the binary protocol.

use crate::protocol::{FrameFlags, FrameType, ProtocolError, ProtocolResult};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::time::{SystemTime, UNIX_EPOCH};

/// Protocol magic bytes: b"KAEX"
pub const PROTOCOL_MAGIC: [u8; 4] = [0x4B, 0x41, 0x45, 0x58];

/// Current protocol version
pub const PROTOCOL_VERSION: u8 = 0x01;

/// Frame header size in bytes
pub const FRAME_HEADER_SIZE: usize = 40;

/// Binary message frame with 40-byte header and variable payload.
///
/// Frame layout:
/// ```text
/// Offset | Size | Field
/// -------|------|-------------
///   0    |  4   | Magic bytes (b"KAEX")
///   4    |  1   | Protocol version
///   5    |  2   | Frame flags (includes frame type)
///   7    |  1   | Reserved (padding)
///   8    |  4   | Stream ID
///  12    |  4   | Reserved (padding)
///  16    |  8   | Sequence number
///  24    |  8   | Timestamp (nanoseconds since epoch)
///  32    |  4   | Payload length
///  36    |  4   | CRC32 checksum
///  40    |  ?   | Payload data
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Frame flags containing type and metadata
    flags: FrameFlags,

    /// Stream multiplexing identifier
    stream_id: u32,

    /// Message sequence number for ordering
    sequence: u64,

    /// Frame timestamp in nanoseconds since Unix epoch
    timestamp: u64,

    /// Frame payload (zero-copy)
    payload: Bytes,
}

impl Frame {
    /// Create a new frame with the specified type, stream ID, and payload.
    ///
    /// # Errors
    /// Returns an error if the payload exceeds maximum size.
    pub fn new(frame_type: FrameType, stream_id: u32, payload: Bytes) -> ProtocolResult<Self> {
        Self::with_sequence(frame_type, stream_id, 0, payload)
    }

    /// Create a new frame with explicit sequence number.
    ///
    /// # Errors
    /// Returns an error if the payload exceeds maximum size or stream ID is invalid.
    pub fn with_sequence(
        frame_type: FrameType,
        stream_id: u32,
        sequence: u64,
        payload: Bytes,
    ) -> ProtocolResult<Self> {
        // Validate payload size
        if payload.len() > crate::protocol::constants::MAX_PAYLOAD_SIZE as usize {
            return Err(ProtocolError::PayloadTooLarge {
                actual: u32::try_from(payload.len()).unwrap_or(u32::MAX),
                max: crate::protocol::constants::MAX_PAYLOAD_SIZE,
            });
        }

        // Validate stream ID (0 is reserved for control)
        if stream_id == 0 && !frame_type.is_control() {
            return Err(ProtocolError::InvalidStreamId {
                stream_id,
                reserved_start: 0,
                reserved_end: 0,
            });
        }

        // Get current timestamp with safe u128 to u64 conversion
        let timestamp = Self::current_timestamp_nanos()?;

        Ok(Self { flags: FrameFlags::new(frame_type), stream_id, sequence, timestamp, payload })
    }

    /// Create a heartbeat frame.
    #[must_use]
    pub fn heartbeat() -> Self {
        Self {
            flags: FrameFlags::new(FrameType::Heartbeat),
            stream_id: 0,
            sequence: 0,
            timestamp: Self::current_timestamp_nanos().unwrap_or(0),
            payload: Bytes::new(),
        }
    }

    /// Create an acknowledgment frame for the given sequence number.
    #[must_use]
    pub fn ack(stream_id: u32, sequence: u64) -> Self {
        Self {
            flags: FrameFlags::new(FrameType::Ack),
            stream_id,
            sequence,
            timestamp: Self::current_timestamp_nanos().unwrap_or(0),
            payload: Bytes::new(),
        }
    }

    /// Create an error frame with error message payload.
    ///
    /// # Errors
    /// Returns an error if the error message exceeds maximum payload size.
    pub fn error(stream_id: u32, error_message: &str) -> ProtocolResult<Self> {
        let payload = Bytes::copy_from_slice(error_message.as_bytes());
        Self::new(FrameType::Error, stream_id, payload)
    }

    /// Get current timestamp in nanoseconds with safe conversion.
    ///
    /// # Errors
    /// Returns an error if system time is before Unix epoch or timestamp overflows u64.
    fn current_timestamp_nanos() -> ProtocolResult<u64> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| ProtocolError::encoding(format!("Invalid system time: {e}")))?
            .as_nanos();

        // Convert u128 to u64 with overflow check
        u64::try_from(nanos).map_err(|_| {
            ProtocolError::encoding("Timestamp overflow: system time too far in future")
        })
    }

    /// Get the frame type.
    ///
    /// # Errors
    /// Returns an error if the frame flags contain an invalid frame type.
    pub const fn frame_type(&self) -> ProtocolResult<FrameType> {
        self.flags.frame_type()
    }

    /// Get the frame flags.
    #[must_use]
    pub const fn flags(&self) -> FrameFlags {
        self.flags
    }

    /// Set frame flags.
    pub const fn set_flags(&mut self, flags: FrameFlags) {
        self.flags = flags;
    }

    /// Get the stream ID.
    #[must_use]
    pub const fn stream_id(&self) -> u32 {
        self.stream_id
    }

    /// Get the sequence number.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Set the sequence number.
    pub const fn set_sequence(&mut self, sequence: u64) {
        self.sequence = sequence;
    }

    /// Get the timestamp.
    #[must_use]
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }

    /// Get the payload.
    #[must_use]
    pub const fn payload(&self) -> &Bytes {
        &self.payload
    }

    /// Get the payload size.
    #[must_use]
    pub const fn payload_size(&self) -> u32 {
        #[allow(clippy::cast_possible_truncation)]
        {
            self.payload.len() as u32
        }
    }

    /// Get the total frame size (header + payload).
    #[must_use]
    pub const fn frame_size(&self) -> usize {
        FRAME_HEADER_SIZE + self.payload.len()
    }

    /// Take ownership of the payload.
    #[must_use]
    pub fn into_payload(self) -> Bytes {
        self.payload
    }

    /// Serialize the frame to bytes with checksum.
    ///
    /// # Errors
    /// Returns an error if serialization fails or buffer allocation fails.
    pub fn serialize(&self) -> ProtocolResult<Bytes> {
        let total_size = self.frame_size();
        let mut buf = BytesMut::with_capacity(total_size);

        // Write header (40 bytes total)
        buf.put_slice(&PROTOCOL_MAGIC); // 0-3: Magic bytes (4 bytes)
        buf.put_u8(PROTOCOL_VERSION); // 4: Version (1 byte)
        buf.put_u16(self.flags.as_u16()); // 5-6: Flags (2 bytes)
        buf.put_u8(0); // 7: Reserved (1 byte)
        buf.put_u32(self.stream_id); // 8-11: Stream ID (4 bytes)
        buf.put_u32(0); // 12-15: Reserved (4 bytes)
        buf.put_u64(self.sequence); // 16-23: Sequence (8 bytes)
        buf.put_u64(self.timestamp); // 24-31: Timestamp (8 bytes)
        buf.put_u32(self.payload_size()); // 32-35: Payload length (4 bytes)

        // Calculate checksum over header + payload (before adding checksum)
        let checksum = Self::calculate_checksum(&buf[..], &self.payload);
        buf.put_u32(checksum); // 36-39: Checksum (4 bytes)

        // Write payload (variable length)
        buf.put_slice(&self.payload);

        Ok(buf.freeze())
    }

    /// Deserialize a frame from bytes with checksum verification.
    ///
    /// # Errors
    /// Returns an error if the buffer is too small, magic bytes are invalid,
    /// version is unsupported, or checksum verification fails.
    pub fn deserialize(data: &Bytes) -> ProtocolResult<Self> {
        // Check minimum size
        if data.len() < FRAME_HEADER_SIZE {
            return Err(ProtocolError::BufferTooSmall {
                required: FRAME_HEADER_SIZE,
                actual: data.len(),
            });
        }

        let mut buf = data.clone();

        // Read header fields in order
        let mut magic = [0u8; 4];
        buf.copy_to_slice(&mut magic);
        if magic != PROTOCOL_MAGIC {
            return Err(ProtocolError::InvalidMagic { expected: PROTOCOL_MAGIC, actual: magic });
        }

        let version = buf.get_u8();
        if version != PROTOCOL_VERSION {
            return Err(ProtocolError::UnsupportedVersion { version, supported: PROTOCOL_VERSION });
        }

        let flags = FrameFlags::from_u16(buf.get_u16());
        let _reserved1 = buf.get_u8(); // Skip reserved byte

        let stream_id = buf.get_u32();
        let _reserved2 = buf.get_u32(); // Skip reserved bytes

        let sequence = buf.get_u64();
        let timestamp = buf.get_u64();
        let payload_len = buf.get_u32();

        // Validate payload length
        if payload_len > crate::protocol::constants::MAX_PAYLOAD_SIZE {
            return Err(ProtocolError::PayloadTooLarge {
                actual: payload_len,
                max: crate::protocol::constants::MAX_PAYLOAD_SIZE,
            });
        }

        let expected_checksum = buf.get_u32();

        // Check if we have enough bytes for payload
        if buf.len() < payload_len as usize {
            return Err(ProtocolError::IncompleteFrame {
                expected: FRAME_HEADER_SIZE + payload_len as usize,
                actual: FRAME_HEADER_SIZE + buf.len(),
            });
        }

        // Extract payload
        let payload = if payload_len == 0 {
            Bytes::new()
        } else {
            buf.slice(..payload_len as usize)
        };

        // Verify checksum (recalculate over header without checksum + payload)
        let header_without_checksum = &data[..36]; // First 36 bytes (excluding checksum)
        let actual_checksum = Self::calculate_checksum(header_without_checksum, &payload);

        if expected_checksum != actual_checksum {
            return Err(ProtocolError::ChecksumMismatch {
                expected: expected_checksum,
                actual: actual_checksum,
            });
        }

        // Validate frame type
        let _frame_type = flags.frame_type()?;

        Ok(Self { flags, stream_id, sequence, timestamp, payload })
    }

    /// Calculate CRC32 checksum over header and payload.
    fn calculate_checksum(header: &[u8], payload: &Bytes) -> u32 {
        use std::hash::{Hash, Hasher};

        // Use a simple hash for now - in production would use CRC32
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        header.hash(&mut hasher);
        payload.as_ref().hash(&mut hasher);

        #[allow(clippy::cast_possible_truncation)]
        {
            hasher.finish() as u32
        }
    }

    /// Check if this frame requires an acknowledgment.
    #[must_use]
    pub fn requires_ack(&self) -> bool {
        self.frame_type().is_ok_and(FrameType::requires_ack)
    }

    /// Check if this is a control frame.
    #[must_use]
    pub fn is_control(&self) -> bool {
        self.frame_type().is_ok_and(FrameType::is_control)
    }
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            flags: FrameFlags::default(),
            stream_id: 1,
            sequence: 0,
            timestamp: 0,
            payload: Bytes::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_creation() {
        let payload = Bytes::from_static(b"hello world");
        let frame = Frame::new(FrameType::Data, 1, payload.clone()).unwrap();

        assert_eq!(frame.frame_type().unwrap(), FrameType::Data);
        assert_eq!(frame.stream_id(), 1);
        assert_eq!(frame.payload(), &payload);
        assert_eq!(frame.payload_size(), 11);
        assert_eq!(frame.frame_size(), FRAME_HEADER_SIZE + 11);
    }

    #[test]
    fn test_frame_serialization_roundtrip() {
        let payload = Bytes::from_static(b"test payload");
        let original = Frame::new(FrameType::Data, 42, payload).unwrap();

        let serialized = original.serialize().unwrap();
        let deserialized = Frame::deserialize(&serialized).unwrap();

        assert_eq!(original.frame_type().unwrap(), deserialized.frame_type().unwrap());
        assert_eq!(original.stream_id(), deserialized.stream_id());
        assert_eq!(original.sequence(), deserialized.sequence());
        assert_eq!(original.payload(), deserialized.payload());
    }

    #[test]
    fn test_heartbeat_frame() {
        let frame = Frame::heartbeat();
        assert_eq!(frame.frame_type().unwrap(), FrameType::Heartbeat);
        assert_eq!(frame.stream_id(), 0);
        assert!(frame.payload().is_empty());
        assert!(frame.is_control());
        assert!(!frame.requires_ack());
    }

    #[test]
    fn test_ack_frame() {
        let frame = Frame::ack(1, 42);
        assert_eq!(frame.frame_type().unwrap(), FrameType::Ack);
        assert_eq!(frame.stream_id(), 1);
        assert_eq!(frame.sequence(), 42);
        assert!(frame.is_control());
    }

    #[test]
    fn test_error_frame() {
        let frame = Frame::error(1, "Something went wrong").unwrap();
        assert_eq!(frame.frame_type().unwrap(), FrameType::Error);
        assert_eq!(frame.stream_id(), 1);
        assert_eq!(frame.payload(), &Bytes::from_static(b"Something went wrong"));
    }

    #[test]
    fn test_invalid_payload_size() {
        let large_payload =
            Bytes::from(vec![0u8; (crate::protocol::constants::MAX_PAYLOAD_SIZE + 1) as usize]);
        let result = Frame::new(FrameType::Data, 1, large_payload);
        assert!(matches!(result, Err(ProtocolError::PayloadTooLarge { .. })));
    }

    #[test]
    fn test_invalid_stream_id() {
        let payload = Bytes::from_static(b"test");
        let result = Frame::new(FrameType::Data, 0, payload); // Stream 0 reserved for control
        assert!(matches!(result, Err(ProtocolError::InvalidStreamId { .. })));
    }

    #[test]
    fn test_frame_properties() {
        let frame = Frame::new(FrameType::Data, 1, Bytes::from_static(b"test")).unwrap();
        assert!(frame.requires_ack());
        assert!(!frame.is_control());

        let control_frame = Frame::heartbeat();
        assert!(!control_frame.requires_ack());
        assert!(control_frame.is_control());
    }

    #[test]
    fn test_buffer_too_small() {
        let small_buf = Bytes::from_static(&[0u8; 10]);
        let result = Frame::deserialize(&small_buf);
        assert!(matches!(result, Err(ProtocolError::BufferTooSmall { .. })));
    }

    #[test]
    fn test_invalid_magic() {
        let mut buf = BytesMut::with_capacity(FRAME_HEADER_SIZE);
        buf.put_slice(b"WXYZ"); // Wrong magic
        buf.put_u8(PROTOCOL_VERSION);
        buf.resize(FRAME_HEADER_SIZE, 0);

        let result = Frame::deserialize(&buf.freeze());
        assert!(matches!(result, Err(ProtocolError::InvalidMagic { .. })));
    }

    #[test]
    fn test_unsupported_version() {
        let mut buf = BytesMut::with_capacity(FRAME_HEADER_SIZE);
        buf.put_slice(&PROTOCOL_MAGIC);
        buf.put_u8(99); // Unsupported version
        buf.resize(FRAME_HEADER_SIZE, 0);

        let result = Frame::deserialize(&buf.freeze());
        assert!(matches!(result, Err(ProtocolError::UnsupportedVersion { .. })));
    }

    #[test]
    fn test_timestamp_safety() {
        // Test that timestamp creation is safe and handles errors
        let frame = Frame::new(FrameType::Data, 1, Bytes::from_static(b"test")).unwrap();
        assert!(frame.timestamp() > 0); // Should have a valid timestamp
    }

    #[test]
    fn test_frame_header_size() {
        // Verify the header size is correct
        assert_eq!(FRAME_HEADER_SIZE, 40);
    }

    #[test]
    fn test_empty_payload() {
        let frame = Frame::new(FrameType::Data, 1, Bytes::new()).unwrap();
        let serialized = frame.serialize().unwrap();

        // Should be header size only
        assert_eq!(serialized.len(), FRAME_HEADER_SIZE);

        let deserialized = Frame::deserialize(&serialized).unwrap();
        assert!(deserialized.payload().is_empty());
        assert_eq!(deserialized.payload_size(), 0);
    }
}
