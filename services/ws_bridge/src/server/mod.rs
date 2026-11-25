pub mod http;
pub mod tcp;
pub mod udp;

use crate::config::ServerConfig;
use crate::protocol::{HandshakeRequest, HandshakeResponse, TunnelProtocol};
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

pub async fn run_server(config: ServerConfig) -> Result<()> {
    let addr = config.listen.clone();
    let listener = TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Failed to bind server listener on {}", addr))?;

    info!("===========================================================");
    info!("               WSB MULTI-PROTOCOL SERVER                   ");
    info!("===========================================================");
    info!("WebSocket listening on: {}", addr);
    println!();
    println!("===========================================================");
    println!(" 🔑 AUTH TOKEN (Copy this into client --token):");
    println!("    {}", config.token);
    println!("===========================================================");
    println!();
    if let Some(ref targets) = config.allowed_targets {
        info!("Allowed target whitelist: {:?}", targets);
    } else {
        info!("Allowed target whitelist: ALL (unrestricted)");
    }
    info!("Protocols supported: TCP, UDP, HTTP, SOCKS5 forward");
    info!("===========================================================");

    let config = Arc::new(config);

    loop {
        let (stream, peer_addr) = match listener.accept().await {
            Ok(val) => val,
            Err(e) => {
                warn!("Accept error: {}", e);
                continue;
            }
        };

        let config = Arc::clone(&config);
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, peer_addr, config).await {
                error!("[{}] Error handling client connection: {:#}", peer_addr, e);
            }
        });
    }
}

async fn handle_connection(
    stream: tokio::net::TcpStream,
    peer_addr: SocketAddr,
    config: Arc<ServerConfig>,
) -> Result<()> {
    let ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .context("WebSocket handshake failed")?;

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // 1. Wait for handshake request
    let first_msg = match ws_receiver.next().await {
        Some(Ok(msg)) => msg,
        Some(Err(e)) => anyhow::bail!("Failed to read initial handshake message: {}", e),
        None => anyhow::bail!("Client disconnected before handshake"),
    };

    let handshake_text = match first_msg {
        Message::Text(t) => t,
        Message::Binary(b) => String::from_utf8(b).context("Invalid UTF-8 in handshake binary")?,
        _ => anyhow::bail!("Unexpected handshake message type"),
    };

    let req: HandshakeRequest = serde_json::from_str(&handshake_text)
        .context("Failed to parse HandshakeRequest JSON")?;

    // 2. Authenticate
    if req.token != config.token {
        warn!("[{}] Unauthorized connection attempt with invalid token", peer_addr);
        let resp = HandshakeResponse {
            success: false,
            message: "Unauthorized: invalid secret token".to_string(),
        };
        let _ = ws_sender.send(Message::Text(serde_json::to_string(&resp)?)).await;
        let _ = ws_sender.close().await;
        return Ok(());
    }

    // 3. Check allowed targets if specified
    if let Some(ref allowed) = config.allowed_targets {
        if !allowed.iter().any(|target| target == &req.target) {
            warn!(
                "[{}] Target '{}' is not in allowed target whitelist",
                peer_addr, req.target
            );
            let resp = HandshakeResponse {
                success: false,
                message: format!("Forbidden: target '{}' not permitted", req.target),
            };
            let _ = ws_sender.send(Message::Text(serde_json::to_string(&resp)?)).await;
            let _ = ws_sender.close().await;
            return Ok(());
        }
    }

    // Handshake successful
    let resp = HandshakeResponse {
        success: true,
        message: format!("Connected tunnel for {} to {}", req.protocol, req.target),
    };
    ws_sender
        .send(Message::Text(serde_json::to_string(&resp)?))
        .await
        .context("Failed to send handshake response")?;

    info!(
        "[+] [{}] Tunnel active: [{}] -> target [{}]",
        peer_addr, req.protocol, req.target
    );

    match req.protocol {
        TunnelProtocol::Tcp => {
            tcp::handle_tcp(ws_sender, ws_receiver, &req.target).await?;
        }
        TunnelProtocol::Udp => {
            udp::handle_udp(ws_sender, ws_receiver, &req.target).await?;
        }
        TunnelProtocol::Http => {
            http::handle_http(ws_sender, ws_receiver, &req.target).await?;
        }
    }

    info!(
        "[-] [{}] Tunnel closed: [{}] -> target [{}]",
        peer_addr, req.protocol, req.target
    );
    Ok(())
}
