pub mod http;
pub mod socks5;
pub mod tcp;
pub mod udp;

use crate::config::ClientConfig;
use crate::protocol::{HandshakeRequest, HandshakeResponse, TunnelProtocol};
use anyhow::{Context, Result};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tracing::{error, info, warn};
use url::Url;

pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Establishes a WebSocket connection to the server and completes handshake authentication
pub async fn connect_tunnel(
    ws_url: &str,
    token: &str,
    protocol: TunnelProtocol,
    target: &str,
) -> Result<(SplitSink<WsStream, Message>, SplitStream<WsStream>)> {
    let _ = Url::parse(ws_url)
        .with_context(|| format!("Invalid remote WebSocket URL: '{}'", ws_url))?;

    let (ws_stream, _) = tokio_tungstenite::connect_async(ws_url)
        .await
        .with_context(|| format!("Failed to connect to WebSocket server at {}", ws_url))?;

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Send handshake request
    let req = HandshakeRequest {
        token: token.to_string(),
        protocol,
        target: target.to_string(),
    };
    let json_req = serde_json::to_string(&req)?;
    ws_sender
        .send(Message::Text(json_req))
        .await
        .context("Failed to send handshake request")?;

    // Wait for handshake response
    let resp_msg = match ws_receiver.next().await {
        Some(Ok(msg)) => msg,
        Some(Err(e)) => anyhow::bail!("Error receiving handshake response: {}", e),
        None => anyhow::bail!("WebSocket closed by server during handshake"),
    };

    let resp_text = match resp_msg {
        Message::Text(t) => t,
        Message::Binary(b) => String::from_utf8(b).context("Invalid UTF-8 in handshake response")?,
        _ => anyhow::bail!("Unexpected handshake response message type"),
    };

    let resp: HandshakeResponse = serde_json::from_str(&resp_text)
        .context("Failed to parse HandshakeResponse JSON from server")?;

    if !resp.success {
        anyhow::bail!("Handshake rejected by server: {}", resp.message);
    }

    Ok((ws_sender, ws_receiver))
}

pub async fn run_client(config: ClientConfig) -> Result<()> {
    info!("===============================================");
    info!("            WSB MULTI-PROTOCOL CLIENT          ");
    info!("===============================================");
    info!("Remote WebSocket URL: {}", config.ws_url);
    info!(
        "Auth token: {}",
        if config.token == "12345" {
            "DEFAULT (12345)"
        } else {
            "[CONFIGURED]"
        }
    );

    let mut handles = Vec::new();

    // Start configured tunnels
    for tunnel in config.tunnels {
        let ws_url = config.ws_url.clone();
        let token = config.token.clone();
        let rule = tunnel.clone();

        info!(
            "-> Tunnel [{}]: protocol=[{}] listen=[{}] -> remote target=[{}]",
            rule.name, rule.protocol, rule.listen, rule.target
        );

        let handle = tokio::spawn(async move {
            let res = match rule.protocol {
                TunnelProtocol::Tcp => tcp::run_tcp_listener(rule, ws_url, token).await,
                TunnelProtocol::Udp => udp::run_udp_listener(rule, ws_url, token).await,
                TunnelProtocol::Http => http::run_http_listener(rule, ws_url, token).await,
            };
            if let Err(e) = res {
                error!("Tunnel listener terminated with error: {:#}", e);
            }
        });
        handles.push(handle);
    }

    // Start SOCKS5 proxy if specified
    if let Some(socks5_addr) = config.socks5 {
        info!("-> SOCKS5 Dynamic Proxy listening on: {}", socks5_addr);
        let ws_url = config.ws_url.clone();
        let token = config.token.clone();
        let handle = tokio::spawn(async move {
            if let Err(e) = socks5::run_socks5_listener(socks5_addr, ws_url, token).await {
                error!("SOCKS5 listener terminated with error: {:#}", e);
            }
        });
        handles.push(handle);
    }

    if handles.is_empty() {
        warn!("No tunnels or SOCKS5 proxy configured! Client has nothing to listen on.");
        return Ok(());
    }

    info!("===============================================");
    info!("[READY] All client listeners are active. Press Ctrl+C to stop.");

    // Wait for all listeners
    futures_util::future::join_all(handles).await;
    Ok(())
}
