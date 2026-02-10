# rift-sdk

<p align="center">
  <a href="https://github.com/infinityabundance/riftd">
    <img src="https://raw.githubusercontent.com/infinityabundance/riftd/main/assets/riftd.svg" alt="riftd" width="80">
  </a>
</p>

<p align="center">
  High-level SDK for building applications with the <a href="https://github.com/infinityabundance/riftd">riftd</a> P2P protocol.
</p>

---

Part of the [riftd](https://github.com/infinityabundance/riftd) project — serverless P2P voice + text chat over UDP.

## What's in this crate?

`rift-sdk` is the easiest way to add P2P voice and text chat to your app:

- **Simple API** — High-level abstractions over the mesh
- **Auto Discovery** — mDNS + DHT peer finding
- **NAT Traversal** — Just works™ (hole punch + TURN fallback)
- **E2EE by Default** — All messages encrypted end-to-end
- **Voice Calls** — Opus-encoded voice with QoS adaptation
- **Cross-Platform** — Desktop, mobile (via FFI), and WASM

## Usage

```rust
use rift_sdk::{RiftConfig, RiftClient};

#[tokio::main]
async fn main() -> Result<()> {
    let config = RiftConfig::default();
    let client = RiftClient::new(config).await?;

    // Join a channel
    client.join("my-channel").await?;

    // Send a message
    client.send_chat("Hello, world!").await?;

    // Start a voice call
    client.start_call(peer_id).await?;

    // Handle events
    while let Some(event) = client.next_event().await {
        match event {
            RiftEvent::Chat { from, message } => println!("{}: {}", from, message),
            RiftEvent::CallStarted { peer } => println!("Call with {}", peer),
            _ => {}
        }
    }

    Ok(())
}
```

## Features

- `ffi` — C FFI bindings for mobile/native integration
- `android` — Android JNI bindings

## Platform Support

| Platform | Status |
|----------|--------|
| Linux | ✅ Full support |
| macOS | ✅ Full support |
| Windows | ✅ Full support |
| Android | ✅ Via JNI |
| iOS | 🚧 Planned |
| Web | ✅ Via rift-wasm |

## Related Crates

| Crate | Description |
|-------|-------------|
| [rift-mesh](https://crates.io/crates/rift-mesh) | Low-level mesh networking |
| [rift-media](https://crates.io/crates/rift-media) | Audio capture/playback |
| [rift-wasm](https://crates.io/crates/rift-wasm) | Browser bindings |

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
