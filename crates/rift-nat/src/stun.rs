//! STUN (Session Traversal Utilities for NAT) RFC 5389 implementation.
//!
//! This module implements STUN Binding Request/Response for discovering
//! public (server-reflexive) addresses through STUN servers.

use std::net::{SocketAddr, IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time::timeout;
use rand::RngCore;
use crate::StunError;

/// STUN message type for Binding Request (0x0001)
const STUN_BINDING_REQUEST: u16 = 0x0001;
/// STUN message type for Binding Response (0x0101)
const STUN_BINDING_RESPONSE: u16 = 0x0101;
/// STUN magic cookie (0x2112A442)
const STUN_MAGIC_COOKIE: u32 = 0x2112A442;
/// STUN attribute: MAPPED-ADDRESS (0x0001)
const STUN_ATTR_MAPPED_ADDRESS: u16 = 0x0001;
/// STUN attribute: XOR-MAPPED-ADDRESS (0x0020)
const STUN_ATTR_XOR_MAPPED_ADDRESS: u16 = 0x0020;

/// STUN Binding Request message builder.
pub struct StunBindingRequest {
    transaction_id: [u8; 12],
}

impl StunBindingRequest {
    /// Create a new STUN Binding Request with a random transaction ID.
    pub fn new() -> Self {
        let mut transaction_id = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut transaction_id);
        Self { transaction_id }
    }

    /// Encode the STUN Binding Request to bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(20);
        
        // STUN header (20 bytes)
        // Message Type (2 bytes)
        buf.extend_from_slice(&STUN_BINDING_REQUEST.to_be_bytes());
        
        // Message Length (2 bytes) - no attributes
        buf.extend_from_slice(&0u16.to_be_bytes());
        
        // Magic Cookie (4 bytes)
        buf.extend_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
        
        // Transaction ID (12 bytes)
        buf.extend_from_slice(&self.transaction_id);
        
        buf
    }

    /// Get the transaction ID for matching responses.
    pub fn transaction_id(&self) -> &[u8; 12] {
        &self.transaction_id
    }
}

/// STUN Binding Response parser.
pub struct StunBindingResponse {
    pub mapped_address: Option<SocketAddr>,
    pub xor_mapped_address: Option<SocketAddr>,
}

impl StunBindingResponse {
    /// Parse a STUN Binding Response from bytes.
    pub fn parse(data: &[u8]) -> Result<Self, StunError> {
        if data.len() < 20 {
            return Err(StunError::InvalidFormat("Message too short".into()));
        }

        // Parse header
        let message_type = u16::from_be_bytes([data[0], data[1]]);
        if message_type != STUN_BINDING_RESPONSE {
            return Err(StunError::InvalidFormat(format!(
                "Invalid message type: 0x{:04x}",
                message_type
            )));
        }

        let message_length = u16::from_be_bytes([data[2], data[3]]) as usize;
        let magic_cookie = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        
        if magic_cookie != STUN_MAGIC_COOKIE {
            return Err(StunError::InvalidFormat("Invalid magic cookie".into()));
        }

        let transaction_id = &data[8..20];

        if data.len() < 20 + message_length {
            return Err(StunError::InvalidFormat("Truncated message".into()));
        }

        // Parse attributes
        let mut offset = 20;
        let mut mapped_address = None;
        let mut xor_mapped_address = None;

        while offset + 4 <= 20 + message_length {
            let attr_type = u16::from_be_bytes([data[offset], data[offset + 1]]);
            let attr_length = u16::from_be_bytes([data[offset + 2], data[offset + 3]]) as usize;
            offset += 4;

            if offset + attr_length > 20 + message_length {
                break;
            }

            let attr_data = &data[offset..offset + attr_length];

            match attr_type {
                STUN_ATTR_MAPPED_ADDRESS => {
                    if let Ok(addr) = Self::parse_mapped_address(attr_data) {
                        mapped_address = Some(addr);
                    }
                }
                STUN_ATTR_XOR_MAPPED_ADDRESS => {
                    if let Ok(addr) = Self::parse_xor_mapped_address(attr_data, transaction_id) {
                        xor_mapped_address = Some(addr);
                    }
                }
                _ => {
                    // Ignore unknown attributes
                }
            }

            // Attributes are padded to 4-byte boundaries
            offset += attr_length;
            let padding = (4 - (attr_length % 4)) % 4;
            offset += padding;
        }

        Ok(Self {
            mapped_address,
            xor_mapped_address,
        })
    }

    /// Parse a MAPPED-ADDRESS attribute.
    fn parse_mapped_address(data: &[u8]) -> Result<SocketAddr, StunError> {
        if data.len() < 4 {
            return Err(StunError::InvalidFormat("MAPPED-ADDRESS too short".into()));
        }

        let family = data[1];
        let port = u16::from_be_bytes([data[2], data[3]]);

        match family {
            0x01 => {
                // IPv4
                if data.len() < 8 {
                    return Err(StunError::InvalidFormat("IPv4 address too short".into()));
                }
                let addr = Ipv4Addr::new(data[4], data[5], data[6], data[7]);
                Ok(SocketAddr::new(IpAddr::V4(addr), port))
            }
            0x02 => {
                // IPv6
                if data.len() < 20 {
                    return Err(StunError::InvalidFormat("IPv6 address too short".into()));
                }
                let mut segments = [0u16; 8];
                for i in 0..8 {
                    segments[i] = u16::from_be_bytes([data[4 + i * 2], data[5 + i * 2]]);
                }
                let addr = Ipv6Addr::from(segments);
                Ok(SocketAddr::new(IpAddr::V6(addr), port))
            }
            _ => Err(StunError::InvalidFormat(format!("Unknown address family: {}", family))),
        }
    }

