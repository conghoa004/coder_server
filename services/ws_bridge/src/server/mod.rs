pub mod http;
pub mod tcp;
pub mod udp;

use crate::config::ServerConfig;
use crate::protocol::{HandshakeRequest, HandshakeResponse, TunnelProtocol};
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

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
    let handshake_timeout = Duration::from_secs(15);

    let ws_stream = tokio::time::timeout(handshake_timeout, tokio_tungstenite::accept_async(stream))
        .await
        .context("WebSocket upgrade handshake timed out")?
        .context("WebSocket handshake failed")?;

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // 1. Wait for handshake request with timeout (ignoring Ping/Pong control frames)
    let first_msg = tokio::time::timeout(handshake_timeout, async {
        loop {
            match ws_receiver.next().await {
                Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => continue,
                Some(Ok(msg)) => return Ok(msg),
                Some(Err(e)) => anyhow::bail!("Failed to read initial handshake message: {}", e),
                None => anyhow::bail!("Client disconnected before handshake"),
            }
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("Timed out waiting for client handshake payload"))??;

    let handshake_text = match first_msg {
        Message::Text(t) => t,
        Message::Binary(b) => String::from_utf8(b).context("Invalid UTF-8 in handshake binary")?,
        _ => anyhow::bail!("Unexpected handshake message type"),
    };

    let req: HandshakeRequest = serde_json::from_str(&handshake_text)
        .context("Failed to parse HandshakeRequest JSON")?;

    // 2. Authenticate
    if !constant_time_eq(req.token.as_bytes(), config.token.as_bytes()) {
        warn!("[{}] Unauthorized connection attempt with invalid token", peer_addr);
        let resp = HandshakeResponse {
            success: false,
            message: "Unauthorized: invalid secret token".to_string(),
        };
        let _ = ws_sender
            .send(Message::Text(serde_json::to_string(&resp)?))
            .await;
        let _ = ws_sender.close().await;
        return Ok(());
    }

    let target = req.target.trim().to_string();

    // 3. Check allowed targets if specified
    if let Some(ref allowed) = config.allowed_targets {
        if !allowed
            .iter()
            .any(|allowed_target| allowed_target.trim().eq_ignore_ascii_case(&target))
        {
            warn!(
                "[{}] Target '{}' is not in allowed target whitelist",
                peer_addr, target
            );
            let resp = HandshakeResponse {
                success: false,
                message: format!("Forbidden: target '{}' not permitted", target),
            };
            let _ = ws_sender
                .send(Message::Text(serde_json::to_string(&resp)?))
                .await;
            let _ = ws_sender.close().await;
            return Ok(());
        }
    }

    // 4. Connect to the target BEFORE confirming success to the client
    enum ConnectedTarget {
        Tcp(tokio::net::TcpStream),
        Udp(tokio::net::UdpSocket),
        Http(tokio::net::TcpStream),
    }

    let connected = match req.protocol {
        TunnelProtocol::Tcp => tcp::connect_tcp_target(&target).await.map(ConnectedTarget::Tcp),
        TunnelProtocol::Udp => udp::connect_udp_target(&target).await.map(ConnectedTarget::Udp),
        TunnelProtocol::Http => http::connect_http_target(&target).await.map(ConnectedTarget::Http),
    };

    let connected = match connected {
        Ok(c) => c,
        Err(e) => {
            let resp = HandshakeResponse {
                success: false,
                message: format!("Failed to connect to target '{}': {:#}", target, e),
            };
            let _ = ws_sender
                .send(Message::Text(serde_json::to_string(&resp)?))
                .await;
            let _ = ws_sender.close().await;
            return Err(e);
        }
    };

    // 5. Handshake successful
    let resp = HandshakeResponse {
        success: true,
        message: format!("Connected tunnel for {} to {}", req.protocol, target),
    };
    ws_sender
        .send(Message::Text(serde_json::to_string(&resp)?))
        .await
        .context("Failed to send handshake response")?;

    info!(
        "[+] [{}] Tunnel active: [{}] -> target [{}]",
        peer_addr, req.protocol, target
    );

    match connected {
        ConnectedTarget::Tcp(target_stream) => {
            tcp::handle_tcp_connected(target_stream, ws_sender, ws_receiver).await?;
        }
        ConnectedTarget::Udp(udp_socket) => {
            udp::handle_udp_connected(ws_sender, ws_receiver, udp_socket).await?;
        }
        ConnectedTarget::Http(target_stream) => {
            http::handle_http_connected(target_stream, ws_sender, ws_receiver).await?;
        }
    }

    info!(
        "[-] [{}] Tunnel closed: [{}] -> target [{}]",
        peer_addr, req.protocol, target
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::connect_tunnel;

    #[tokio::test]
    async fn test_server_rejects_handshake_when_target_unreachable() {
        // Bind and immediately drop a TCP listener to obtain an unused closed port
        let closed_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let closed_addr = closed_listener.local_addr().unwrap();
        drop(closed_listener);

        let server_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap();
        let config = Arc::new(ServerConfig {
            listen: server_addr.to_string(),
            token: "secret-tok".to_string(),
            allowed_targets: Some(vec![closed_addr.to_string()]),
        });

        let server_task = tokio::spawn(async move {
            let (stream, peer_addr) = server_listener.accept().await.unwrap();
            let _ = handle_connection(stream, peer_addr, config).await;
        });

        let ws_url = format!("ws://{}", server_addr);
        let res = connect_tunnel(
            &ws_url,
            "secret-tok",
            TunnelProtocol::Tcp,
            &closed_addr.to_string(),
        )
        .await;

        assert!(
            res.is_err(),
            "Expected handshake to fail when target port is closed"
        );
        let err_str = format!("{:#}", res.unwrap_err());
        assert!(
            err_str.contains("Handshake rejected by server"),
            "Unexpected error: {}",
            err_str
        );

        server_task.await.unwrap();
    }

    #[tokio::test]
    async fn test_server_rejects_unauthorized_and_disallowed_target() {
        let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target_addr = target_listener.local_addr().unwrap();

        let server_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server_listener.local_addr().unwrap();
        let config = Arc::new(ServerConfig {
            listen: server_addr.to_string(),
            token: "secret-tok".to_string(),
            allowed_targets: Some(vec![target_addr.to_string()]),
        });

        let server_task = tokio::spawn(async move {
            for _ in 0..2 {
                let (stream, peer_addr) = server_listener.accept().await.unwrap();
                let cfg = Arc::clone(&config);
                let _ = handle_connection(stream, peer_addr, cfg).await;
            }
        });

        let ws_url = format!("ws://{}", server_addr);

        // 1. Wrong token
        let err1 = connect_tunnel(
            &ws_url,
            "wrong-tok",
            TunnelProtocol::Tcp,
            &target_addr.to_string(),
        )
        .await
        .unwrap_err();
        assert!(format!("{:#}", err1).contains("Unauthorized"));

        // 2. Disallowed target
        let err2 = connect_tunnel(&ws_url, "secret-tok", TunnelProtocol::Tcp, "127.0.0.1:9999")
            .await
            .unwrap_err();
        assert!(format!("{:#}", err2).contains("Forbidden"));

        server_task.await.unwrap();
    }
}
