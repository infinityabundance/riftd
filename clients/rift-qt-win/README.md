# Rift Qt Windows

Qt6 desktop client for Windows using rift-sdk C FFI.

## Build

```powershell
cmake -S clients/rift-qt-win -B build/rift-qt-win `
  -DRIFT_SDK_LIB=C:\path\to\rift_sdk.lib `
  -DRIFT_SDK_INCLUDE_DIR=C:\path\to\rift-sdk
cmake --build build/rift-qt-win --config Release
```

## Run (dev)

Copy `rift_sdk.dll` next to `rift-qt-win.exe` or ensure it is on PATH.

## Notes
- Use DHT + bootstrap nodes for internet discovery.
- Space starts PTT when chat input is not focused.
