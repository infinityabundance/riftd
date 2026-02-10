# rift-media

<p align="center">
  <a href="https://github.com/infinityabundance/riftd">
    <img src="https://raw.githubusercontent.com/infinityabundance/riftd/main/assets/riftd.svg" alt="riftd" width="80">
  </a>
</p>

<p align="center">
  Audio capture, playback, and Opus codec for the <a href="https://github.com/infinityabundance/riftd">riftd</a> P2P protocol.
</p>

---

Part of the [riftd](https://github.com/infinityabundance/riftd) project — serverless P2P voice + text chat over UDP.

## What's in this crate?

`rift-media` handles everything audio:

- **Audio Capture** — Cross-platform microphone input via cpal
- **Audio Playback** — Cross-platform speaker output via cpal
- **Opus Codec** — High-quality, low-latency voice encoding
- **VAD** — Voice activity detection to reduce bandwidth
- **Level Metering** — Audio level detection for UI indicators
- **Quality Presets** — Low/medium/high bitrate configurations

## Usage

```rust
use rift_media::{AudioConfig, OpusEncoder, OpusDecoder, AudioCapture};

let config = AudioConfig::default();

// Encoding
let mut encoder = OpusEncoder::new(&config)?;
let encoded = encoder.encode_f32(&samples, &mut output)?;

// Decoding
let mut decoder = OpusDecoder::new(&config)?;
let decoded = decoder.decode_f32(&encoded, &mut output)?;

// Capture from microphone
let capture = AudioCapture::new(&config)?;
capture.start(|samples| { /* process samples */ })?;
```

## Related Crates

| Crate | Description |
|-------|-------------|
| [rift-protocol](https://crates.io/crates/rift-protocol) | Voice frame message types |
| [rift-mesh](https://crates.io/crates/rift-mesh) | Sends/receives voice over mesh |
| [rift-sdk](https://crates.io/crates/rift-sdk) | High-level voice call API |

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
