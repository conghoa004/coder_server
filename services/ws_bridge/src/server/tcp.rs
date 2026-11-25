use anyhow::{Context, Result};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use tracing::debug;

pub async fn handle_tcp(
    mut ws_sender: SplitSink<WebSocketStream<TcpStream>, Message>,
    mut ws_receiver: SplitStream<WebSocketStream<TcpStream>>,
    target: &str,
) -> Result<()> {
    let target_stream = TcpStream::connect(target)
        .await
        .with_context(|| format!("Failed to connect to target TCP service at {}", target))?;

    let (mut tcp_reader, mut tcp_writer) = target_stream.into_split();

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
                    debug!("WS receive error in TCP tunnel: {}", e);
                    break;
                }
            }
        }
        let _ = tcp_writer.shutdown().await;
    };

    // tcp -> ws
    let tcp_to_ws = async {
        let mut buf = vec![0u8; 65536];
        loop {
            match tcp_reader.read(&mut buf).await {
                Ok(0) => break, // EOF
                Ok(n) => {
                    let data = buf[..n].to_vec();
                    if let Err(e) = ws_sender.send(Message::Binary(data)).await {
                        debug!("WS send error in TCP tunnel: {}", e);
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

    tokio::select! {
        _ = ws_to_tcp => {},
        _ = tcp_to_ws => {},
    }

    Ok(())
}
