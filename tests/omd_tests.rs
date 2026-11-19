//! Comprehensive unit and integration tests for HKEX OMD-C feed handler and order book.

use hkex_omd_rs::{
    BookEntryRef, MarketByPriceBook, OmdError, OmdFeedHandler, PacketHeader, SequenceReset,
    SequenceStatus, SequenceTracker, Side, UpdateAction, MSG_AGGREGATE_ORDER_BOOK_UPDATE,
    MSG_TRADE,
};

/// Helper to build a standard 16-byte HKEX OMD-C packet header.
fn build_header(pkt_size: u16, msg_count: u8, seq_num: u32, send_time: u64) -> [u8; 16] {
    let mut buf = [0u8; 16];
    buf[0..2].copy_from_slice(&pkt_size.to_le_bytes());
    buf[2] = msg_count;
    buf[3] = 0; // filler
    buf[4..8].copy_from_slice(&seq_num.to_le_bytes());
    buf[8..16].copy_from_slice(&send_time.to_le_bytes());
    buf
}

/// Helper to encode an AggregateOrderBookUpdate (MsgType 53) message.
fn encode_book_update(
    security_code: u32,
    entries: &[(u64, i32, u32, UpdateAction, Side)],
) -> Vec<u8> {
    let no_entries = entries.len() as u8;
    let msg_size = (12 + entries.len() * 18) as u16;
    let mut buf = Vec::with_capacity(msg_size as usize);

    // Msg Header
    buf.extend_from_slice(&msg_size.to_le_bytes());
    buf.extend_from_slice(&MSG_AGGREGATE_ORDER_BOOK_UPDATE.to_le_bytes());

    // Payload Header: SecurityCode (4), Filler (3), NoEntries (1)
    buf.extend_from_slice(&security_code.to_le_bytes());
    buf.extend_from_slice(&[0u8, 0, 0]);
    buf.push(no_entries);

    // Repeating entries
    for &(qty, price, orders, action, side) in entries {
        buf.extend_from_slice(&qty.to_le_bytes());
        buf.extend_from_slice(&price.to_le_bytes());
        buf.extend_from_slice(&orders.to_le_bytes());
        buf.push(action as u8);
        buf.push(side as u8);
    }

    buf
}

/// Helper to encode a Trade (MsgType 50) message.
fn encode_trade(
    security_code: u32,
    trade_id: u32,
    price: i32,
    quantity: u32,
    trd_type: i16,
    trade_time: u64,
) -> Vec<u8> {
    let msg_size = 30u16;
    let mut buf = Vec::with_capacity(msg_size as usize);

    buf.extend_from_slice(&msg_size.to_le_bytes());
    buf.extend_from_slice(&MSG_TRADE.to_le_bytes());
    buf.extend_from_slice(&security_code.to_le_bytes());
    buf.extend_from_slice(&trade_id.to_le_bytes());
    buf.extend_from_slice(&price.to_le_bytes());
    buf.extend_from_slice(&quantity.to_le_bytes());
    buf.extend_from_slice(&trd_type.to_le_bytes());
    buf.extend_from_slice(&trade_time.to_le_bytes());

    buf
}

#[test]
fn test_packet_header_parsing() {
    let hdr_bytes = build_header(64, 2, 1001, 1700000000123456789);
    let parsed = PacketHeader::parse(&hdr_bytes).expect("Valid header");

    assert_eq!(parsed.pkt_size, 64);
    assert_eq!(parsed.msg_count, 2);
    assert_eq!(parsed.seq_num, 1001);
    assert_eq!(parsed.send_time, 1700000000123456789);

    // Short buffer test
    let err = PacketHeader::parse(&hdr_bytes[..12]).unwrap_err();
    assert_eq!(
        err,
        OmdError::PacketTooShort {
            actual: 12,
            required: 16
        }
    );
}

