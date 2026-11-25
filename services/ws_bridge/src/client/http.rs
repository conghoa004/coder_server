use crate::client::connect_tunnel;
use crate::config::TunnelRule;
use crate::protocol::TunnelProtocol;
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};

pub async fn run_http_listener(
    rule: TunnelRule,
    ws_url: String,
    token: String,
) -> Result<()> {
    let listener = TcpListener::bind(&rule.listen)
        .await
        .with_context(|| format!("Failed to bind HTTP listener on {}", rule.listen))?;

    info!(
        "[HTTP:{}] Listening on http://{} -> forward to remote {}",
        rule.name, rule.listen, rule.target
    );

    loop {
        let (local_stream, client_addr) = match listener.accept().await {
            Ok(v) => v,
            Err(e) => {
                warn!("[HTTP:{}] Accept error: {}", rule.name, e);
                continue;
            }
        };

        let ws_url = ws_url.clone();
        let token = token.clone();
        let target = rule.target.clone();
        let name = rule.name.clone();

        tokio::spawn(async move {
            debug!("[HTTP:{}] Inbound connection from {}", name, client_addr);
            if let Err(e) = handle_http_client(local_stream, &ws_url, &token, &target).await {
                error!("[HTTP:{}] Tunnel error for {}: {:#}", name, client_addr, e);
            }
            debug!("[HTTP:{}] Connection closed for {}", name, client_addr);
        });
    }
}

async fn handle_http_client(
    mut local_stream: tokio::net::TcpStream,
    ws_url: &str,
    token: &str,
    target: &str,
) -> Result<()> {
    // Read the initial chunk to inspect and rewrite headers if applicable
    let mut initial_buf = vec![0u8; 8192];
    let n = local_stream.read(&mut initial_buf).await?;
    if n == 0 {
        return Ok(());
    }
    initial_buf.truncate(n);

    // Optional header rewrite: rewrite Host header to target so Apache / phpMyAdmin
    // recognizes the host and generates proper links and redirects
    let modified_buf = rewrite_http_headers(&initial_buf, target);

    let (mut ws_sender, mut ws_receiver) =
        connect_tunnel(ws_url, token, TunnelProtocol::Http, target).await?;

    // Send the first (potentially rewritten) HTTP request chunk
    ws_sender.send(Message::Binary(modified_buf)).await?;

    let (mut tcp_reader, mut tcp_writer) = local_stream.into_split();

    // tcp -> ws
    let tcp_to_ws = async {
        let mut buf = vec![0u8; 65536];
        loop {
            match tcp_reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(bytes_read) => {
                    let data = buf[..bytes_read].to_vec();
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
                    debug!("WS receive error in HTTP tunnel: {}", e);
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

/// Rewrites the Host header in the initial HTTP request buffer to match the target
fn rewrite_http_headers(raw: &[u8], target: &str) -> Vec<u8> {
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut req = httparse::Request::new(&mut headers);

    let parsed = match req.parse(raw) {
        Ok(httparse::Status::Complete(offset)) | Ok(httparse::Status::Partial(offset)) => offset,
        Err(_) => return raw.to_vec(), // Not valid HTTP header, pass raw bytes through
    };

    let method = match req.method {
        Some(m) => m,
        None => return raw.to_vec(),
    };
    let path = match req.path {
        Some(p) => p,
        None => return raw.to_vec(),
    };
    let version = match req.version {
        Some(1) => "HTTP/1.1",
        _ => "HTTP/1.0",
    };

    let mut new_request = format!("{} {} {}\r\n", method, path, version);

    // Reconstruct headers with replaced Host header
    for header in req.headers.iter() {
        if header.name.eq_ignore_ascii_case("Host") {
            new_request.push_str(&format!("Host: {}\r\n", target));
        } else {
            let val = String::from_utf8_lossy(header.value);
            new_request.push_str(&format!("{}: {}\r\n", header.name, val));
        }
    }
    new_request.push_str("\r\n");

    let mut result = new_request.into_bytes();
    // Append any body data that was read after headers
    if raw.len() > parsed {
        result.extend_from_slice(&raw[parsed..]);
    }
    result
}
