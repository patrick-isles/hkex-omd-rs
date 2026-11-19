//! HKEX OMD-C binary protocol definitions and zero-copy packet parsers.

pub mod messages;
pub mod packet;

pub use messages::{
    AggregateOrderBookUpdate, BookEntryIterator, BookEntryRef, SequenceReset, Side, Trade,
    UpdateAction, MSG_AGGREGATE_ORDER_BOOK_UPDATE, MSG_SECURITY_DEFINITION, MSG_SECURITY_STATUS,
    MSG_SEQUENCE_RESET, MSG_TRADE,
};
pub use packet::{
    MessageIterator, OmdError, PacketHeader, RawMessageRef, MESSAGE_HEADER_SIZE, PACKET_HEADER_SIZE,
};
