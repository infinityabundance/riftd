//! Predictive Rendezvous (PR) core types.

pub mod identity;
pub mod schedule;
pub mod strategy;
pub mod time;

pub use identity::IdentityConstraints;
pub use schedule::{compute_slot_params, Role, SlotParams};
pub use strategy::{EscalationPolicy, SearchStrategy};
pub use time::TimeModel;

use std::collections::HashMap;
use std::fmt;

use base64::Engine;

/// Semantic Rendezvous Token (SRT) describes the deterministic rendezvous schedule
/// and the constraints used to locate peers.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SemanticRendezvousToken {
    /// 32-byte seed that deterministically drives schedule generation.
    pub seed: [u8; 32],
    /// Identity constraints that limit acceptable peers.
    pub identities: IdentityConstraints,
    /// Time model used for schedule slotting and windows.
    pub time_model: TimeModel,
    /// Search strategy used to traverse the deterministic schedule.
    pub search_strategy: SearchStrategy,
    /// Escalation policy describing how the search evolves over time.
    pub escalation: EscalationPolicy,
}

impl SemanticRendezvousToken {
    /// Construct a new SRT from its component parts.
    pub fn new(
        seed: [u8; 32],
        identities: IdentityConstraints,
        time_model: TimeModel,
        search_strategy: SearchStrategy,
        escalation: EscalationPolicy,
    ) -> Self {
        Self {
            seed,
            identities,
            time_model,
            search_strategy,
            escalation,
        }
    }

    /// Encode this SRT as a compact, machine-oriented URI.
    ///
    /// Example:
    /// `riftd-srt://v1?seed=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA&t0=1700000000&tw=120&slot=250&ss=basic&esc=none`
    pub fn to_uri(&self) -> Result<String, SrtError> {
        let seed = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(self.seed);

        let ids = if self.identities.allowed_fingerprints.is_empty() {
            None
        } else {
            let mut parts = Vec::with_capacity(self.identities.allowed_fingerprints.len());
            for fp in &self.identities.allowed_fingerprints {
                parts.push(hex::encode(fp));
            }
            Some(parts.join("."))
        };

        let ss = match self.search_strategy {
            SearchStrategy::BasicDeterministic => "basic",
            SearchStrategy::Extended => "extended",
        };
        let esc = match self.escalation {
            EscalationPolicy::None => "none",
            EscalationPolicy::Simple => "simple",
            EscalationPolicy::Aggressive => "aggressive",
        };

        let mut uri = format!(
            "riftd-srt://v1?seed={seed}&t0={}&tw={}&slot={}&ss={ss}&esc={esc}",
            self.time_model.t0, self.time_model.window_secs, self.time_model.slot_ms
        );
        if let Some(ids) = ids {
            uri.push_str("&ids=");
            uri.push_str(&ids);
        }
        Ok(uri)
    }

    /// Decode an SRT from a URI string.
    pub fn from_uri(uri: &str) -> Result<Self, SrtError> {
        let (scheme, rest) = uri
            .split_once("://")
            .ok_or(SrtError::InvalidScheme)?;
        if scheme != "riftd-srt" {
            return Err(SrtError::InvalidScheme);
        }

        let (version, query) = rest.split_once('?').ok_or(SrtError::MissingField("query"))?;
        if version != "v1" {
            return Err(SrtError::InvalidVersion(version.to_string()));
        }

        let params = parse_query(query)?;

        let seed = decode_seed(params.get("seed"))?;
        let t0 = parse_u64(params.get("t0"), "t0")?;
        let window_secs = parse_u64(params.get("tw"), "tw")?;
        let slot_ms = parse_u64(params.get("slot"), "slot")?;
        let search_strategy = parse_strategy(params.get("ss"))?;
        let escalation = parse_escalation(params.get("esc"))?;
        let identities = IdentityConstraints {
            allowed_fingerprints: decode_ids(params.get("ids"))?,
        };

        Ok(Self {
            seed,
            identities,
            time_model: TimeModel {
                t0,
                window_secs,
                slot_ms,
            },
            search_strategy,
            escalation,
        })
    }
}

/// Errors produced while encoding or decoding SRT URIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SrtError {
    InvalidScheme,
    InvalidVersion(String),
    MissingField(&'static str),
    InvalidField(&'static str),
    DecodeError(&'static str),
}

impl fmt::Display for SrtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SrtError::InvalidScheme => write!(f, "invalid SRT scheme"),
            SrtError::InvalidVersion(v) => write!(f, "unsupported SRT version: {v}"),
            SrtError::MissingField(field) => write!(f, "missing field: {field}"),
            SrtError::InvalidField(field) => write!(f, "invalid field: {field}"),
            SrtError::DecodeError(field) => write!(f, "failed to decode field: {field}"),
        }
    }
}

