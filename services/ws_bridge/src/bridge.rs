use anyhow::Result;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use tracing::debug;

const WS_PING_INTERVAL: Duration = Duration::from_secs(25);

/// Bridges a TCP stream and a WebSocket stream bidirectionally, sending periodic
/// WebSocket Ping frames to keep idle tunnels alive behind proxies (e.g. Cloudflare).
pub async fn bridge_tcp_ws<S>(
    tcp_stream: TcpStream,
    mut ws_sender: SplitSink<WebSocketStream<S>, Message>,
    mut ws_receiver: SplitStream<WebSocketStream<S>>,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let _ = tcp_stream.set_nodelay(true);
    let (mut tcp_reader, mut tcp_writer) = tcp_stream.into_split();

    // tcp -> ws (with periodic WebSocket Ping keepalive)
    let tcp_to_ws = async {
        let mut ping_interval = tokio::time::interval(WS_PING_INTERVAL);
        ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        ping_interval.tick().await; // consume initial immediate tick

        let mut buf = vec![0u8; 65536];
        loop {
            tokio::select! {
                res = tcp_reader.read(&mut buf) => {
                    match res {
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
                _ = ping_interval.tick() => {
                    if let Err(e) = ws_sender.send(Message::Ping(Vec::new())).await {
                        debug!("WS ping error: {}", e);
                        break;
                    }
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
