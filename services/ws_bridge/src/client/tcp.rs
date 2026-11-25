use crate::client::connect_tunnel;
use crate::config::TunnelRule;
use crate::protocol::TunnelProtocol;
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};

pub async fn run_tcp_listener(
    rule: TunnelRule,
    ws_url: String,
    token: String,
) -> Result<()> {
    let listener = TcpListener::bind(&rule.listen)
        .await
        .with_context(|| format!("Failed to bind TCP listener on {}", rule.listen))?;

    info!("[TCP:{}] Listening on {}", rule.name, rule.listen);

    loop {
        let (local_stream, client_addr) = match listener.accept().await {
            Ok(v) => v,
            Err(e) => {
                warn!("[TCP:{}] Accept error: {}", rule.name, e);
                continue;
            }
        };

        let ws_url = ws_url.clone();
        let token = token.clone();
        let target = rule.target.clone();
        let name = rule.name.clone();

        tokio::spawn(async move {
            debug!("[TCP:{}] Inbound connection from {}", name, client_addr);
            if let Err(e) = handle_tcp_client(local_stream, &ws_url, &token, &target).await {
                error!("[TCP:{}] Tunnel error for {}: {:#}", name, client_addr, e);
            }
            debug!("[TCP:{}] Connection closed for {}", name, client_addr);
        });
    }
}

async fn handle_tcp_client(
    local_stream: tokio::net::TcpStream,
    ws_url: &str,
    token: &str,
    target: &str,
) -> Result<()> {
    let (mut ws_sender, mut ws_receiver) =
        connect_tunnel(ws_url, token, TunnelProtocol::Tcp, target).await?;

    let (mut tcp_reader, mut tcp_writer) = local_stream.into_split();

    // tcp -> ws
    let tcp_to_ws = async {
        let mut buf = vec![0u8; 65536];
        loop {
            match tcp_reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let data = buf[..n].to_vec();
                    if let Err(e) = ws_sender.send(Message::Binary(data)).await {
                        debug!("WS send error: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    debug!("TCP read error: {}", e);
                    break;
                }
            }
        }
        let _ = ws_sender.close().await;
    };

    // ws -> tcp
    let ws_to_tcp = async {
        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(Message::Binary(data)) => {
                    if let Err(e) = tcp_writer.write_all(&data).await {
                        debug!("TCP write error: {}", e);
                        break;
                    }
                }
                Ok(Message::Close(_)) => break,
                Ok(_) => {}
                Err(e) => {
                    debug!("WS receive error: {}", e);
                    break;
                }
            }
        }
        let _ = tcp_writer.shutdown().await;
    };

    tokio::select! {
        _ = tcp_to_ws => {},
        _ = ws_to_tcp => {},
    }

    Ok(())
}
