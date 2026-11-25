use crate::client::connect_tunnel;
use crate::protocol::TunnelProtocol;
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

/// SOCKS5 Dynamic Proxy Listener
/// Allows any SOCKS5 client (browsers, curl, database GUIs, SSH) to connect to ANY
/// target on the remote machine dynamically through a single port!
pub async fn run_socks5_listener(
    listen_addr: String,
    ws_url: String,
    token: String,
) -> Result<()> {
    let listener = TcpListener::bind(&listen_addr)
        .await
        .with_context(|| format!("Failed to bind SOCKS5 listener on {}", listen_addr))?;

    info!("[SOCKS5] Listening on socks5://{}", listen_addr);

    loop {
        let (stream, client_addr) = match listener.accept().await {
            Ok(v) => v,
            Err(e) => {
                warn!("[SOCKS5] Accept error: {}", e);
                continue;
            }
        };

        let ws_url = ws_url.clone();
        let token = token.clone();

        tokio::spawn(async move {
            debug!("[SOCKS5] Inbound connection from {}", client_addr);
            if let Err(e) = handle_socks5_client(stream, &ws_url, &token).await {
                debug!("[SOCKS5] Client error for {}: {:#}", client_addr, e);
            }
        });
    }
}

async fn handle_socks5_client(
    mut stream: tokio::net::TcpStream,
    ws_url: &str,
    token: &str,
) -> Result<()> {
    // 1. Negotiation Phase
    let mut ver_nmethods = [0u8; 2];
    stream.read_exact(&mut ver_nmethods).await?;

    if ver_nmethods[0] != 0x05 {
        anyhow::bail!("Unsupported SOCKS version: {}", ver_nmethods[0]);
    }

    let nmethods = ver_nmethods[1] as usize;
    let mut methods = vec![0u8; nmethods];
    stream.read_exact(&mut methods).await?;

    // We accept NO AUTHENTICATION REQUIRED (0x00)
    if !methods.contains(&0x00) {
        stream.write_all(&[0x05, 0xFF]).await?;
        anyhow::bail!("No supported auth method offered by SOCKS5 client");
    }

    stream.write_all(&[0x05, 0x00]).await?;

    // 2. Request Phase
    let mut header = [0u8; 4];
    stream.read_exact(&mut header).await?;

    let cmd = header[1];
    let atyp = header[3];

    // We support CONNECT (0x01)
    if cmd != 0x01 {
        // Command not supported
        stream.write_all(&[0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;
        anyhow::bail!("Unsupported SOCKS5 command: {}", cmd);
    }

    let target_host = match atyp {
        0x01 => {
            // IPv4 (4 bytes)
            let mut ip = [0u8; 4];
            stream.read_exact(&mut ip).await?;
            format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3])
        }
        0x03 => {
            // Domain name (1 byte len + string)
            let mut len = [0u8; 1];
            stream.read_exact(&mut len).await?;
            let mut domain = vec![0u8; len[0] as usize];
            stream.read_exact(&mut domain).await?;
            String::from_utf8(domain).context("Invalid UTF-8 domain name")?
        }
        0x04 => {
            // IPv6 (16 bytes)
            let mut ip = [0u8; 16];
            stream.read_exact(&mut ip).await?;
            use std::net::Ipv6Addr;
            Ipv6Addr::from(ip).to_string()
        }
        _ => {
            stream.write_all(&[0x05, 0x08, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;
            anyhow::bail!("Unsupported address type: {}", atyp);
        }
    };

    let mut port_bytes = [0u8; 2];
    stream.read_exact(&mut port_bytes).await?;
    let target_port = u16::from_be_bytes(port_bytes);
    let target = format!("{}:{}", target_host, target_port);

    debug!("[SOCKS5] Requesting tunnel to {}", target);

    // 3. Connect to WebSocket tunnel
    let (mut ws_sender, mut ws_receiver) = match connect_tunnel(
        ws_url,
        token,
        TunnelProtocol::Tcp,
        &target,
    )
    .await
    {
        Ok(t) => t,
        Err(e) => {
            // General SOCKS server failure (0x01)
            let _ = stream.write_all(&[0x05, 0x01, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await;
            return Err(e);
        }
    };

    // Reply SOCKS5 Success (0x00)
    stream.write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;

    // 4. Bi-directional forwarding
    let (mut tcp_reader, mut tcp_writer) = stream.into_split();

    let tcp_to_ws = async {
        let mut buf = vec![0u8; 65536];
        loop {
            match tcp_reader.read(&mut buf).await {
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
        let _ = ws_sender.close().await;
    };

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
