//! Zero-copy parsing for HKEX OMD-C UDP packet headers and message slicing.

use core::fmt;

/// Length of the standard HKEX OMD-C binary packet header in bytes.
pub const PACKET_HEADER_SIZE: usize = 16;

/// Standard HKEX OMD-C message header size (MsgSize + MsgType).
pub const MESSAGE_HEADER_SIZE: usize = 4;

/// Errors that can occur during packet or message parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OmdError {
    /// Buffer is smaller than the required 16-byte packet header.
    PacketTooShort { actual: usize, required: usize },
    /// Packet size field in header does not match slice length.
    PacketSizeMismatch {
        header_size: u16,
        actual_size: usize,
    },
    /// Message length exceeds remaining packet payload.
    MessageOutOfBounds {
        offset: usize,
        msg_size: u16,
        total_len: usize,
    },
    /// Message size is smaller than the minimum 4-byte message header.
    MessageTooShort { msg_size: u16 },
    /// Unknown or unsupported message structure.
    InvalidMessagePayload { msg_type: u16 },
}

impl fmt::Display for OmdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PacketTooShort { actual, required } => {
                write!(
                    f,
                    "Packet too short: got {} bytes, required {}",
                    actual, required
                )
            }
            Self::PacketSizeMismatch {
                header_size,
                actual_size,
            } => {
                write!(
                    f,
                    "Packet size mismatch: header says {}, actual is {}",
                    header_size, actual_size
                )
            }
            Self::MessageOutOfBounds {
                offset,
                msg_size,
                total_len,
            } => {
                write!(
                    f,
                    "Message out of bounds: offset {} + size {} > total {}",
                    offset, msg_size, total_len
                )
            }
            Self::MessageTooShort { msg_size } => {
                write!(f, "Message too short: {} bytes < 4-byte header", msg_size)
            }
            Self::InvalidMessagePayload { msg_type } => {
                write!(f, "Invalid payload for MsgType {}", msg_type)
            }
        }
    }
}

impl std::error::Error for OmdError {}

/// Standard 16-byte binary packet header broadcast over HKEX OMD-C multicast.
///
/// Layout (Little-Endian):
/// - Offset 0: `pkt_size` (u16, 2 bytes)
/// - Offset 2: `msg_count` (u8, 1 byte)
/// - Offset 3: `filler` (u8, 1 byte)
/// - Offset 4: `seq_num` (u32, 4 bytes)
/// - Offset 8: `send_time` (u64, 8 bytes - nanoseconds since UNIX epoch)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct PacketHeader {
    pub pkt_size: u16,
    pub msg_count: u8,
    pub filler: u8,
    pub seq_num: u32,
    pub send_time: u64,
}

impl PacketHeader {
    /// Parse packet header with zero allocations directly from byte slice.
    #[inline(always)]
    pub fn parse(buf: &[u8]) -> Result<Self, OmdError> {
        if buf.len() < PACKET_HEADER_SIZE {
            return Err(OmdError::PacketTooShort {
                actual: buf.len(),
                required: PACKET_HEADER_SIZE,
            });
        }

        let pkt_size = u16::from_le_bytes([buf[0], buf[1]]);
        let msg_count = buf[2];
        let filler = buf[3];
        let seq_num = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
        let send_time = u64::from_le_bytes([
            buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
        ]);

        Ok(Self {
            pkt_size,
            msg_count,
            filler,
            seq_num,
            send_time,
        })
    }
}

/// A zero-copy reference to an unparsed raw message within an OMD packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawMessageRef<'a> {
    pub msg_size: u16,
    pub msg_type: u16,
    pub payload: &'a [u8],
}

/// Zero-copy, zero-allocation iterator over messages inside a verified OMD-C packet.
pub struct MessageIterator<'a> {
    data: &'a [u8],
    offset: usize,
    remaining_msgs: u8,
}

impl<'a> MessageIterator<'a> {
    #[inline(always)]
    pub fn new(packet_data: &'a [u8], msg_count: u8) -> Self {
        Self {
            data: packet_data,
            offset: PACKET_HEADER_SIZE,
            remaining_msgs: msg_count,
        }
    }
}

impl<'a> Iterator for MessageIterator<'a> {
    type Item = Result<RawMessageRef<'a>, OmdError>;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_msgs == 0 || self.offset >= self.data.len() {
            return None;
        }

        if self.offset + MESSAGE_HEADER_SIZE > self.data.len() {
            return Some(Err(OmdError::MessageOutOfBounds {
                offset: self.offset,
                msg_size: MESSAGE_HEADER_SIZE as u16,
                total_len: self.data.len(),
            }));
        }

        let msg_size = u16::from_le_bytes([self.data[self.offset], self.data[self.offset + 1]]);
        let msg_type = u16::from_le_bytes([self.data[self.offset + 2], self.data[self.offset + 3]]);

        if msg_size < MESSAGE_HEADER_SIZE as u16 {
            return Some(Err(OmdError::MessageTooShort { msg_size }));
        }

        let end = self.offset + msg_size as usize;
        if end > self.data.len() {
            return Some(Err(OmdError::MessageOutOfBounds {
                offset: self.offset,
                msg_size,
                total_len: self.data.len(),
            }));
        }

        let payload = &self.data[self.offset + MESSAGE_HEADER_SIZE..end];
        self.offset = end;
        self.remaining_msgs = self.remaining_msgs.saturating_sub(1);

        Some(Ok(RawMessageRef {
            msg_size,
            msg_type,
            payload,
        }))
    }
}
