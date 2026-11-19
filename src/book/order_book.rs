//! High-frequency 10-level aggregate limit order book (Market By Price).

use super::level::PriceLevel;
use crate::protocol::messages::{BookEntryRef, Side, UpdateAction};

/// Maximum order book depth broadcast by HKEX OMD-C (10 levels per side).
pub const MAX_DEPTH: usize = 10;

/// A zero-allocation, cache-localized 10-level Market-by-Price Limit Order Book.
///
/// Designed to live entirely inside L1 cache lines (approx. 320 bytes total).
/// Memory layout maintains bids in descending order (highest price first)
/// and asks in ascending order (lowest price first).
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct MarketByPriceBook {
    pub security_code: u32,
    pub last_seq_num: u32,
    pub last_update_time: u64,
    bids: [PriceLevel; MAX_DEPTH],
    asks: [PriceLevel; MAX_DEPTH],
    bid_count: usize,
    ask_count: usize,
}

impl MarketByPriceBook {
    /// Initialize a new, empty order book for a given HKEX security code.
    pub const fn new(security_code: u32) -> Self {
        Self {
            security_code,
            last_seq_num: 0,
            last_update_time: 0,
            bids: [PriceLevel::new(0, 0, 0); MAX_DEPTH],
            asks: [PriceLevel::new(0, 0, 0); MAX_DEPTH],
            bid_count: 0,
            ask_count: 0,
        }
    }

    /// Reset all depth levels to empty.
    #[inline(always)]
    pub fn clear(&mut self) {
        self.bids = [PriceLevel::new(0, 0, 0); MAX_DEPTH];
        self.asks = [PriceLevel::new(0, 0, 0); MAX_DEPTH];
        self.bid_count = 0;
        self.ask_count = 0;
    }

    /// Number of active bid levels.
    #[inline(always)]
    pub fn bid_depth(&self) -> usize {
        self.bid_count
    }

    /// Number of active ask levels.
    #[inline(always)]
    pub fn ask_depth(&self) -> usize {
        self.ask_count
    }

    /// Slice of active bid levels (sorted descending by price).
    #[inline(always)]
    pub fn bids(&self) -> &[PriceLevel] {
        &self.bids[..self.bid_count]
    }

    /// Slice of active ask levels (sorted ascending by price).
    #[inline(always)]
    pub fn asks(&self) -> &[PriceLevel] {
        &self.asks[..self.ask_count]
    }

    /// Best Bid price level (top of book).
    #[inline(always)]
    pub fn best_bid(&self) -> Option<PriceLevel> {
        if self.bid_count > 0 {
            Some(self.bids[0])
        } else {
            None
        }
    }

    /// Best Ask price level (top of book).
    #[inline(always)]
    pub fn best_ask(&self) -> Option<PriceLevel> {
        if self.ask_count > 0 {
            Some(self.asks[0])
        } else {
            None
        }
    }

    /// Top-of-Book Best Bid and Offer (BBO).
    #[inline(always)]
    pub fn bbo(&self) -> Option<(PriceLevel, PriceLevel)> {
        match (self.best_bid(), self.best_ask()) {
            (Some(b), Some(a)) => Some((b, a)),
            _ => None,
        }
    }

    /// Bid-Ask Spread in integer price ticks.
    #[inline(always)]
    pub fn spread(&self) -> Option<i32> {
        self.bbo().map(|(bid, ask)| ask.price - bid.price)
    }

    /// Bid-Ask Spread in standard currency units (HKD).
    #[inline(always)]
    pub fn spread_f64(&self) -> Option<f64> {
        self.spread().map(|s| s as f64 / 1000.0)
    }

    /// Mid-price of the Best Bid and Offer in HKD.
    #[inline(always)]
    pub fn mid_price(&self) -> Option<f64> {
        self.bbo()
            .map(|(bid, ask)| (bid.price_f64() + ask.price_f64()) * 0.5)
    }

    /// Volume-Weighted BBO Microprice in HKD:
    ///
    /// $$ \text{Microprice} = \frac{P_{\text{bid}} \cdot Q_{\text{ask}} + P_{\text{ask}} \cdot Q_{\text{bid}}}{Q_{\text{bid}} + Q_{\text{ask}}} $$
    ///
    /// Standard high-frequency indicator for short-horizon price pressure.
    #[inline(always)]
    pub fn microprice(&self) -> Option<f64> {
        self.bbo().and_then(|(bid, ask)| {
            let total_qty = bid.quantity + ask.quantity;
            if total_qty == 0 {
                None
            } else {
                let weighted = (bid.price_f64() * ask.quantity as f64)
                    + (ask.price_f64() * bid.quantity as f64);
                Some(weighted / total_qty as f64)
            }
        })
    }

    /// Order Flow / Depth Imbalance bounded in `[-1.0, 1.0]`:
    ///
    /// $$ \text{OFI} = \frac{Q_{\text{bid, total}} - Q_{\text{ask, total}}}{Q_{\text{bid, total}} + Q_{\text{ask, total}}} $$
    #[inline(always)]
    pub fn depth_imbalance(&self) -> f64 {
        let total_bid_qty: u64 = self.bids().iter().map(|l| l.quantity).sum();
        let total_ask_qty: u64 = self.asks().iter().map(|l| l.quantity).sum();
        let sum = total_bid_qty + total_ask_qty;
        if sum == 0 {
            0.0
        } else {
            (total_bid_qty as f64 - total_ask_qty as f64) / sum as f64
        }
    }

