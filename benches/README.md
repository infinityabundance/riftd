# Rift Benchmarks

This folder contains lightweight performance benchmarks for Rift.

## Benchmarks
- `mesh_encode_decode`: encode/decode of protocol frames
- `mesh_encrypt_decrypt`: Noise session encrypt/decrypt throughput
- `media_opus`: Opus encode/decode of 20ms frames

## Running

```
cargo bench -p rift-protocol
cargo bench -p rift-mesh
cargo bench -p rift-media
```

Notes:
- Benchmarks are CPU-only and do not require network access.
- Use `-- --sample-size 50` to reduce runtime if needed.
