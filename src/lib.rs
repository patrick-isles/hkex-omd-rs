//! # hkex-omd-rs
//!
//! A high-performance, zero-allocation, sub-microsecond HKEX Orion Market Data
//! (OMD-C) multicast binary feed handler and L3/MBP (Market by Price) order book engine.
//!
//! ## Key Capabilities
//! - **Zero-Copy Binary Parsing:** Parses HKEX OMD-C UDP packet headers and message payloads directly off network buffers without heap allocation.
//! - **Cache-Localized 10-Level Order Book:** Keeps full top-10 Market By Price depth inside L1 cache lines with sub-15ns in-place updates.
//! - **Quantitative Telemetry:** Computes BBO, Spread, Microprice, Depth / Order Flow Imbalance (OFI), and simulated market sweep VWAP.
//! - **Multicast Gap Detection:** Fast sequence gap identification with dropped packet tracking.

pub mod book;
pub mod feed;
pub mod protocol;

pub use book::{MarketByPriceBook, PriceLevel, MAX_DEPTH};
pub use feed::{FeedMetrics, OmdFeedHandler, ProcessReport, SequenceStatus, SequenceTracker};
pub use protocol::{
    AggregateOrderBookUpdate, BookEntryIterator, BookEntryRef, MessageIterator, OmdError,
    PacketHeader, RawMessageRef, SequenceReset, Side, Trade, UpdateAction,
    MSG_AGGREGATE_ORDER_BOOK_UPDATE, MSG_SECURITY_DEFINITION, MSG_SECURITY_STATUS,
    MSG_SEQUENCE_RESET, MSG_TRADE,
};
