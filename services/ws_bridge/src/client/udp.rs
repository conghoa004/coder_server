use crate::bridge::WS_PING_INTERVAL;
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

const UDP_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

struct SessionEntry {
    id: u64,
    sender: mpsc::Sender<Vec<u8>>,
}

type SessionMap = Arc<Mutex<HashMap<SocketAddr, SessionEntry>>>;

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
    let mut next_session_id: u64 = 0;
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

        // Keep lock short and never await while holding it
        let pending_packet = {
            let mut map = sessions.lock().await;
            if let Some(entry) = map.get(&peer_addr) {
                match entry.sender.try_send(packet) {
                    Ok(()) => None,
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        debug!(
                            "[UDP:{}] Session queue full for {}, dropping packet",
                            rule.name, peer_addr
                        );
                        None
                    }
                    Err(mpsc::error::TrySendError::Closed(returned_packet)) => {
                        map.remove(&peer_addr);
                        Some(returned_packet)
                    }
                }
            } else {
                Some(packet)
            }
        };

        if let Some(packet) = pending_packet {
            next_session_id = next_session_id.wrapping_add(1);
            let session_id = next_session_id;
            let (tx, rx) = mpsc::channel(100);
            let _ = tx.try_send(packet);
            {
                let mut map = sessions.lock().await;
                map.insert(
                    peer_addr,
                    SessionEntry {
                        id: session_id,
                        sender: tx,
                    },
                );
            }

            let sessions_clone = Arc::clone(&sessions);
            let socket_clone = Arc::clone(&socket);
            let ws_url = ws_url.clone();
            let token = token.clone();
            let target = rule.target.clone();
            let name = rule.name.clone();

            tokio::spawn(async move {
                debug!(
                    "[UDP:{}] Starting new UDP session {} for peer {}",
                    name, session_id, peer_addr
                );
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
                let mut m = sessions_clone.lock().await;
                if m.get(&peer_addr)
                    .is_some_and(|entry| entry.id == session_id)
                {
                    m.remove(&peer_addr);
                }
                debug!(
                    "[UDP:{}] Closed UDP session {} for peer {}",
                    name, session_id, peer_addr
                );
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

    let mut ping_interval = tokio::time::interval(WS_PING_INTERVAL);
    ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ping_interval.tick().await; // consume initial immediate tick

    let idle_sleep = tokio::time::sleep(UDP_IDLE_TIMEOUT);
    tokio::pin!(idle_sleep);

    loop {
        tokio::select! {
            pkt = rx.recv() => {
                match pkt {
                    Some(data) => {
                        idle_sleep.as_mut().reset(tokio::time::Instant::now() + UDP_IDLE_TIMEOUT);
                        if let Err(e) = ws_sender.send(Message::Binary(data)).await {
                            debug!("WS send error: {}", e);
                            break;
                        }
                    }
                    None => break,
                }
            }
            msg = ws_receiver.next() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        idle_sleep.as_mut().reset(tokio::time::Instant::now() + UDP_IDLE_TIMEOUT);
                        if let Err(e) = socket.send_to(&data, peer_addr).await {
                            debug!("UDP send_to error: {}", e);
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        debug!("WS receive error in UDP tunnel: {}", e);
                        break;
                    }
                }
            }
            _ = ping_interval.tick() => {
                if let Err(e) = ws_sender.send(Message::Ping(Vec::new())).await {
                    debug!("WS ping error in UDP tunnel: {}", e);
                    break;
                }
            }
            _ = &mut idle_sleep => {
                debug!("UDP session idle timeout");
                break;
            }
        }
    }

    let _ = ws_sender.close().await;
    Ok(())
}
