use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;

/// NAT translation interface for the simulator.
pub trait NatModel: Send {
    fn translate_outbound(&mut self, local_addr: SocketAddr, dest: SocketAddr, now_ms: u64) -> SocketAddr;
    fn translate_inbound(&mut self, external_dst: SocketAddr, now_ms: u64) -> Option<SocketAddr>;
}

#[derive(Debug, Clone)]
struct Mapping {
    internal: SocketAddr,
    external: SocketAddr,
    last_seen_ms: u64,
}

/// Port-preserving NAT (external port mirrors internal port).
#[derive(Debug, Clone)]
pub struct PortPreservingNat {
    base_ip: [u8; 4],
    timeout_ms: u64,
    mappings: HashMap<u16, Mapping>,
}

impl PortPreservingNat {
    pub fn new(base_port: u16, timeout_ms: u64) -> Self {
        let _ = base_port;
        Self {
            base_ip: [203, 0, 113, 1],
            timeout_ms,
            mappings: HashMap::new(),
        }
    }
}

impl NatModel for PortPreservingNat {
    fn translate_outbound(&mut self, local_addr: SocketAddr, _dest: SocketAddr, now_ms: u64) -> SocketAddr {
        let port = local_addr.port();
        let external = SocketAddr::new(self.base_ip.into(), port);
        self.mappings.insert(
            port,
            Mapping {
                internal: local_addr,
                external,
                last_seen_ms: now_ms,
            },
        );
        external
    }

    fn translate_inbound(&mut self, external_dst: SocketAddr, now_ms: u64) -> Option<SocketAddr> {
        let port = external_dst.port();
        if let Some(mapping) = self.mappings.get_mut(&port) {
            if now_ms.saturating_sub(mapping.last_seen_ms) <= self.timeout_ms {
                mapping.last_seen_ms = now_ms;
                return Some(mapping.internal);
            }
        }
        None
    }
}

/// Random port NAT (assigns a random external port per internal address).
#[derive(Debug, Clone)]
pub struct RandomPortNat {
    base_ip: [u8; 4],
    next_port: u16,
    timeout_ms: u64,
    mappings: HashMap<SocketAddr, Mapping>,
}

impl RandomPortNat {
    pub fn new(start_port: u16, timeout_ms: u64) -> Self {
        Self {
            base_ip: [203, 0, 113, 2],
            next_port: start_port,
            timeout_ms,
            mappings: HashMap::new(),
        }
    }

    fn allocate(&mut self) -> u16 {
        let port = self.next_port;
        self.next_port = self.next_port.wrapping_add(1);
        port
    }
}

impl NatModel for RandomPortNat {
    fn translate_outbound(&mut self, local_addr: SocketAddr, _dest: SocketAddr, now_ms: u64) -> SocketAddr {
        let entry = if self.mappings.contains_key(&local_addr) {
            self.mappings.get_mut(&local_addr).unwrap()
        } else {
            let mapping = Mapping {
                internal: local_addr,
                external: SocketAddr::new(self.base_ip.into(), self.allocate()),
                last_seen_ms: now_ms,
            };
            self.mappings.insert(local_addr, mapping);
            self.mappings.get_mut(&local_addr).unwrap()
        };
        entry.last_seen_ms = now_ms;
        entry.external
    }

    fn translate_inbound(&mut self, external_dst: SocketAddr, now_ms: u64) -> Option<SocketAddr> {
        for mapping in self.mappings.values_mut() {
            if mapping.external == external_dst && now_ms.saturating_sub(mapping.last_seen_ms) <= self.timeout_ms {
                mapping.last_seen_ms = now_ms;
                return Some(mapping.internal);
            }
        }
        None
    }
}

/// Symmetric NAT: mapping depends on destination.
#[derive(Debug, Clone)]
pub struct SymmetricNat {
    base_ip: [u8; 4],
    next_port: u16,
    timeout_ms: u64,
    mappings: HashMap<(SocketAddr, SocketAddr), Mapping>,
}

impl SymmetricNat {
    pub fn new(start_port: u16, _range: u16, timeout_ms: u64) -> Self {
        let _ = Duration::from_millis(timeout_ms);
        Self {
            base_ip: [203, 0, 113, 3],
            next_port: start_port,
            timeout_ms,
            mappings: HashMap::new(),
        }
    }

    fn allocate(&mut self) -> u16 {
        let port = self.next_port;
        self.next_port = self.next_port.wrapping_add(1);
        port
    }
}

impl NatModel for SymmetricNat {
    fn translate_outbound(&mut self, local_addr: SocketAddr, dest: SocketAddr, now_ms: u64) -> SocketAddr {
        let key = (local_addr, dest);
        let entry = if self.mappings.contains_key(&key) {
            self.mappings.get_mut(&key).unwrap()
        } else {
            let mapping = Mapping {
                internal: local_addr,
                external: SocketAddr::new(self.base_ip.into(), self.allocate()),
                last_seen_ms: now_ms,
            };
            self.mappings.insert(key, mapping);
            self.mappings.get_mut(&key).unwrap()
        };
        entry.last_seen_ms = now_ms;
        entry.external
    }

    fn translate_inbound(&mut self, external_dst: SocketAddr, now_ms: u64) -> Option<SocketAddr> {
        for mapping in self.mappings.values_mut() {
            if mapping.external == external_dst && now_ms.saturating_sub(mapping.last_seen_ms) <= self.timeout_ms {
                mapping.last_seen_ms = now_ms;
                return Some(mapping.internal);
            }
        }
        None
    }
}
