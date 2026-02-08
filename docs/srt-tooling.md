# SRT Tooling

This document describes developer tooling for creating and inspecting Semantic Rendezvous Tokens (SRTs). The tools are presentation-layer helpers only; they do not define protocol semantics.

## Generate an SRT
```
rift srt generate --start-in 10 --window-secs 120 --slot-ms 250
```

Optional flags:
- `--role-hint <caller|callee|symmetric>`: human hint only.
- `--peer-identity <hex>[,<hex>...]`: restrict allowed fingerprints.

The command prints a summary and the encoded SRT URI.

## Inspect an SRT
```
rift srt inspect <riftd-srt://...>
```

The inspector decodes the token, prints the time model and constraints, and emits warnings for obviously invalid windows.

## Sharing
SRT URIs can be copied into chat, embedded in markdown, or shared as short-lived links. Tooling should remain transport-agnostic so it can be used with any exchange medium.
