//! Binary message framing protocol for ultra-high-performance streaming.
//!
//! This module implements the wire protocol for `MemoryStreamer`, providing:
//! - Binary frame format with 32-byte header + variable payload
//! - Zero-copy encoding/decoding operations
//! - Target performance: <1μs encode/decode latency
//! - CRC32 integrity checking
//! - Stream multiplexing support
//!
//! ## Frame Structure
//!
//! The frame format uses a fixed 32-byte header followed by variable payload:
//!
//! ```text
//! +--------+--------+--------+--------+--------+--------+--------+--------+
//! | Magic  | Ver    | Flags           | Stream ID                       |
//! +--------+--------+--------+--------+--------+--------+--------+--------+
//! | Sequence Number                                                     |
//! +--------+--------+--------+--------+--------+--------+--------+--------+
//! | Timestamp (nanoseconds)                                             |
//! +--------+--------+--------+--------+--------+--------+--------+--------+
//! | Payload Length            | Checksum                              |
//! +--------+--------+--------+--------+--------+--------+--------+--------+
//! | Payload Data (variable length)                                     |
//! +--------+--------+--------+--------+--------+--------+--------+--------+
//! ```
//!
//! ## Usage
//!
//! ```rust
//! use kaelix_core::protocol::{Frame, FrameEncoder, FrameDecoder, FrameType};
//! use bytes::Bytes;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a data frame
//! let payload = Bytes::from_static(b"hello world");
//! let frame = Frame::new(FrameType::Data, 1, payload)?;
//!
//! // Encode to bytes
//! let mut encoder = FrameEncoder::new();
//! let encoded = encoder.encode(&frame)?;
//!
//! // Decode from bytes
//! let mut decoder = FrameDecoder::new();
//! let decoded = decoder.decode(&encoded)?.unwrap();
//! assert_eq!(frame.payload(), decoded.payload());
//! # Ok(())
//! # }
//! ```

pub mod codec;
pub mod error;
pub mod frame;

pub use codec::{FrameDecoder, FrameEncoder};
pub use error::{ProtocolError, ProtocolResult};
pub use frame::{FRAME_HEADER_SIZE, Frame, PROTOCOL_MAGIC, PROTOCOL_VERSION};

/// Protocol constants for frame handling.
pub mod constants {
    /// Maximum payload size in bytes (16MB).
    pub const MAX_PAYLOAD_SIZE: u32 = 16 * 1024 * 1024;

    /// Minimum frame size (header only).
    pub const MIN_FRAME_SIZE: usize = super::FRAME_HEADER_SIZE;

    /// Maximum frame size (header + max payload).
    pub const MAX_FRAME_SIZE: usize = super::FRAME_HEADER_SIZE + MAX_PAYLOAD_SIZE as usize;

    /// Frame alignment requirement for optimal performance.
    pub const FRAME_ALIGNMENT: usize = 8;
}

/// Frame type enumeration for different message categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum FrameType {
    /// Regular data frame containing user messages.
    Data = 0x0001,

    /// Control frame for protocol management.
    Control = 0x0002,

    /// Heartbeat frame for connection liveness.
    Heartbeat = 0x0003,

    /// Acknowledgment frame for reliable delivery.
    Ack = 0x0004,

    /// Error frame for reporting issues.
    Error = 0x0005,

    /// Close frame for graceful connection termination.
    Close = 0x0006,
}

impl FrameType {
    /// Convert from u16 value.
    ///
    /// # Errors
    /// Returns an error if the value doesn't correspond to a valid frame type.
    pub const fn from_u16(value: u16) -> ProtocolResult<Self> {
        match value {
            0x0001 => Ok(Self::Data),
            0x0002 => Ok(Self::Control),
            0x0003 => Ok(Self::Heartbeat),
            0x0004 => Ok(Self::Ack),
            0x0005 => Ok(Self::Error),
            0x0006 => Ok(Self::Close),
            _ => Err(ProtocolError::InvalidFrameType { frame_type: value }),
        }
    }

    /// Convert to u16 value.
    #[must_use]
    pub const fn as_u16(self) -> u16 {
        self as u16
    }

    /// Check if this frame type requires acknowledgment.
    #[must_use]
    pub const fn requires_ack(self) -> bool {
        matches!(self, Self::Data | Self::Control)
    }

    /// Check if this frame type is a control frame.
    #[must_use]
    pub const fn is_control(self) -> bool {
        matches!(self, Self::Control | Self::Heartbeat | Self::Ack | Self::Error | Self::Close)
    }
}

/// Frame flags for additional frame metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameFlags(u16);

impl FrameFlags {
    /// Create new frame flags with the given frame type.
    #[must_use]
    pub const fn new(frame_type: FrameType) -> Self {
        Self(frame_type.as_u16())
    }

    /// Get the frame type from flags.
    ///
    /// # Errors
    /// Returns an error if the frame type is invalid.
    pub const fn frame_type(self) -> ProtocolResult<FrameType> {
        FrameType::from_u16(self.0 & 0x00FF)
    }

    /// Set compression flag.
    #[must_use]
    pub const fn with_compression(mut self) -> Self {
        self.0 |= 0x0100;
        self
    }

    /// Check if compression is enabled.
    #[must_use]
    pub const fn is_compressed(self) -> bool {
        (self.0 & 0x0100) != 0
    }

    /// Set end-of-stream flag.
    #[must_use]
    pub const fn with_end_of_stream(mut self) -> Self {
        self.0 |= 0x0200;
        self
    }

    /// Check if this is end of stream.
    #[must_use]
    pub const fn is_end_of_stream(self) -> bool {
        (self.0 & 0x0200) != 0
    }

    /// Get raw flags value.
    #[must_use]
    pub const fn as_u16(self) -> u16 {
        self.0
    }

    /// Create from raw u16 value.
    #[must_use]
    pub const fn from_u16(value: u16) -> Self {
        Self(value)
    }
}

impl Default for FrameFlags {
    fn default() -> Self {
        Self::new(FrameType::Data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_type_conversion() {
        assert_eq!(FrameType::Data.as_u16(), 0x0001);
        assert_eq!(FrameType::from_u16(0x0001).unwrap(), FrameType::Data);
        assert!(FrameType::from_u16(0x9999).is_err());
    }

    #[test]
    fn test_frame_type_properties() {
        assert!(FrameType::Data.requires_ack());
        assert!(!FrameType::Heartbeat.requires_ack());
        assert!(FrameType::Control.is_control());
        assert!(!FrameType::Data.is_control());
    }

    #[test]
    fn test_frame_flags() {
        let flags = FrameFlags::new(FrameType::Data).with_compression().with_end_of_stream();

        assert_eq!(flags.frame_type().unwrap(), FrameType::Data);
        assert!(flags.is_compressed());
        assert!(flags.is_end_of_stream());
    }

    #[test]
    fn test_frame_flags_conversion() {
        let flags = FrameFlags::new(FrameType::Control);
        let raw = flags.as_u16();
        let reconstructed = FrameFlags::from_u16(raw);
        assert_eq!(flags, reconstructed);
    }
}
