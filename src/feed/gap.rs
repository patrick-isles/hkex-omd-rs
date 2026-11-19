//! Multicast sequence gap detection and out-of-order packet tracking.

/// Capacity of the out-of-order packet ring buffer.
pub const REORDER_RING_CAPACITY: usize = 64;

/// Result of evaluating an incoming packet's sequence number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceStatus {
    /// Packet matches the expected sequence number perfectly.
    InOrder,
    /// Sequence gap detected; packets are missing between `expected` and `received`.
    GapDetected {
        expected: u32,
        received: u32,
        missing: u32,
    },
    /// Stale or duplicate packet that has already been processed.
    DuplicateOrStale { expected: u32, received: u32 },
    /// Sequence number was deliberately reset by an exchange SequenceReset (MsgType 100).
    Reset { new_seq: u32 },
}

/// Zero-allocation tracker for multicast packet sequencing.
#[derive(Debug, Clone)]
pub struct SequenceTracker {
    expected_seq_num: u32,
    total_packets_processed: u64,
    total_gaps_detected: u64,
    total_duplicates_dropped: u64,
    initialized: bool,
}

impl SequenceTracker {
    /// Create a new uninitialized sequence tracker.
    ///
    /// The first received packet will synchronize the initial sequence number.
    pub const fn new() -> Self {
        Self {
            expected_seq_num: 0,
            total_packets_processed: 0,
            total_gaps_detected: 0,
            total_duplicates_dropped: 0,
            initialized: false,
        }
    }

    /// Create a sequence tracker with an explicitly expected starting sequence number.
    pub const fn with_expected(start_seq: u32) -> Self {
        Self {
            expected_seq_num: start_seq,
            total_packets_processed: 0,
            total_gaps_detected: 0,
            total_duplicates_dropped: 0,
            initialized: true,
        }
    }

    /// Get current expected sequence number.
    #[inline(always)]
    pub fn expected(&self) -> u32 {
        self.expected_seq_num
    }

    /// Reset sequence tracker to a new baseline (e.g. following MsgType 100).
    #[inline(always)]
    pub fn reset(&mut self, new_seq: u32) {
        self.expected_seq_num = new_seq;
        self.initialized = true;
    }

    /// Evaluate an incoming packet's sequence number and message count.
    ///
    /// Advances internal sequence counters if the packet is accepted.
    #[inline(always)]
    pub fn on_packet(&mut self, seq_num: u32, msg_count: u8) -> SequenceStatus {
        if !self.initialized {
            self.expected_seq_num = seq_num;
            self.initialized = true;
        }

        if seq_num == self.expected_seq_num {
            self.total_packets_processed += 1;
            // Advance expected sequence number by message count (heartbeats with 0 msgs don't advance)
            let advance = if msg_count == 0 { 0 } else { msg_count as u32 };
            self.expected_seq_num = self.expected_seq_num.wrapping_add(advance);
            SequenceStatus::InOrder
        } else if seq_num > self.expected_seq_num {
            self.total_gaps_detected += 1;
            let missing = seq_num - self.expected_seq_num;
            let expected = self.expected_seq_num;
            // Fast-forward expected to accommodate future packets
            let advance = if msg_count == 0 { 0 } else { msg_count as u32 };
            self.expected_seq_num = seq_num.wrapping_add(advance);
            self.total_packets_processed += 1;

            SequenceStatus::GapDetected {
                expected,
                received: seq_num,
                missing,
            }
        } else {
            self.total_duplicates_dropped += 1;
            SequenceStatus::DuplicateOrStale {
                expected: self.expected_seq_num,
                received: seq_num,
            }
        }
    }

    #[inline(always)]
    pub fn total_processed(&self) -> u64 {
        self.total_packets_processed
    }

    #[inline(always)]
    pub fn total_gaps(&self) -> u64 {
        self.total_gaps_detected
    }

    #[inline(always)]
    pub fn total_duplicates(&self) -> u64 {
        self.total_duplicates_dropped
    }
}

impl Default for SequenceTracker {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}