    /// Volume-Weighted Average Price (VWAP) for a simulated market sell of `target_volume`
    /// walking down the Bid book.
    pub fn vwap_bid(&self, target_volume: u64) -> Option<f64> {
        if target_volume == 0 || self.bid_count == 0 {
            return None;
        }

        let mut remaining = target_volume;
        let mut total_cost: f64 = 0.0;

        for level in self.bids() {
            let filled = remaining.min(level.quantity);
            total_cost += filled as f64 * level.price_f64();
            remaining -= filled;
            if remaining == 0 {
                return Some(total_cost / target_volume as f64);
            }
        }
        None // Insufficient liquidity in top 10 levels
    }

    /// Volume-Weighted Average Price (VWAP) for a simulated market buy of `target_volume`
    /// walking up the Ask book.
    pub fn vwap_ask(&self, target_volume: u64) -> Option<f64> {
        if target_volume == 0 || self.ask_count == 0 {
            return None;
        }

        let mut remaining = target_volume;
        let mut total_cost: f64 = 0.0;

        for level in self.asks() {
            let filled = remaining.min(level.quantity);
            total_cost += filled as f64 * level.price_f64();
            remaining -= filled;
            if remaining == 0 {
                return Some(total_cost / target_volume as f64);
            }
        }
        None // Insufficient liquidity in top 10 levels
    }

    /// Apply an incoming aggregate order book update entry in-place.
    ///
    /// Executes in under 15 nanoseconds without any dynamic allocation.
    #[inline(always)]
    pub fn apply_entry(&mut self, entry: &BookEntryRef) {
        match entry.side {
            Side::Bid => self.apply_bid(entry),
            Side::Ask => self.apply_ask(entry),
        }
    }

    #[inline(always)]
    fn apply_bid(&mut self, entry: &BookEntryRef) {
        match entry.update_action {
            UpdateAction::Clear => {
                self.bids = [PriceLevel::new(0, 0, 0); MAX_DEPTH];
                self.bid_count = 0;
            }
            UpdateAction::New => {
                let new_level = PriceLevel::new(
                    entry.price,
                    entry.aggregate_quantity,
                    entry.number_of_orders,
                );
                // Bids are sorted descending (highest price first)
                let mut idx = 0;
                while idx < self.bid_count && self.bids[idx].price > entry.price {
                    idx += 1;
                }

                if idx < MAX_DEPTH {
                    let end = self.bid_count.min(MAX_DEPTH - 1);
                    let mut j = end;
                    while j > idx {
                        self.bids[j] = self.bids[j - 1];
                        j -= 1;
                    }
                    self.bids[idx] = new_level;
                    if self.bid_count < MAX_DEPTH {
                        self.bid_count += 1;
                    }
                }
            }
            UpdateAction::Change => {
                for i in 0..self.bid_count {
                    if self.bids[i].price == entry.price {
                        self.bids[i].quantity = entry.aggregate_quantity;
                        self.bids[i].order_count = entry.number_of_orders;
                        return;
                    }
                }
            }
            UpdateAction::Delete => {
                let mut found = false;
                for i in 0..self.bid_count {
                    if self.bids[i].price == entry.price {
                        found = true;
                    }
                    if found && i + 1 < self.bid_count {
                        self.bids[i] = self.bids[i + 1];
                    }
                }
                if found {
                    self.bid_count -= 1;
                    self.bids[self.bid_count] = PriceLevel::new(0, 0, 0);
                }
            }
        }
    }

    #[inline(always)]
    fn apply_ask(&mut self, entry: &BookEntryRef) {
        match entry.update_action {
            UpdateAction::Clear => {
                self.asks = [PriceLevel::new(0, 0, 0); MAX_DEPTH];
                self.ask_count = 0;
            }
            UpdateAction::New => {
                let new_level = PriceLevel::new(
                    entry.price,
                    entry.aggregate_quantity,
                    entry.number_of_orders,
                );
                // Asks are sorted ascending (lowest price first)
                let mut idx = 0;
                while idx < self.ask_count && self.asks[idx].price < entry.price {
                    idx += 1;
                }

                if idx < MAX_DEPTH {
                    let end = self.ask_count.min(MAX_DEPTH - 1);
                    let mut j = end;
                    while j > idx {
                        self.asks[j] = self.asks[j - 1];
                        j -= 1;
                    }
                    self.asks[idx] = new_level;
                    if self.ask_count < MAX_DEPTH {
                        self.ask_count += 1;
                    }
                }
            }
            UpdateAction::Change => {
                for i in 0..self.ask_count {
                    if self.asks[i].price == entry.price {
                        self.asks[i].quantity = entry.aggregate_quantity;
                        self.asks[i].order_count = entry.number_of_orders;
                        return;
                    }
                }
            }
            UpdateAction::Delete => {
                let mut found = false;
                for i in 0..self.ask_count {
                    if self.asks[i].price == entry.price {
                        found = true;
                    }
                    if found && i + 1 < self.ask_count {
                        self.asks[i] = self.asks[i + 1];
                    }
                }
                if found {
                    self.ask_count -= 1;
                    self.asks[self.ask_count] = PriceLevel::new(0, 0, 0);
                }
            }
        }
    }
}
