use anyhow::Result;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use tracing::debug;

pub const WS_PING_INTERVAL: Duration = Duration::from_secs(25);
const EOF_CONTROL_MSG: &str = "wsb:eof";

#[derive(Clone, Copy, Default)]
struct BridgeState {
    local_eof: bool,
    remote_eof: bool,
    aborted: bool,
}

impl BridgeState {
    fn is_done(self) -> bool {
        self.aborted || (self.local_eof && self.remote_eof)
    }
}

/// Bridges a TCP stream and a WebSocket stream bidirectionally, preserving TCP
/// half-close semantics and sending periodic WebSocket Ping frames to keep idle
/// tunnels alive behind proxies (e.g. Cloudflare).
pub async fn bridge_tcp_ws<S>(
    tcp_stream: TcpStream,
    ws_sender: SplitSink<WebSocketStream<S>, Message>,
    ws_receiver: SplitStream<WebSocketStream<S>>,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let _ = tcp_stream.set_nodelay(true);
    let (tcp_reader, tcp_writer) = tcp_stream.into_split();
    bridge_io_ws(tcp_reader, tcp_writer, ws_sender, ws_receiver).await
}

/// Bridges arbitrary `AsyncRead` + `AsyncWrite` halves with a WebSocket stream.
pub async fn bridge_io_ws<R, W, S>(
    mut reader: R,
    mut writer: W,
    mut ws_sender: SplitSink<WebSocketStream<S>, Message>,
    mut ws_receiver: SplitStream<WebSocketStream<S>>,
) -> Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (state_tx, _) = watch::channel(BridgeState::default());

    // local -> ws (with periodic WebSocket Ping keepalive and half-close signaling)
    let mut sender_state_rx = state_tx.subscribe();
    let local_to_ws = async {
        let mut ping_interval = tokio::time::interval(WS_PING_INTERVAL);
        ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        ping_interval.tick().await; // consume initial immediate tick

        let mut buf = vec![0u8; 65536];
        let mut local_done = false;

        loop {
            if sender_state_rx.borrow_and_update().is_done() {
                break;
            }
            tokio::select! {
                res = reader.read(&mut buf), if !local_done => {
                    match res {
                        Ok(0) => {
                            local_done = true;
                            if let Err(e) = ws_sender.send(Message::Text(EOF_CONTROL_MSG.to_string())).await {
                                debug!("WS EOF send error: {}", e);
                                state_tx.send_modify(|s| s.aborted = true);
                                break;
                            }
                            state_tx.send_modify(|s| s.local_eof = true);
                        }
                        Ok(n) => {
                            let data = buf[..n].to_vec();
                            if let Err(e) = ws_sender.send(Message::Binary(data)).await {
                                debug!("WS send error: {}", e);
                                state_tx.send_modify(|s| s.aborted = true);
                                break;
                            }
                        }
                        Err(e) => {
                            debug!("Stream read error: {}", e);
                            state_tx.send_modify(|s| s.aborted = true);
                            break;
                        }
                    }
                }
                _ = ping_interval.tick() => {
                    if let Err(e) = ws_sender.send(Message::Ping(Vec::new())).await {
                        debug!("WS ping error: {}", e);
                        state_tx.send_modify(|s| s.aborted = true);
                        break;
                    }
                }
                res = sender_state_rx.changed() => {
                    if res.is_err() {
                        break;
                    }
                }
            }
        }
        let _ = ws_sender.close().await;
    };

    // ws -> local
    let mut receiver_state_rx = state_tx.subscribe();
    let ws_to_local = async {
        loop {
            if receiver_state_rx.borrow_and_update().is_done() {
                break;
            }
            tokio::select! {
                msg = ws_receiver.next() => {
                    match msg {
                        Some(Ok(Message::Binary(data))) => {
                            if let Err(e) = writer.write_all(&data).await {
                                debug!("Stream write error: {}", e);
                                state_tx.send_modify(|s| s.aborted = true);
                                break;
                            }
                        }
                        Some(Ok(Message::Text(text))) if text == EOF_CONTROL_MSG => {
                            let _ = writer.shutdown().await;
                            state_tx.send_modify(|s| s.remote_eof = true);
                        }
                        Some(Ok(Message::Close(_))) | None => {
                            let _ = writer.shutdown().await;
                            state_tx.send_modify(|s| s.aborted = true);
                            break;
                        }
                        Some(Ok(_)) => {}
                        Some(Err(e)) => {
                            debug!("WS receive error: {}", e);
                            state_tx.send_modify(|s| s.aborted = true);
                            break;
                        }
                    }
                }
                res = receiver_state_rx.changed() => {
                    if res.is_err() {
                        break;
                    }
                }
            }
        }
        let _ = writer.shutdown().await;
    };

    tokio::join!(local_to_ws, ws_to_local);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn test_bridge_preserves_tcp_half_close() {
        // Local TCP server (acts as ws_bridge client's local listener)
        let client_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client_addr = client_listener.local_addr().unwrap();

        // Remote target TCP service (waits for client EOF before replying)
        let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target_addr = target_listener.local_addr().unwrap();

        // WS server listener
        let ws_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ws_addr = ws_listener.local_addr().unwrap();

        let target_task = tokio::spawn(async move {
            let (mut sock, _) = target_listener.accept().await.unwrap();
            let mut req = Vec::new();
            sock.read_to_end(&mut req).await.unwrap();
            assert_eq!(req, b"PING_HALF_CLOSE");
            sock.write_all(b"PONG_RESPONSE").await.unwrap();
            sock.shutdown().await.unwrap();
        });

        let ws_server_task = tokio::spawn(async move {
            let (ws_tcp, _) = ws_listener.accept().await.unwrap();
            let ws_stream = tokio_tungstenite::accept_async(ws_tcp).await.unwrap();
            let (ws_sender, ws_receiver) = ws_stream.split();
            let target_stream = TcpStream::connect(target_addr).await.unwrap();
            bridge_tcp_ws(target_stream, ws_sender, ws_receiver)
                .await
                .unwrap();
        });

        let ws_client_task = tokio::spawn(async move {
            let (local_stream, _) = client_listener.accept().await.unwrap();
            let ws_tcp = TcpStream::connect(ws_addr).await.unwrap();
            let (ws_stream, _) =
                tokio_tungstenite::client_async(format!("ws://{}", ws_addr), ws_tcp)
                    .await
                    .unwrap();
            let (ws_sender, ws_receiver) = ws_stream.split();
            bridge_tcp_ws(local_stream, ws_sender, ws_receiver)
                .await
                .unwrap();
        });

        // App connects to local listener, writes request, immediately shuts down write half
        let mut app = TcpStream::connect(client_addr).await.unwrap();
        app.write_all(b"PING_HALF_CLOSE").await.unwrap();
        app.shutdown().await.unwrap();

        let mut resp = Vec::new();
        app.read_to_end(&mut resp).await.unwrap();
        assert_eq!(resp, b"PONG_RESPONSE");

        target_task.await.unwrap();
        ws_server_task.await.unwrap();
        ws_client_task.await.unwrap();
    }
}