#[test]
fn test_order_book_sorting_and_bbo() {
    let mut book = MarketByPriceBook::new(700); // Tencent Holdings (00700.HK)

    // Insert Bids: 385.000, 386.000, 384.500
    book.apply_entry(&BookEntryRef {
        aggregate_quantity: 10_000,
        price: 385_000,
        number_of_orders: 12,
        update_action: UpdateAction::New,
        side: Side::Bid,
    });
    book.apply_entry(&BookEntryRef {
        aggregate_quantity: 5_000,
        price: 386_000,
        number_of_orders: 6,
        update_action: UpdateAction::New,
        side: Side::Bid,
    });
    book.apply_entry(&BookEntryRef {
        aggregate_quantity: 20_000,
        price: 384_500,
        number_of_orders: 25,
        update_action: UpdateAction::New,
        side: Side::Bid,
    });

    // Bids must be sorted descending: 386.000 > 385.000 > 384.500
    assert_eq!(book.bid_depth(), 3);
    assert_eq!(book.bids()[0].price, 386_000);
    assert_eq!(book.bids()[1].price, 385_000);
    assert_eq!(book.bids()[2].price, 384_500);

    // Insert Asks: 387.000, 386.500, 388.000
    book.apply_entry(&BookEntryRef {
        aggregate_quantity: 8_000,
        price: 387_000,
        number_of_orders: 10,
        update_action: UpdateAction::New,
        side: Side::Ask,
    });
    book.apply_entry(&BookEntryRef {
        aggregate_quantity: 4_000,
        price: 386_500,
        number_of_orders: 4,
        update_action: UpdateAction::New,
        side: Side::Ask,
    });
    book.apply_entry(&BookEntryRef {
        aggregate_quantity: 15_000,
        price: 388_000,
        number_of_orders: 18,
        update_action: UpdateAction::New,
        side: Side::Ask,
    });

    // Asks must be sorted ascending: 386.500 < 387.000 < 388.000
    assert_eq!(book.ask_depth(), 3);
    assert_eq!(book.asks()[0].price, 386_500);
    assert_eq!(book.asks()[1].price, 387_000);
    assert_eq!(book.asks()[2].price, 388_000);

    // BBO Verification
    let (best_bid, best_ask) = book.bbo().expect("BBO present");
    assert_eq!(best_bid.price, 386_000);
    assert_eq!(best_bid.quantity, 5_000);
    assert_eq!(best_ask.price, 386_500);
    assert_eq!(best_ask.quantity, 4_000);

    // Spread: 386.500 - 386.000 = 0.500 HKD (500 ticks)
    assert_eq!(book.spread(), Some(500));
    assert!((book.spread_f64().unwrap() - 0.500).abs() < 1e-6);

    // Midprice: (386.000 + 386.500) / 2 = 386.250
    assert!((book.mid_price().unwrap() - 386.250).abs() < 1e-6);

    // Microprice: (386 * 4000 + 386.5 * 5000) / 9000 = 3476500 / 9000 ≈ 386.2777...
    let micro = book.microprice().unwrap();
    assert!((micro - 386.277777).abs() < 1e-4);

    // Depth Imbalance:
    // Total Bids = 5000 + 10000 + 20000 = 35000
    // Total Asks = 4000 + 8000 + 15000 = 27000
    // OFI = (35000 - 27000) / 62000 = 8000 / 62000 ≈ 0.12903
    let ofi = book.depth_imbalance();
    assert!((ofi - (8000.0 / 62000.0)).abs() < 1e-5);
}

#[test]
fn test_order_book_modifications_and_vwap() {
    let mut book = MarketByPriceBook::new(9988); // Alibaba Group (09988.HK)

    // Seed 3 Ask levels
    book.apply_entry(&BookEntryRef {
        aggregate_quantity: 1_000,
        price: 80_000, // 80.000 HKD
        number_of_orders: 1,
        update_action: UpdateAction::New,
        side: Side::Ask,
    });
    book.apply_entry(&BookEntryRef {
        aggregate_quantity: 2_000,
        price: 80_500, // 80.500 HKD
        number_of_orders: 2,
        update_action: UpdateAction::New,
        side: Side::Ask,
    });
    book.apply_entry(&BookEntryRef {
        aggregate_quantity: 5_000,
        price: 81_000, // 81.000 HKD
        number_of_orders: 5,
        update_action: UpdateAction::New,
        side: Side::Ask,
    });

    // Test VWAP Ask for 2,500 shares:
    // 1000 shares @ 80.00 = 80,000
    // 1500 shares @ 80.50 = 120,750
    // Total cost = 200,750 / 2500 = 80.30 HKD
    let vwap = book.vwap_ask(2_500).expect("Sufficient depth for VWAP");
    assert!((vwap - 80.30).abs() < 1e-6);

    // Modify price level 80.500 quantity to 4,000
    book.apply_entry(&BookEntryRef {
        aggregate_quantity: 4_000,
        price: 80_500,
        number_of_orders: 4,
        update_action: UpdateAction::Change,
        side: Side::Ask,
    });
    assert_eq!(book.asks()[1].quantity, 4_000);
    assert_eq!(book.asks()[1].order_count, 4);

    // Delete best ask (80.000)
    book.apply_entry(&BookEntryRef {
        aggregate_quantity: 0,
        price: 80_000,
        number_of_orders: 0,
        update_action: UpdateAction::Delete,
        side: Side::Ask,
    });
    assert_eq!(book.ask_depth(), 2);
    assert_eq!(book.best_ask().unwrap().price, 80_500);

    // Clear Asks
    book.apply_entry(&BookEntryRef {
        aggregate_quantity: 0,
        price: 0,
        number_of_orders: 0,
        update_action: UpdateAction::Clear,
        side: Side::Ask,
    });
    assert_eq!(book.ask_depth(), 0);
    assert!(book.best_ask().is_none());
}

