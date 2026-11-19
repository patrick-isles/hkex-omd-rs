//! Price level representations for limit order book architectures.

use core::fmt;

/// An individual price level in an aggregated limit order book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub struct PriceLevel {
    /// Integer price with 3 implied decimal places (e.g. 50000 = 50.000 HKD).
    pub price: i32,
    /// Total aggregate shares available at this price level.
    pub quantity: u64,
    /// Number of individual broker orders contributing to this level.
    pub order_count: u32,
}

impl PriceLevel {
    #[inline(always)]
    pub const fn new(price: i32, quantity: u64, order_count: u32) -> Self {
        Self {
            price,
            quantity,
            order_count,
        }
    }

    /// Price in standard decimal currency units (e.g., 385.00 HKD).
    #[inline(always)]
    pub fn price_f64(&self) -> f64 {
        self.price as f64 / 1000.0
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.quantity == 0 || self.price == 0
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.price = 0;
        self.quantity = 0;
        self.order_count = 0;
    }
}

impl fmt::Display for PriceLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:.3} x {} ({} orders)",
            self.price_f64(),
            self.quantity,
            self.order_count
        )
    }
}
