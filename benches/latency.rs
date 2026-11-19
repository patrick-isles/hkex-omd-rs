use criterion::{black_box, criterion_group, criterion_main, Criterion};
use hkex_omd_rs::{
    BookEntryRef, MarketByPriceBook, OmdFeedHandler, PacketHeader, Side, UpdateAction,
    MSG_AGGREGATE_ORDER_BOOK_UPDATE,
};

fn build_bench_packet() -> Vec<u8> {
    let mut buf = Vec::with_capacity(128);
    // Header
    buf.extend_from_slice(&64u16.to_le_bytes()); // pkt_size
    buf.push(1); // msg_count
    buf.push(0); // filler
    buf.extend_from_slice(&1001u32.to_le_bytes()); // seq_num
    buf.extend_from_slice(&1700000000000000u64.to_le_bytes()); // send_time

    // MsgType 53 with 2 entries
    let msg_size = (12 + 2 * 18) as u16; // 48
    buf.extend_from_slice(&msg_size.to_le_bytes());
    buf.extend_from_slice(&MSG_AGGREGATE_ORDER_BOOK_UPDATE.to_le_bytes());
    buf.extend_from_slice(&700u32.to_le_bytes()); // Tencent
    buf.extend_from_slice(&[0, 0, 0]); // filler
    buf.push(2); // no_entries

    // Entry 1: Bid
    buf.extend_from_slice(&10_000u64.to_le_bytes());
    buf.extend_from_slice(&385_000i32.to_le_bytes());
    buf.extend_from_slice(&12u32.to_le_bytes());
    buf.push(UpdateAction::New as u8);
    buf.push(Side::Bid as u8);

    // Entry 2: Ask
    buf.extend_from_slice(&15_000u64.to_le_bytes());
    buf.extend_from_slice(&386_000i32.to_le_bytes());
    buf.extend_from_slice(&18u32.to_le_bytes());
    buf.push(UpdateAction::New as u8);
    buf.push(Side::Ask as u8);

    buf
}

fn bench_packet_header_parse(c: &mut Criterion) {
    let raw = build_bench_packet();
    c.bench_function("packet_header_parse", |b| {
        b.iter(|| {
            let hdr = PacketHeader::parse(black_box(&raw)).unwrap();
            black_box(hdr);
        })
    });
}

fn bench_order_book_apply(c: &mut Criterion) {
    let mut book = MarketByPriceBook::new(700);
    let entry = BookEntryRef {
        aggregate_quantity: 5_000,
        price: 385_500,
        number_of_orders: 8,
        update_action: UpdateAction::New,
        side: Side::Bid,
    };

    c.bench_function("order_book_apply_entry", |b| {
        b.iter(|| {
            book.apply_entry(black_box(&entry));
            black_box(&book);
        })
    });
}

fn bench_order_book_analytics(c: &mut Criterion) {
    let mut book = MarketByPriceBook::new(700);
    for i in 1..=10 {
        book.apply_entry(&BookEntryRef {
            aggregate_quantity: 1000 * i as u64,
            price: 385_000 - (i * 500),
            number_of_orders: i as u32,
            update_action: UpdateAction::New,
            side: Side::Bid,
        });
        book.apply_entry(&BookEntryRef {
            aggregate_quantity: 1000 * i as u64,
            price: 385_500 + (i * 500),
            number_of_orders: i as u32,
            update_action: UpdateAction::New,
            side: Side::Ask,
        });
    }

    c.bench_function("order_book_microprice", |b| {
        b.iter(|| {
            black_box(book.microprice());
        })
    });

    c.bench_function("order_book_depth_imbalance", |b| {
        b.iter(|| {
            black_box(book.depth_imbalance());
        })
    });

    c.bench_function("order_book_vwap_ask_sweep", |b| {
        b.iter(|| {
            black_box(book.vwap_ask(black_box(15_000)));
        })
    });
}

fn bench_end_to_end_feed_packet(c: &mut Criterion) {
    let raw = build_bench_packet();
    let mut handler = OmdFeedHandler::new();
    handler.register_security(700);

    c.bench_function("feed_handler_process_packet", |b| {
        b.iter(|| {
            let report = handler.process_packet(black_box(&raw)).unwrap();
            black_box(report);
        })
    });
}

criterion_group!(
    benches,
    bench_packet_header_parse,
    bench_order_book_apply,
    bench_order_book_analytics,
    bench_end_to_end_feed_packet
);
criterion_main!(benches);
