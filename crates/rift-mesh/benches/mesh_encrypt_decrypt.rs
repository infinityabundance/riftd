use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rift_core::noise::{noise_builder, NoiseSession};

fn bench_noise_encrypt(c: &mut Criterion) {
    let builder = noise_builder();
    let static_kp = builder.generate_keypair().unwrap();
    let mut initiator = builder
        .local_private_key(&static_kp.private)
        .build_initiator()
        .unwrap();
    let mut responder = builder
        .local_private_key(&static_kp.private)
        .build_responder()
        .unwrap();

    let mut buf = [0u8; 1024];
    let len = initiator.write_message(&[], &mut buf).unwrap();
    responder.read_message(&buf[..len], &mut []).unwrap();
    let len = responder.write_message(&[], &mut buf).unwrap();
    initiator.read_message(&buf[..len], &mut []).unwrap();
    let len = initiator.write_message(&[], &mut buf).unwrap();
    responder.read_message(&buf[..len], &mut []).unwrap();

    let mut a = NoiseSession::new(initiator.into_transport_mode().unwrap());
    let mut b = NoiseSession::new(responder.into_transport_mode().unwrap());
    let payload = vec![0u8; 256];

    c.bench_function("mesh_encrypt_decrypt", |bench| {
        bench.iter(|| {
            let mut out = vec![0u8; payload.len() + 128];
            let len = a.encrypt(black_box(&payload), &mut out).unwrap();
            out.truncate(len);
            let mut out2 = vec![0u8; out.len() + 128];
            let len2 = b.decrypt(black_box(&out), &mut out2).unwrap();
            out2.truncate(len2);
            black_box(out2);
        })
    });
}

criterion_group!(benches, bench_noise_encrypt);
criterion_main!(benches);
