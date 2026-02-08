# Release Checklist (v0.1.0)

## Code / Build
- [ ] `cargo fmt` clean
- [ ] `cargo clippy --workspace --all-targets -D warnings`
- [ ] `cargo test --workspace`
- [ ] E2E tests (LAN + STUN/TURN as available)
- [ ] Android build (debug APK)
- [ ] Qt clients build (Linux/Windows)

## Security
- [ ] SECURITY.md reviewed
- [ ] Known limitations documented
- [ ] Audit log settings verified

## Docs
- [ ] README.md updated
- [ ] PROTOCOL.md updated for any protocol changes
- [ ] TURN_GUIDE.md reviewed
- [ ] CHANGELOG.md updated

## Release Artifacts
- [ ] Tag `v0.1.0`
- [ ] GitHub Release notes
- [ ] `rift` binary (Linux)
- [ ] `rift-sdk` shared lib (Linux)
- [ ] Android APK
- [ ] Optional Docker relay image

## Publish
- [ ] crates.io publish (core/protocol/mesh/sdk)
- [ ] Announce release
