pub mod http;
pub mod socks5;
pub mod tcp;
pub mod udp;

use crate::config::ClientConfig;
use crate::protocol::{HandshakeRequest, HandshakeResponse, TunnelProtocol};
use anyhow::{Context, Result};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tracing::{debug, error, info, warn};
use url::Url;

pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Connects to the remote WebSocket server, prioritizing IPv4 addresses first
/// and falling back across all resolved addresses if TCP/TLS/WS handshake fails.
async fn connect_ws_with_fallback(ws_url: &str) -> Result<WsStream> {
    let url = Url::parse(ws_url)
        .with_context(|| format!("Invalid remote WebSocket URL: '{}'", ws_url))?;

    let host = url
        .host_str()
        .with_context(|| format!("Missing host in WebSocket URL: '{}'", ws_url))?;
    let port = url
        .port_or_known_default()
        .with_context(|| format!("Missing or unknown port in WebSocket URL: '{}'", ws_url))?;

    let host_port = format!("{host}:{port}");
    let mut addrs: Vec<SocketAddr> = tokio::net::lookup_host(&host_port)
        .await
        .with_context(|| format!("Failed to resolve DNS for {}", host_port))?
        .collect();

    if addrs.is_empty() {
        anyhow::bail!("No addresses resolved for {}", host_port);
    }

    // Prioritize IPv4 addresses before IPv6 (avoids ISP IPv6 TLS resets like Viettel -> Cloudflare)
    addrs.sort_by_key(|addr| !addr.is_ipv4());

    let mut last_err: Option<anyhow::Error> = None;
    for addr in addrs {
        debug!("Trying WebSocket connection via {}", addr);
        let attempt = tokio::time::timeout(Duration::from_secs(10), async {
            let socket = TcpStream::connect(addr)
                .await
                .with_context(|| format!("TCP connect failed to {}", addr))?;
            let _ = socket.set_nodelay(true);
            let (ws_stream, _) = tokio_tungstenite::client_async_tls(ws_url, socket)
                .await
                .with_context(|| format!("WebSocket/TLS handshake failed via {}", addr))?;
            Ok::<WsStream, anyhow::Error>(ws_stream)
        })
        .await;

        match attempt {
            Ok(Ok(ws_stream)) => {
                debug!("Connected to WebSocket server via {}", addr);
                return Ok(ws_stream);
            }
            Ok(Err(err)) => {
                debug!("Attempt via {} failed: {:#}", addr, err);
                last_err = Some(err);
            }
            Err(_) => {
                debug!("Attempt via {} timed out", addr);
                last_err = Some(anyhow::anyhow!("Connection timed out via {}", addr));
            }
        }
    }

    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("Failed to connect to {}", ws_url)))
        .with_context(|| format!("Failed to connect to WebSocket server at {}", ws_url))
}

/// Establishes a WebSocket connection to the server and completes handshake authentication
pub async fn connect_tunnel(
    ws_url: &str,
    token: &str,
    protocol: TunnelProtocol,
    target: &str,
) -> Result<(SplitSink<WsStream, Message>, SplitStream<WsStream>)> {
    let ws_stream = connect_ws_with_fallback(ws_url).await?;

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