impl std::error::Error for SrtError {}

fn parse_query(query: &str) -> Result<HashMap<&str, &str>, SrtError> {
    if query.is_empty() {
        return Err(SrtError::MissingField("query"));
    }
    let mut params = HashMap::new();
    for pair in query.split('&') {
        let (key, value) = pair
            .split_once('=')
            .ok_or(SrtError::InvalidField("query"))?;
        params.insert(key, value);
    }
    Ok(params)
}

fn decode_seed(value: Option<&&str>) -> Result<[u8; 32], SrtError> {
    let value = value.ok_or(SrtError::MissingField("seed"))?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| SrtError::DecodeError("seed"))?;
    if bytes.len() != 32 {
        return Err(SrtError::InvalidField("seed"));
    }
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&bytes);
    Ok(seed)
}

fn decode_ids(value: Option<&&str>) -> Result<Vec<[u8; 32]>, SrtError> {
    let value = match value {
        Some(v) if !v.is_empty() => *v,
        _ => return Ok(Vec::new()),
    };
    let mut out = Vec::new();
    for part in value.split('.') {
        let bytes = hex::decode(part).map_err(|_| SrtError::DecodeError("ids"))?;
        if bytes.len() != 32 {
            return Err(SrtError::InvalidField("ids"));
        }
        let mut fp = [0u8; 32];
        fp.copy_from_slice(&bytes);
        out.push(fp);
    }
    Ok(out)
}

fn parse_u64(value: Option<&&str>, field: &'static str) -> Result<u64, SrtError> {
    let value = value.ok_or(SrtError::MissingField(field))?;
    value.parse::<u64>().map_err(|_| SrtError::InvalidField(field))
}

fn parse_strategy(value: Option<&&str>) -> Result<SearchStrategy, SrtError> {
    match value.ok_or(SrtError::MissingField("ss"))? {
        &"basic" => Ok(SearchStrategy::BasicDeterministic),
        &"extended" => Ok(SearchStrategy::Extended),
        _ => Err(SrtError::InvalidField("ss")),
    }
}

fn parse_escalation(value: Option<&&str>) -> Result<EscalationPolicy, SrtError> {
    match value.ok_or(SrtError::MissingField("esc"))? {
        &"none" => Ok(EscalationPolicy::None),
        &"simple" => Ok(EscalationPolicy::Simple),
        &"aggressive" => Ok(EscalationPolicy::Aggressive),
        _ => Err(SrtError::InvalidField("esc")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructs_srt() {
        let seed = [7u8; 32];
        let identities = IdentityConstraints {
            allowed_fingerprints: vec![[1u8; 32], [2u8; 32]],
        };
        let time_model = TimeModel {
            t0: 1_700_000_000,
            window_secs: 120,
            slot_ms: 250,
        };
        let search_strategy = SearchStrategy::BasicDeterministic;
        let escalation = EscalationPolicy::None;

        let token = SemanticRendezvousToken::new(
            seed,
            identities.clone(),
            time_model.clone(),
            search_strategy,
            escalation,
        );

        assert_eq!(token.seed, seed);
        assert_eq!(token.identities, identities);
        assert_eq!(token.time_model, time_model);
        assert_eq!(token.search_strategy, search_strategy);
        assert_eq!(token.escalation, escalation);
    }

    #[test]
    fn round_trip_uri() {
        let token = SemanticRendezvousToken::new(
            [9u8; 32],
            IdentityConstraints {
                allowed_fingerprints: vec![[3u8; 32], [4u8; 32]],
            },
            TimeModel {
                t0: 1_700_000_123,
                window_secs: 300,
                slot_ms: 500,
            },
            SearchStrategy::Extended,
            EscalationPolicy::Simple,
        );

        let uri = token.to_uri().expect("encode uri");
        let decoded = SemanticRendezvousToken::from_uri(&uri).expect("decode uri");

        assert_eq!(token, decoded);
    }

    #[test]
    fn rejects_invalid_scheme() {
        let err = SemanticRendezvousToken::from_uri("http://v1?seed=abcd")
            .expect_err("invalid scheme");
        assert_eq!(err, SrtError::InvalidScheme);
    }

    #[test]
    fn rejects_invalid_version() {
        let err = SemanticRendezvousToken::from_uri("riftd-srt://v2?seed=abcd")
            .expect_err("invalid version");
        assert!(matches!(err, SrtError::InvalidVersion(_)));
    }
}