    /// Parse an XOR-MAPPED-ADDRESS attribute.
    fn parse_xor_mapped_address(data: &[u8], transaction_id: &[u8]) -> Result<SocketAddr, StunError> {
        if data.len() < 4 {
            return Err(StunError::InvalidFormat("XOR-MAPPED-ADDRESS too short".into()));
        }

        let family = data[1];
        let xor_port = u16::from_be_bytes([data[2], data[3]]);
        let port = xor_port ^ (STUN_MAGIC_COOKIE >> 16) as u16;

        match family {
            0x01 => {
                // IPv4
                if data.len() < 8 {
                    return Err(StunError::InvalidFormat("IPv4 address too short".into()));
                }
                let xor_addr = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
                let addr_u32 = xor_addr ^ STUN_MAGIC_COOKIE;
                let addr = Ipv4Addr::from(addr_u32);
                Ok(SocketAddr::new(IpAddr::V4(addr), port))
            }
            0x02 => {
                // IPv6
                if data.len() < 20 {
                    return Err(StunError::InvalidFormat("IPv6 address too short".into()));
                }
                
                // XOR with magic cookie + transaction ID
                let mut xor_key = Vec::with_capacity(16);
                xor_key.extend_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
                xor_key.extend_from_slice(transaction_id);

                let mut addr_bytes = [0u8; 16];
                for i in 0..16 {
                    addr_bytes[i] = data[4 + i] ^ xor_key[i];
                }
                
                let addr = Ipv6Addr::from(addr_bytes);
                Ok(SocketAddr::new(IpAddr::V6(addr), port))
            }
            _ => Err(StunError::InvalidFormat(format!("Unknown address family: {}", family))),
        }
    }

    /// Get the best mapped address (prefer XOR-MAPPED over MAPPED).
    pub fn get_mapped_address(&self) -> Option<SocketAddr> {
        self.xor_mapped_address.or(self.mapped_address)
    }
}

/// STUN client for discovering public addresses.
pub struct StunClient {
    /// List of STUN servers to query.
    servers: Vec<SocketAddr>,
    /// Timeout for each STUN query.
    timeout: Duration,
}

impl StunClient {
    /// Create a new STUN client.
    pub fn new(servers: Vec<SocketAddr>, timeout: Duration) -> Self {
        Self { servers, timeout }
    }

    /// Discover the public address using STUN.
    /// 
    /// Tries each server in sequence until one responds.
    pub async fn discover_public_addr(&self, socket: &UdpSocket) -> Result<SocketAddr, StunError> {
        if self.servers.is_empty() {
            return Err(StunError::NoServers);
        }

        for server in &self.servers {
            match self.query_server(socket, *server).await {
                Ok(addr) => return Ok(addr),
                Err(e) => {
                    tracing::debug!("STUN query to {} failed: {}", server, e);
                    continue;
                }
            }
        }

        Err(StunError::NoServers)
    }

    /// Query a single STUN server.
    async fn query_server(&self, socket: &UdpSocket, server: SocketAddr) -> Result<SocketAddr, StunError> {
        let request = StunBindingRequest::new();
        let request_bytes = request.encode();

        // Send request
        socket.send_to(&request_bytes, server).await?;

        // Wait for response
        let mut buf = vec![0u8; 1024];
        let (len, _from) = timeout(self.timeout, socket.recv_from(&mut buf)).await
            .map_err(|_| StunError::Timeout)??;

        buf.truncate(len);

        // Parse response
        let response = StunBindingResponse::parse(&buf)?;
        
        response.get_mapped_address()
            .ok_or(StunError::NoMappedAddress)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stun_binding_request_encode() {
        let request = StunBindingRequest::new();
        let encoded = request.encode();
        
        // Should be exactly 20 bytes
        assert_eq!(encoded.len(), 20);
        
        // Message type should be Binding Request
        assert_eq!(&encoded[0..2], &STUN_BINDING_REQUEST.to_be_bytes());
        
        // Message length should be 0 (no attributes)
        assert_eq!(&encoded[2..4], &0u16.to_be_bytes());
        
        // Magic cookie should be correct
        assert_eq!(&encoded[4..8], &STUN_MAGIC_COOKIE.to_be_bytes());
    }

    #[test]
    fn test_parse_xor_mapped_address_ipv4() {
        // Create a fake XOR-MAPPED-ADDRESS attribute for 192.0.2.1:32853
        let transaction_id = [0u8; 12];
        let port = 32853u16;
        let addr = Ipv4Addr::new(192, 0, 2, 1);
        
        // XOR the values
        let xor_port = port ^ (STUN_MAGIC_COOKIE >> 16) as u16;
        let xor_addr = u32::from(addr) ^ STUN_MAGIC_COOKIE;
        
        let mut attr_data = Vec::new();
        attr_data.push(0); // Reserved
        attr_data.push(0x01); // IPv4
        attr_data.extend_from_slice(&xor_port.to_be_bytes());
        attr_data.extend_from_slice(&xor_addr.to_be_bytes());
        
        let result = StunBindingResponse::parse_xor_mapped_address(&attr_data, &transaction_id).unwrap();
        assert_eq!(result.ip(), IpAddr::V4(addr));
        assert_eq!(result.port(), port);
    }
}
