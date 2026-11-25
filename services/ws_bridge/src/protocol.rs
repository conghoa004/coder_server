use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TunnelProtocol {
    Tcp,
    Udp,
    Http,
}

impl std::fmt::Display for TunnelProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tcp => write!(f, "tcp"),
            Self::Udp => write!(f, "udp"),
            Self::Http => write!(f, "http"),
        }
    }
}

impl std::str::FromStr for TunnelProtocol {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "tcp" => Ok(Self::Tcp),
            "udp" => Ok(Self::Udp),
            "http" => Ok(Self::Http),
            other => Err(format!(
                "Unknown protocol: '{}'. Supported protocols: tcp, udp, http",
                other
            )),
        }
    }
}

/// Initial handshake payload sent from client to server over WebSocket
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeRequest {
    pub token: String,
    pub protocol: TunnelProtocol,
    pub target: String, // e.g. "127.0.0.1:3306" or "127.0.0.1:80" or "8.8.8.8:53"
}

/// Handshake response returned by server to client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeResponse {
    pub success: bool,
    pub message: String,
}
