# rift-wasm

<p align="center">
  <a href="https://github.com/infinityabundance/riftd">
    <img src="https://raw.githubusercontent.com/infinityabundance/riftd/main/assets/riftd.svg" alt="riftd" width="80">
  </a>
</p>

<p align="center">
  WebAssembly bindings for browser clients of the <a href="https://github.com/infinityabundance/riftd">riftd</a> P2P protocol.
</p>

---

Part of the [riftd](https://github.com/infinityabundance/riftd) project — serverless P2P voice + text chat over UDP.

## What's in this crate?

`rift-wasm` brings rift to the browser:

- **WasmClient** — Browser-compatible rift client
- **E2EE** — Encrypt/decrypt messages in the browser
- **Voice Encoding** — Encode/decode voice frames
- **Audio Utilities** — Level detection, gain, mixing
- **PCM Conversion** — Float32 ↔ PCM16 for Web Audio API

## Usage

```javascript
import init, { WasmClient, AudioConfig } from 'rift-wasm';

await init();

const client = new WasmClient();

// Encrypt a message
const encrypted = client.encrypt(plaintext);

// Encode voice for transmission
const encoded = client.encode_voice_pcm(pcmSamples);

// Audio utilities
const level = audio_level(samples);
const isActive = is_voice_active(samples);
```

## Building

```bash
# Install wasm-pack
cargo install wasm-pack

# Build for web
wasm-pack build --target web

# Build for bundlers
wasm-pack build --target bundler
```

## Browser Prototype

See [README.browser.md](https://github.com/infinityabundance/riftd/blob/main/README.browser.md) for the browser demo setup.

## Related Crates

| Crate | Description |
|-------|-------------|
| [rift-core](https://crates.io/crates/rift-core) | Core types and crypto |
| [rift-protocol](https://crates.io/crates/rift-protocol) | Message encoding |
| [rift-sdk](https://crates.io/crates/rift-sdk) | Native SDK |

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
