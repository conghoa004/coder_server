use crate::bridge::bridge_tcp_ws;
use crate::client::connect_tunnel;
use crate::config::TunnelRule;
use crate::protocol::TunnelProtocol;
use anyhow::{Context, Result};
use tokio::net::TcpListener;
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
    let (ws_sender, ws_receiver) =
        connect_tunnel(ws_url, token, TunnelProtocol::Tcp, target).await?;

    bridge_tcp_ws(local_stream, ws_sender, ws_receiver).await
}
