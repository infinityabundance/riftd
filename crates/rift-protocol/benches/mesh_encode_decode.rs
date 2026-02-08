use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rift_core::{MessageId, PeerId};
use rift_protocol::{ChatMessage, ControlMessage, ProtocolVersion, RiftFrameHeader, RiftPayload, StreamKind};

fn bench_encode_decode(c: &mut Criterion) {
    let header = RiftFrameHeader {
        version: ProtocolVersion::V1,
        stream: StreamKind::Text,
        flags: 0,
        seq: 42,
        timestamp: 1700000000,
        source: PeerId([7u8; 32]),
        session: rift_protocol::SessionId::NONE,
    };
    let chat = ChatMessage {
        id: MessageId::new(),
        from: PeerId([9u8; 32]),
        timestamp: 1700000000,
        text: "hello world".to_string(),
    };
    let payload = RiftPayload::Control(ControlMessage::Chat(chat));

    c.bench_function("mesh_encode_decode", |b| {
        b.iter(|| {
            let bytes = rift_protocol::encode_frame(black_box(&header), black_box(&payload));
            let (h, p) = rift_protocol::decode_frame(black_box(&bytes)).unwrap();
            black_box(h);
            black_box(p);
        })
    });
}

criterion_group!(benches, bench_encode_decode);
criterion_main!(benches);
