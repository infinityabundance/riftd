use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rift_media::{AudioConfig, OpusDecoder, OpusEncoder};

fn bench_opus(c: &mut Criterion) {
    let config = AudioConfig::default();
    let frame_samples = (config.sample_rate as usize * config.frame_duration_ms as usize / 1000)
        * config.channels as usize;
    let samples = vec![0.1f32; frame_samples];
    let mut encoder = OpusEncoder::new(&config).unwrap();
    let mut decoder = OpusDecoder::new(&config).unwrap();

    c.bench_function("media_opus", |bench| {
        bench.iter(|| {
            let encoded = encoder.encode_f32(black_box(&samples)).unwrap();
            let decoded = decoder.decode_f32(black_box(&encoded)).unwrap();
            black_box(decoded);
        })
    });
}

criterion_group!(benches, bench_opus);
criterion_main!(benches);
