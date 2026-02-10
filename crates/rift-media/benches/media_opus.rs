//! Criterion benchmark for Opus encode/decode.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rift_media::{AudioConfig, OpusDecoder, OpusEncoder};

/// Bench Opus encoding and decoding for a single frame.
fn bench_opus(c: &mut Criterion) {
    let config = AudioConfig::default();
    let frame_samples = (config.sample_rate as usize * config.frame_duration_ms as usize / 1000)
        * config.channels as usize;
    let samples = vec![0.1f32; frame_samples];
    let mut encoder = OpusEncoder::new(&config).unwrap();
    let mut decoder = OpusDecoder::new(&config).unwrap();
    let mut encode_buf = vec![0u8; 4096];
    let mut decode_buf = vec![0f32; frame_samples];

    c.bench_function("media_opus", |bench| {
        bench.iter(|| {
            let encoded_len = encoder.encode_f32(black_box(&samples), &mut encode_buf).unwrap();
            let decoded_len = decoder.decode_f32(black_box(&encode_buf[..encoded_len]), &mut decode_buf).unwrap();
            black_box(decoded_len);
        })
    });
}

criterion_group!(benches, bench_opus);
criterion_main!(benches);
