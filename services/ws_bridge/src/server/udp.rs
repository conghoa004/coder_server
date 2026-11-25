use anyhow::{Context, Result};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::net::{TcpStream, UdpSocket};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use tracing::debug;

pub async fn handle_udp(
    mut ws_sender: SplitSink<WebSocketStream<TcpStream>, Message>,
    mut ws_receiver: SplitStream<WebSocketStream<TcpStream>>,
    target: &str,
) -> Result<()> {
    // Bind ephemeral UDP socket for this session
    let socket = UdpSocket::bind("0.0.0.0:0")
        .await
        .context("Failed to bind ephemeral UDP socket")?;

    socket
        .connect(target)
        .await
        .with_context(|| format!("Failed to connect UDP socket to target {}", target))?;

    let timeout_duration = Duration::from_secs(60);

    // Forward WS binary -> UDP socket
    let ws_to_udp = async {
        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(Message::Binary(data)) => {
                    if let Err(e) = socket.send(&data).await {
                        debug!("UDP send error: {}", e);
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

    // Forward UDP socket -> WS binary
    let udp_to_ws = async {
        let mut buf = vec![0u8; 65536];
        loop {
            match tokio::time::timeout(timeout_duration, socket.recv(&mut buf)).await {
                Ok(Ok(n)) => {
                    let data = buf[..n].to_vec();
                    if let Err(e) = ws_sender.send(Message::Binary(data)).await {
                        debug!("WS send error in UDP tunnel: {}", e);
                        break;
                    }
                }
                Ok(Err(e)) => {
                    debug!("UDP read error: {}", e);
                    break;
                }
                Err(_) => {
                    debug!("UDP session idle timeout");
                    break;
                }
            }
        }
        let _ = ws_sender.close().await;
    };

    tokio::select! {
        _ = ws_to_udp => {},
        _ = udp_to_ws => {},
    }

    Ok(())
}
