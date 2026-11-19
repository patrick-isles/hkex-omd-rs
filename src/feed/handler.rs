//! Multicast feed handler orchestrating packet ingestion, gap tracking, and book dispatch.

use std::collections::HashMap;

use super::gap::{SequenceStatus, SequenceTracker};
use crate::book::order_book::MarketByPriceBook;
use crate::protocol::messages::{
    AggregateOrderBookUpdate, SequenceReset, Trade, MSG_AGGREGATE_ORDER_BOOK_UPDATE,
    MSG_SEQUENCE_RESET, MSG_TRADE,
};
use crate::protocol::packet::{MessageIterator, OmdError, PacketHeader};

/// Summary report emitted after processing a raw network packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessReport {
    pub seq_num: u32,
    pub msg_count: u8,
    pub seq_status: SequenceStatus,
    pub book_updates: u16,
    pub trades: u16,
    pub is_heartbeat: bool,
}

/// Statistics and telemetry for the feed handler.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FeedMetrics {
    pub packets_received: u64,
    pub messages_parsed: u64,
    pub book_entries_applied: u64,
    pub trades_recorded: u64,
    pub sequence_gaps: u64,
    pub duplicates_dropped: u64,
}

/// High-throughput HKEX OMD-C Feed Handler.
pub struct OmdFeedHandler {
    tracker: SequenceTracker,
    books: HashMap<u32, MarketByPriceBook>,
    metrics: FeedMetrics,
}

impl OmdFeedHandler {
    /// Create a new feed handler.
    pub fn new() -> Self {
        Self {
            tracker: SequenceTracker::new(),
            books: HashMap::with_capacity(256),
            metrics: FeedMetrics::default(),
        }
    }

    /// Pre-register a security in the order book map.
    pub fn register_security(&mut self, security_code: u32) {
        self.books
            .entry(security_code)
            .or_insert_with(|| MarketByPriceBook::new(security_code));
    }

    /// Get a reference to a security's Limit Order Book if tracked.
    #[inline(always)]
    pub fn get_book(&self, security_code: u32) -> Option<&MarketByPriceBook> {
        self.books.get(&security_code)
    }

    /// Get a mutable reference to a security's Limit Order Book.
    #[inline(always)]
    pub fn get_book_mut(&mut self, security_code: u32) -> Option<&mut MarketByPriceBook> {
        self.books.get_mut(&security_code)
    }

    /// Total number of active securities currently in the book table.
    #[inline(always)]
    pub fn tracked_securities_count(&self) -> usize {
        self.books.len()
    }

    /// Read-only telemetry metrics.
    #[inline(always)]
    pub fn metrics(&self) -> &FeedMetrics {
        &self.metrics
    }

    /// Process an incoming raw byte slice from a UDP socket or pcap capture.
    ///
    /// Executes completely zero-allocation in the hot path.
    pub fn process_packet(&mut self, packet_bytes: &[u8]) -> Result<ProcessReport, OmdError> {
        let header = PacketHeader::parse(packet_bytes)?;
        self.metrics.packets_received += 1;

        let seq_status = self.tracker.on_packet(header.seq_num, header.msg_count);
        match seq_status {
            SequenceStatus::DuplicateOrStale { .. } => {
                self.metrics.duplicates_dropped += 1;
                return Ok(ProcessReport {
                    seq_num: header.seq_num,
                    msg_count: header.msg_count,
                    seq_status,
                    book_updates: 0,
                    trades: 0,
                    is_heartbeat: header.msg_count == 0,
                });
            }
            SequenceStatus::GapDetected { .. } => {
                self.metrics.sequence_gaps += 1;
            }
            _ => {}
        }

        if header.msg_count == 0 {
            // Heartbeat packet
            return Ok(ProcessReport {
                seq_num: header.seq_num,
                msg_count: 0,
                seq_status,
                book_updates: 0,
                trades: 0,
                is_heartbeat: true,
            });
        }

        let mut book_updates = 0;
        let mut trades = 0;

        let iter = MessageIterator::new(packet_bytes, header.msg_count);
        for msg_res in iter {
            let msg = msg_res?;
            self.metrics.messages_parsed += 1;

            match msg.msg_type {
                MSG_AGGREGATE_ORDER_BOOK_UPDATE => {
                    let update = AggregateOrderBookUpdate::parse(msg.payload)?;
                    let book = self
                        .books
                        .entry(update.security_code)
                        .or_insert_with(|| MarketByPriceBook::new(update.security_code));

                    book.last_seq_num = header.seq_num;
                    book.last_update_time = header.send_time;

                    for entry in update.entries() {
                        book.apply_entry(&entry);
                        self.metrics.book_entries_applied += 1;
                    }
                    book_updates += 1;
                }
                MSG_TRADE => {
                    let _trade = Trade::parse(msg.payload)?;
                    self.metrics.trades_recorded += 1;
                    trades += 1;
                }
                MSG_SEQUENCE_RESET => {
                    let reset = SequenceReset::parse(msg.payload)?;
                    self.tracker.reset(reset.new_seq_num);
                }
                _ => {
                    // Other message types (SecurityStatus, Statistics, etc.) safely bypassed
                }
            }
        }

        Ok(ProcessReport {
            seq_num: header.seq_num,
            msg_count: header.msg_count,
            seq_status,
            book_updates,
            trades,
            is_heartbeat: false,
        })
    }
}

impl Default for OmdFeedHandler {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}
