use crate::bridge::bridge_tcp_ws;
use anyhow::{Context, Result};
use futures_util::stream::{SplitSink, SplitStream};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

pub async fn handle_tcp(
    ws_sender: SplitSink<WebSocketStream<TcpStream>, Message>,
    ws_receiver: SplitStream<WebSocketStream<TcpStream>>,
    target: &str,
) -> Result<()> {
    let target_stream = TcpStream::connect(target)
        .await
        .with_context(|| format!("Failed to connect to target TCP service at {}", target))?;

    bridge_tcp_ws(target_stream, ws_sender, ws_receiver).await
}