#[test]
fn test_sequence_reset_parsing() {
    let raw = 5000u32.to_le_bytes();
    let reset = SequenceReset::parse(&raw).expect("Valid SequenceReset payload");
    assert_eq!(reset.new_seq_num, 5000);
}

#[test]
fn test_sequence_tracker_gaps_and_stale() {
    let mut tracker = SequenceTracker::with_expected(100);

    // In-order packet with 5 messages
    let status = tracker.on_packet(100, 5);
    assert_eq!(status, SequenceStatus::InOrder);
    assert_eq!(tracker.expected(), 105);

    // Duplicate/stale packet
    let stale = tracker.on_packet(102, 3);
    assert!(matches!(stale, SequenceStatus::DuplicateOrStale { .. }));
    assert_eq!(tracker.total_duplicates(), 1);

    // Gap detected: jump to sequence 120 (missing 105..119)
    let gap = tracker.on_packet(120, 4);
    assert_eq!(
        gap,
        SequenceStatus::GapDetected {
            expected: 105,
            received: 120,
            missing: 15,
        }
    );
    assert_eq!(tracker.total_gaps(), 1);
    assert_eq!(tracker.expected(), 124);
}

#[test]
fn test_end_to_end_feed_handler() {
    let mut handler = OmdFeedHandler::new();

    // Construct a packet containing:
    // Message 1: AggregateOrderBookUpdate for HKEX 700 (2 entries: 1 Bid, 1 Ask)
    // Message 2: Trade for HKEX 700
    let book_entries = [
        (10_000u64, 380_000i32, 8u32, UpdateAction::New, Side::Bid),
        (15_000u64, 381_000i32, 12u32, UpdateAction::New, Side::Ask),
    ];
    let msg1 = encode_book_update(700, &book_entries);
    let msg2 = encode_trade(700, 99901, 380_500, 2_000, 0, 1700000000100000);

    let total_pkt_len = 16 + msg1.len() + msg2.len();
    let mut packet = Vec::with_capacity(total_pkt_len);
    packet.extend_from_slice(&build_header(
        total_pkt_len as u16,
        2,
        5001,
        1700000000000000,
    ));
    packet.extend_from_slice(&msg1);
    packet.extend_from_slice(&msg2);

    let report = handler
        .process_packet(&packet)
        .expect("Processed packet cleanly");
    assert_eq!(report.seq_num, 5001);
    assert_eq!(report.msg_count, 2);
    assert_eq!(report.book_updates, 1);
    assert_eq!(report.trades, 1);
    assert_eq!(report.seq_status, SequenceStatus::InOrder);

    // Verify order book for 700 was populated
    let book = handler.get_book(700).expect("Book for 700 exists");
    assert_eq!(book.security_code, 700);
    assert_eq!(book.bids()[0].price, 380_000);
    assert_eq!(book.asks()[0].price, 381_000);
    assert_eq!(book.spread(), Some(1_000)); // 1.00 HKD spread
    assert_eq!(book.last_seq_num, 5001);

    // Verify metrics
    let m = handler.metrics();
    assert_eq!(m.packets_received, 1);
    assert_eq!(m.messages_parsed, 2);
    assert_eq!(m.book_entries_applied, 2);
    assert_eq!(m.trades_recorded, 1);
    assert_eq!(m.sequence_gaps, 0);
}
