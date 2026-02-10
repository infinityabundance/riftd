# rift-media

Audio capture, playback, and Opus codec for Rift P2P.

## Features

- Cross-platform audio capture and playback (via cpal)
- Opus encoding and decoding
- Voice activity detection
- Audio level metering

## Usage

```rust
use rift_media::{AudioConfig, OpusEncoder, OpusDecoder};

let config = AudioConfig::default();
let encoder = OpusEncoder::new(&config)?;
let decoder = OpusDecoder::new(&config)?;
```

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
