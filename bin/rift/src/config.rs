use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

// Example config (~/.config/rift/config.toml):
//
// [user]
// name = "alice"
//
// [audio]
// input_device = "Built-in Microphone"
// output_device = "Built-in Output"
// quality = "medium"
// ptt = true
// ptt_key = "f1" # f1..f12, space, ctrl_space, alt_space, ctrl_backtick, ctrl_semicolon
// vad = true
// mute_output = false
//
// [network]
// prefer_p2p = true
// relay = false
// local_ports = [7777, 7778, 7779]
//
// [ui]
// theme = "dark"
//
// [dht]
// enabled = false
// bootstrap_nodes = ["1.2.3.4:4001"]
//
// [qos]
// target_latency_ms = 50
// max_latency_ms = 200
// min_bitrate = 16000
// max_bitrate = 96000
// packet_loss_tolerance = 0.08
//
// [logging]
// level = "info"
// target = "stderr" # stderr | file | file:/path/to/rift.log
//
// [metrics]
// enabled = true
//
// [security]
// trust_on_first_use = true
// known_hosts_path = "~/.config/rift/known_hosts"
// reject_on_mismatch = false
// channel_shared_secret = ""
// audit_log_path = "~/.config/rift/audit.log"

#[derive(Debug, Clone, Deserialize, Default)]
pub struct UserConfig {
    #[serde(default)]
    pub user: UserSection,
    #[serde(default)]
    pub audio: AudioSection,
    #[serde(default)]
    pub network: NetworkSection,
    #[serde(default)]
    pub dht: DhtSection,
    #[serde(default)]
    pub qos: QosSection,
    #[serde(default)]
    pub logging: LoggingSection,
    #[serde(default)]
    pub metrics: MetricsSection,
    #[serde(default)]
    pub security: SecuritySection,
    #[serde(default)]
    pub ui: UiSection,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct UserSection {
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AudioSection {
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub quality: Option<String>,
    pub ptt: Option<bool>,
    pub ptt_key: Option<String>,
    pub vad: Option<bool>,
    pub mute_output: Option<bool>,
}

impl Default for AudioSection {
    fn default() -> Self {
        Self {
            input_device: None,
            output_device: None,
            quality: Some("medium".to_string()),
            ptt: Some(false),
            ptt_key: Some("f1".to_string()),
            vad: Some(true),
            mute_output: Some(false),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct NetworkSection {
    pub prefer_p2p: Option<bool>,
    pub relay: Option<bool>,
    pub local_ports: Option<Vec<u16>>,
}

impl Default for NetworkSection {
    fn default() -> Self {
        Self {
            prefer_p2p: Some(true),
            relay: Some(false),
            local_ports: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct UiSection {
    pub theme: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QosSection {
    pub target_latency_ms: Option<u32>,
    pub max_latency_ms: Option<u32>,
    pub min_bitrate: Option<u32>,
    pub max_bitrate: Option<u32>,
    pub packet_loss_tolerance: Option<f32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingSection {
    pub level: Option<String>,
    pub target: Option<String>,
}

impl Default for LoggingSection {
    fn default() -> Self {
        Self {
            level: Some("info".to_string()),
            target: Some("stderr".to_string()),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct MetricsSection {
    pub enabled: Option<bool>,
}

impl Default for MetricsSection {
    fn default() -> Self {
        Self {
            enabled: Some(true),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SecuritySection {
    pub trust_on_first_use: Option<bool>,
    pub known_hosts_path: Option<String>,
    pub reject_on_mismatch: Option<bool>,
    pub channel_shared_secret: Option<String>,
    pub audit_log_path: Option<String>,
}

impl Default for SecuritySection {
    fn default() -> Self {
        Self {
            trust_on_first_use: Some(true),
            known_hosts_path: None,
            reject_on_mismatch: Some(false),
            channel_shared_secret: None,
            audit_log_path: None,
        }
    }
}

impl Default for QosSection {
    fn default() -> Self {
        Self {
            target_latency_ms: Some(50),
            max_latency_ms: Some(200),
            min_bitrate: Some(16_000),
            max_bitrate: Some(96_000),
            packet_loss_tolerance: Some(0.08),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DhtSection {
    pub enabled: Option<bool>,
    pub bootstrap_nodes: Option<Vec<String>>,
}

impl Default for DhtSection {
    fn default() -> Self {
        Self {
            enabled: Some(false),
            bootstrap_nodes: None,
        }
    }
}

impl Default for UiSection {
    fn default() -> Self {
        Self {
            theme: Some("dark".to_string()),
        }
    }
}

impl QosSection {
    pub fn to_profile(&self) -> rift_sdk::RiftQosProfile {
        rift_sdk::RiftQosProfile {
            target_latency_ms: self.target_latency_ms.unwrap_or(50),
            max_latency_ms: self.max_latency_ms.unwrap_or(200),
            min_bitrate: self.min_bitrate.unwrap_or(16_000),
            max_bitrate: self.max_bitrate.unwrap_or(96_000),
            packet_loss_tolerance: self.packet_loss_tolerance.unwrap_or(0.08),
        }
    }
}

impl UserConfig {
    pub fn load() -> Result<Self> {
        let path = config_path()?;
        let content = match fs::read_to_string(&path) {
            Ok(data) => data,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok(UserConfig::default());
            }
            Err(err) => return Err(err.into()),
        };
        let cfg: UserConfig = toml::from_str(&content)
            .with_context(|| format!("failed to parse config at {}", path.display()))?;
        Ok(cfg)
    }
}

pub fn config_path() -> Result<PathBuf> {
    let base = dirs::config_dir().context("config directory not found")?;
    Ok(base.join("rift").join("config.toml"))
}
