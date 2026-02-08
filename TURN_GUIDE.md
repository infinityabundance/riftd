# Self-Hosted TURN Guide

This guide describes running a minimal TURN server for Rift. TURN is optional and only used when direct or punched paths fail.

## Recommended: coturn

Install:
```bash
# Debian/Ubuntu
sudo apt-get install coturn

# Arch
sudo pacman -S coturn
```

Minimal config (`/etc/turnserver.conf`):
```conf
listening-port=3478
fingerprint
lt-cred-mech
realm=rift
user=riftuser:riftpass
no-cli
no-loopback-peers
no-multicast-peers
```

Start:
```bash
sudo systemctl enable --now coturn
```

## Rift configuration

CLI:
```bash
rift create --channel gaming --internet --enable-turn \
  --turn-servers "turn:your.host:3478?username=riftuser&credential=riftpass"
```

Config file:
```toml
[network]
turn = true
turn_servers = ["turn:your.host:3478?username=riftuser&credential=riftpass"]
```

## Notes
- Prefer self-hosted TURN to avoid exposing metadata to third parties.
- TURN relays only see encrypted payloads, but can observe traffic patterns.
- Keep TURN credentials private; do not log them.
