use crate::bridge::WS_PING_INTERVAL;
use anyhow::{Context, Result};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::net::{TcpStream, UdpSocket};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use tracing::debug;

const UDP_IDLE_TIMEOUT: Duration = Duration::from_secs(60);
const UDP_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub async fn connect_udp_target(target: &str) -> Result<UdpSocket> {
    tokio::time::timeout(UDP_CONNECT_TIMEOUT, async {
        let target_addr = tokio::net::lookup_host(target)
            .await
            .with_context(|| format!("Failed to resolve UDP target {}", target))?
            .next()
            .with_context(|| format!("No address resolved for UDP target {}", target))?;

        let bind_addr = if target_addr.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        };

        // Bind ephemeral UDP socket matching target IP family
        let socket = UdpSocket::bind(bind_addr)
            .await
            .with_context(|| format!("Failed to bind ephemeral UDP socket on {}", bind_addr))?;

        socket
            .connect(target_addr)
            .await
            .with_context(|| format!("Failed to connect UDP socket to target {}", target_addr))?;

        Ok(socket)
    })
    .await
    .with_context(|| format!("Timed out connecting to UDP target {}", target))?
}

pub async fn handle_udp_connected(
    mut ws_sender: SplitSink<WebSocketStream<TcpStream>, Message>,
    mut ws_receiver: SplitStream<WebSocketStream<TcpStream>>,
    socket: UdpSocket,
) -> Result<()> {
    let mut buf = vec![0u8; 65536];
    let mut ping_interval = tokio::time::interval(WS_PING_INTERVAL);
    ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ping_interval.tick().await; // consume initial immediate tick

    let idle_sleep = tokio::time::sleep(UDP_IDLE_TIMEOUT);
    tokio::pin!(idle_sleep);

    loop {
        tokio::select! {
            msg = ws_receiver.next() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        idle_sleep.as_mut().reset(tokio::time::Instant::now() + UDP_IDLE_TIMEOUT);
                        if let Err(e) = socket.send(&data).await {
                            debug!("UDP send error: {}", e);
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
            res = socket.recv(&mut buf) => {
                match res {
                    Ok(n) => {
                        idle_sleep.as_mut().reset(tokio::time::Instant::now() + UDP_IDLE_TIMEOUT);
                        let data = buf[..n].to_vec();
                        if let Err(e) = ws_sender.send(Message::Binary(data)).await {
                            debug!("WS send error in UDP tunnel: {}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        debug!("UDP read error: {}", e);
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
