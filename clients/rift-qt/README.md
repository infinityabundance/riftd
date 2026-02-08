# Rift Qt (KDE)

Qt6/Kirigami desktop client for Rift using the C FFI (`rift.h` / `librift_sdk.so`).

## Build

```bash
cmake -S clients/rift-qt -B build/rift-qt \
  -DRIFT_SDK_LIB=/path/to/librift_sdk.so \
  -DRIFT_SDK_INCLUDE_DIR=/path/to/rift-sdk
cmake --build build/rift-qt
```

## Notes
- The UI expects Kirigami; build with `-DRIFT_QT_KIRIGAMI=ON`.
- Configure DHT bootstrap nodes in the UI when using internet discovery.
