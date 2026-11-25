use crate::client::connect_tunnel;
use crate::config::TunnelRule;
use crate::protocol::TunnelProtocol;
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};

type SessionMap = Arc<Mutex<HashMap<SocketAddr, mpsc::Sender<Vec<u8>>>>>;

pub async fn run_udp_listener(
    rule: TunnelRule,
    ws_url: String,
    token: String,
) -> Result<()> {
    let socket = Arc::new(
        UdpSocket::bind(&rule.listen)
            .await
            .with_context(|| format!("Failed to bind UDP listener on {}", rule.listen))?,
    );

    info!("[UDP:{}] Listening on {}", rule.name, rule.listen);

    let sessions: SessionMap = Arc::new(Mutex::new(HashMap::new()));
    let mut buf = vec![0u8; 65536];

    loop {
        let (len, peer_addr) = match socket.recv_from(&mut buf).await {
            Ok(val) => val,
            Err(e) => {
                warn!("[UDP:{}] Recv error: {}", rule.name, e);
                continue;
            }
        };

        let packet = buf[..len].to_vec();

        let mut map = sessions.lock().await;
        if let Some(sender) = map.get(&peer_addr) {
            let _ = sender.send(packet).await;
        } else {
            let (tx, rx) = mpsc::channel(100);
            let _ = tx.send(packet).await;
            map.insert(peer_addr, tx);

            let sessions_clone = Arc::clone(&sessions);
            let socket_clone = Arc::clone(&socket);
            let ws_url = ws_url.clone();
            let token = token.clone();
            let target = rule.target.clone();
            let name = rule.name.clone();

            tokio::spawn(async move {
                debug!("[UDP:{}] Starting new UDP session for peer {}", name, peer_addr);
                if let Err(e) = handle_udp_session(
                    rx,
                    peer_addr,
                    socket_clone,
                    &ws_url,
                    &token,
                    &target,
                )
                .await
                {
                    error!("[UDP:{}] Session error for {}: {:#}", name, peer_addr, e);
                }
                // Cleanup session from map
                let mut m = sessions_clone.lock().await;
                m.remove(&peer_addr);
                debug!("[UDP:{}] Closed UDP session for peer {}", name, peer_addr);
            });
        }
    }
}

async fn handle_udp_session(
    mut rx: mpsc::Receiver<Vec<u8>>,
    peer_addr: SocketAddr,
    socket: Arc<UdpSocket>,
    ws_url: &str,
    token: &str,
    target: &str,
) -> Result<()> {
    let (mut ws_sender, mut ws_receiver) =
        connect_tunnel(ws_url, token, TunnelProtocol::Udp, target).await?;

    let idle_timeout = Duration::from_secs(60);

    // Forward local UDP packets -> WebSocket
    let client_to_ws = async {
        loop {
            match tokio::time::timeout(idle_timeout, rx.recv()).await {
                Ok(Some(data)) => {
                    if let Err(e) = ws_sender.send(Message::Binary(data)).await {
                        debug!("WS send error: {}", e);
                        break;
                    }
                }
                Ok(None) => break,
                Err(_) => {
                    debug!("UDP session idle timeout");
                    break;
                }
            }
        }
        let _ = ws_sender.close().await;
    };

    // Forward WebSocket responses -> local UDP socket
    let ws_to_client = async {
        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(Message::Binary(data)) => {
                    if let Err(e) = socket.send_to(&data, peer_addr).await {
                        debug!("UDP send_to error: {}", e);
                        break;
                    }
                }
                Ok(Message::Close(_)) => break,
                Ok(_) => {}
                Err(e) => {
                    debug!("WS receive error in UDP tunnel: {}", e);
                    break;
                }
            }
        }
    };

    tokio::select! {
        _ = client_to_ws => {},
        _ = ws_to_client => {},
    }

    Ok(())
}
