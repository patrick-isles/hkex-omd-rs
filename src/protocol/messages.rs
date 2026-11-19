//! Strongly typed, zero-allocation decoders for core HKEX OMD-C messages.

use super::packet::OmdError;

/// OMD-C Message Type Constants.
pub const MSG_SECURITY_DEFINITION: u16 = 11;
pub const MSG_SECURITY_STATUS: u16 = 20;
pub const MSG_TRADE: u16 = 50;
pub const MSG_AGGREGATE_ORDER_BOOK_UPDATE: u16 = 53;
pub const MSG_SEQUENCE_RESET: u16 = 100;

/// Order book side (EntryType in HKEX specification).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Side {
    Bid = 0,
    Ask = 1,
}

impl Side {
    #[inline(always)]
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0 => Some(Self::Bid),
            1 => Some(Self::Ask),
            _ => None,
        }
    }
}

/// Action to perform on the aggregate order book.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum UpdateAction {
    /// Add a new price level to the book.
    New = 0,
    /// Delete an existing price level from the book.
    Delete = 1,
    /// Modify quantity and/or order count at an existing price level.
    Change = 2,
    /// Clear all orders on this side (or entire book).
    Clear = 3,
}

impl UpdateAction {
    #[inline(always)]
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0 => Some(Self::New),
            1 => Some(Self::Delete),
            2 => Some(Self::Change),
            3 => Some(Self::Clear),
            _ => None,
        }
    }
}

/// A zero-copy reference to an individual entry in an Aggregate Order Book Update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BookEntryRef {
    pub aggregate_quantity: u64,
    /// Price with 3 implied decimal places (e.g., 385000 = 385.000 HKD).
    pub price: i32,
    pub number_of_orders: u32,
    pub update_action: UpdateAction,
    pub side: Side,
}

impl BookEntryRef {
    pub const ENTRY_SIZE: usize = 18;

    /// Price in standard currency units (e.g., 385.00 HKD).
    #[inline(always)]
    pub fn price_f64(&self) -> f64 {
        self.price as f64 / 1000.0
    }
}

/// Zero-copy decoder for HKEX OMD-C MsgType 53 (Aggregate Order Book Update).
#[derive(Debug, Clone, Copy)]
pub struct AggregateOrderBookUpdate<'a> {
    pub security_code: u32,
    pub no_entries: u8,
    entries_payload: &'a [u8],
}

impl<'a> AggregateOrderBookUpdate<'a> {
    pub const HEADER_PAYLOAD_SIZE: usize = 8; // SecurityCode (4) + Filler (3) + NoEntries (1)

    #[inline(always)]
    pub fn parse(payload: &'a [u8]) -> Result<Self, OmdError> {
        if payload.len() < Self::HEADER_PAYLOAD_SIZE {
            return Err(OmdError::InvalidMessagePayload {
                msg_type: MSG_AGGREGATE_ORDER_BOOK_UPDATE,
            });
        }

        let security_code = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
        let no_entries = payload[7];

        let expected_payload_len =
            Self::HEADER_PAYLOAD_SIZE + (no_entries as usize * BookEntryRef::ENTRY_SIZE);
        if payload.len() < expected_payload_len {
            return Err(OmdError::InvalidMessagePayload {
                msg_type: MSG_AGGREGATE_ORDER_BOOK_UPDATE,
            });
        }

        Ok(Self {
            security_code,
            no_entries,
            entries_payload: &payload[Self::HEADER_PAYLOAD_SIZE..expected_payload_len],
        })
    }

    #[inline(always)]
    pub fn entries(&self) -> BookEntryIterator<'a> {
        BookEntryIterator {
            data: self.entries_payload,
            remaining: self.no_entries,
            offset: 0,
        }
    }
}

/// Zero-allocation iterator over repeating book entries in an AggregateOrderBookUpdate.
pub struct BookEntryIterator<'a> {
    data: &'a [u8],
    remaining: u8,
    offset: usize,
}

impl<'a> Iterator for BookEntryIterator<'a> {
    type Item = BookEntryRef;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 || self.offset + BookEntryRef::ENTRY_SIZE > self.data.len() {
            return None;
        }

        let chunk = &self.data[self.offset..self.offset + BookEntryRef::ENTRY_SIZE];
        let aggregate_quantity = u64::from_le_bytes([
            chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7],
        ]);
        let price = i32::from_le_bytes([chunk[8], chunk[9], chunk[10], chunk[11]]);
        let number_of_orders = u32::from_le_bytes([chunk[12], chunk[13], chunk[14], chunk[15]]);
        let update_action = UpdateAction::from_u8(chunk[16]).unwrap_or(UpdateAction::Change);
        let side = Side::from_u8(chunk[17]).unwrap_or(Side::Bid);

        self.offset += BookEntryRef::ENTRY_SIZE;
        self.remaining = self.remaining.saturating_sub(1);

        Some(BookEntryRef {
            aggregate_quantity,
            price,
            number_of_orders,
            update_action,
            side,
        })
    }
}

/// Zero-copy decoder for HKEX OMD-C MsgType 50 (Trade).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trade {
    pub security_code: u32,
    pub trade_id: u32,
    /// Trade price with 3 implied decimals.
    pub price: i32,
    pub quantity: u32,
    pub trd_type: i16,
    pub trade_time: u64,
}

impl Trade {
    pub const PAYLOAD_SIZE: usize = 26;

    #[inline(always)]
    pub fn parse(payload: &[u8]) -> Result<Self, OmdError> {
        if payload.len() < Self::PAYLOAD_SIZE {
            return Err(OmdError::InvalidMessagePayload {
                msg_type: MSG_TRADE,
            });
        }

        let security_code = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
        let trade_id = u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]);
        let price = i32::from_le_bytes([payload[8], payload[9], payload[10], payload[11]]);
        let quantity = u32::from_le_bytes([payload[12], payload[13], payload[14], payload[15]]);
        let trd_type = i16::from_le_bytes([payload[16], payload[17]]);
        let trade_time = u64::from_le_bytes([
            payload[18],
            payload[19],
            payload[20],
            payload[21],
            payload[22],
            payload[23],
            payload[24],
            payload[25],
        ]);

        Ok(Self {
            security_code,
            trade_id,
            price,
            quantity,
            trd_type,
            trade_time,
        })
    }

    #[inline(always)]
    pub fn price_f64(&self) -> f64 {
        self.price as f64 / 1000.0
    }
}

/// Zero-copy decoder for HKEX OMD-C MsgType 100 (Sequence Reset).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SequenceReset {
    pub new_seq_num: u32,
}

impl SequenceReset {
    pub const PAYLOAD_SIZE: usize = 4;

    #[inline(always)]
    pub fn parse(payload: &[u8]) -> Result<Self, OmdError> {
        if payload.len() < Self::PAYLOAD_SIZE {
            return Err(OmdError::InvalidMessagePayload {
                msg_type: MSG_SEQUENCE_RESET,
            });
        }
        let new_seq_num = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
        Ok(Self { new_seq_num })
    }
}
