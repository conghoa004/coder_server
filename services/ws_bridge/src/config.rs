use crate::protocol::TunnelProtocol;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    #[serde(default = "default_server_listen")]
    pub listen: String,
    #[serde(default = "default_token")]
    pub token: String,
    pub allowed_targets: Option<Vec<String>>,
}

fn default_server_listen() -> String {
    "0.0.0.0:8766".to_string()
}

fn default_token() -> String {
    "auto".to_string()
}

/// Generates a cryptographically secure 256-bit (64 hex characters) random token
pub fn generate_secure_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen: default_server_listen(),
            token: default_token(),
            allowed_targets: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClientConfig {
    pub ws_url: String,
    #[serde(default = "default_token")]
    pub token: String,
    #[serde(default)]
    pub tunnels: Vec<TunnelRule>,
    pub socks5: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TunnelRule {
    pub name: String,
    pub protocol: TunnelProtocol,
    pub listen: String,
    pub target: String,
}

impl ServerConfig {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read server config file {:?}", path.as_ref()))?;
        toml::from_str(&content).context("Failed to parse server config toml")
    }
}

impl ClientConfig {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read client config file {:?}", path.as_ref()))?;
        toml::from_str(&content).context("Failed to parse client config toml")
    }
}
