use crate::bridge::bridge_tcp_ws;
use anyhow::{Context, Result};
use futures_util::stream::{SplitSink, SplitStream};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

const TCP_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub async fn connect_tcp_target(target: &str) -> Result<TcpStream> {
    tokio::time::timeout(TCP_CONNECT_TIMEOUT, TcpStream::connect(target))
        .await
        .with_context(|| format!("Timed out connecting to target TCP service at {}", target))?
        .with_context(|| format!("Failed to connect to target TCP service at {}", target))
}

pub async fn handle_tcp_connected(
    target_stream: TcpStream,
    ws_sender: SplitSink<WebSocketStream<TcpStream>, Message>,
    ws_receiver: SplitStream<WebSocketStream<TcpStream>>,
) -> Result<()> {
    bridge_tcp_ws(target_stream, ws_sender, ws_receiver).await
}
