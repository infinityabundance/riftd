# rift-wasm

WebAssembly bindings for Rift P2P browser clients.

## Features

- Browser-compatible P2P messaging
- E2EE encryption/decryption
- Voice frame encoding/decoding
- Audio utilities (level detection, gain, mixing)

## Usage

```javascript
import init, { WasmClient } from 'rift-wasm';

await init();
const client = new WasmClient();
```

## Building

```bash
wasm-pack build --target web
```

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
