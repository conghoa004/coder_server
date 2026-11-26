use crate::bridge::bridge_io_ws;
use crate::client::connect_tunnel;
use crate::config::TunnelRule;
use crate::protocol::TunnelProtocol;
use anyhow::{Context, Result};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;
use tracing::{debug, error, info, warn};

const MAX_HTTP_HEADER_BYTES: usize = 256 * 1024;
const MAX_CHUNK_LINE_BYTES: usize = 8192;

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
    let _ = local_stream.set_nodelay(true);

    // Wait for initial bytes before opening the remote WebSocket tunnel
    let mut initial_buf = vec![0u8; 32768];
    let n = local_stream.read(&mut initial_buf).await?;
    if n == 0 {
        return Ok(());
    }
    initial_buf.truncate(n);

    let (ws_sender, ws_receiver) =
        connect_tunnel(ws_url, token, TunnelProtocol::Http, target).await?;

    let (local_reader, local_writer) = local_stream.into_split();
    let (pipe_writer, pipe_reader) = tokio::io::duplex(65536);

    let target_owned = target.to_string();
    let rewriter_task = tokio::spawn(async move {
        if let Err(e) =
            forward_and_rewrite_http(initial_buf, local_reader, pipe_writer, &target_owned).await
        {
            debug!("HTTP rewriter finished with error: {}", e);
        }
    });

    let bridge_res = bridge_io_ws(pipe_reader, local_writer, ws_sender, ws_receiver).await;
    rewriter_task.abort();
    bridge_res
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HttpParseMode {
    Headers,
    FixedBody(usize),
    ChunkHeader,
    ChunkData(usize),
    ChunkTrailers,
    Passthrough,
}

/// Streams HTTP requests from `reader` to `writer`, rewriting the `Host` header
/// on every request (including HTTP/1.1 Keep-Alive, fragmented headers, fixed
/// `Content-Length` bodies, and `Transfer-Encoding: chunked` bodies).
async fn forward_and_rewrite_http<R, W>(
    initial: Vec<u8>,
    mut reader: R,
    mut writer: W,
    target: &str,
) -> Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buf = initial;
    let mut mode = HttpParseMode::Headers;
    let mut eof_reached = false;
    let mut read_buf = vec![0u8; 32768];

    loop {
        let mut need_more_data = false;

        match mode {
            HttpParseMode::Passthrough => {
                if !buf.is_empty() {
                    writer.write_all(&buf).await?;
                    buf.clear();
                }
                if eof_reached {
                    break;
                }
                let n = reader.read(&mut read_buf).await?;
                if n == 0 {
                    break;
                }
                writer.write_all(&read_buf[..n]).await?;
                continue;
            }
            HttpParseMode::FixedBody(mut remaining) => {
                if remaining == 0 {
                    mode = HttpParseMode::Headers;
                    continue;
                }
                if !buf.is_empty() {
                    let take = remaining.min(buf.len());
                    writer.write_all(&buf[..take]).await?;
                    buf.drain(..take);
                    remaining -= take;
                    mode = if remaining == 0 {
                        HttpParseMode::Headers
                    } else {
                        HttpParseMode::FixedBody(remaining)
                    };
                } else if eof_reached {
                    break;
                } else {
                    need_more_data = true;
                }
            }
            HttpParseMode::ChunkData(mut remaining) => {
                if remaining == 0 {
                    mode = HttpParseMode::ChunkHeader;
                    continue;
                }
                if !buf.is_empty() {
                    let take = remaining.min(buf.len());
                    writer.write_all(&buf[..take]).await?;
                    buf.drain(..take);
                    remaining -= take;
                    mode = if remaining == 0 {
                        HttpParseMode::ChunkHeader
                    } else {
                        HttpParseMode::ChunkData(remaining)
                    };
                } else if eof_reached {
                    break;
                } else {
                    need_more_data = true;
                }
            }
            HttpParseMode::ChunkHeader => {
                if let Some(crlf_pos) = find_crlf(&buf) {
                    let line = &buf[..crlf_pos];
                    let size_part = match line.iter().position(|&b| b == b';') {
                        Some(idx) => &line[..idx],
                        None => line,
                    };
                    let size_str = std::str::from_utf8(size_part).ok().map(|s| s.trim());
                    let parsed_chunk = size_str
                        .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit()))
                        .and_then(|s| usize::from_str_radix(s, 16).ok());
                    match parsed_chunk {
                        Some(0) => {
                            writer.write_all(&buf[..crlf_pos + 2]).await?;
                            buf.drain(..crlf_pos + 2);
                            mode = HttpParseMode::ChunkTrailers;
                        }
                        Some(chunk_size) => match chunk_size.checked_add(2) {
                            Some(total_len) => {
                                writer.write_all(&buf[..crlf_pos + 2]).await?;
                                buf.drain(..crlf_pos + 2);
                                mode = HttpParseMode::ChunkData(total_len);
                            }
                            None => {
                                mode = HttpParseMode::Passthrough;
                            }
                        },
                        None => {
                            mode = HttpParseMode::Passthrough;
                        }
                    }
                } else if buf.len() >= MAX_CHUNK_LINE_BYTES || eof_reached {
                    mode = HttpParseMode::Passthrough;
                } else {
                    need_more_data = true;
                }
            }
            HttpParseMode::ChunkTrailers => {
                if let Some(crlf_pos) = find_crlf(&buf) {
                    writer.write_all(&buf[..crlf_pos + 2]).await?;
                    buf.drain(..crlf_pos + 2);
                    if crlf_pos == 0 {
                        mode = HttpParseMode::Headers;
                    }
                } else if buf.len() >= MAX_HTTP_HEADER_BYTES || eof_reached {
                    mode = HttpParseMode::Passthrough;
                } else {
                    need_more_data = true;
                }
            }
            HttpParseMode::Headers => {
                if buf.is_empty() {
                    if eof_reached {
                        break;
                    }
                    need_more_data = true;
                } else {
                    match parse_and_rewrite_request(&buf, target) {
                        ParseOutcome::Rewritten {
                            rewritten,
                            consumed,
                            next_mode,
                        } => {
                            writer.write_all(&rewritten).await?;
                            buf.drain(..consumed);
                            mode = next_mode;
                        }
                        ParseOutcome::Partial => {
                            if buf.len() >= MAX_HTTP_HEADER_BYTES || eof_reached {
                                mode = HttpParseMode::Passthrough;
                            } else {
                                need_more_data = true;
                            }
                        }
                        ParseOutcome::NotHttp => {
                            mode = HttpParseMode::Passthrough;
                        }
                    }
                }
            }
        }

        if need_more_data {
            let n = reader.read(&mut read_buf).await?;
            if n == 0 {
                eof_reached = true;
            } else {
                buf.extend_from_slice(&read_buf[..n]);
            }
        }
    }

    writer.shutdown().await?;
    Ok(())
}

fn find_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\r\n")
}

enum ParseOutcome {
    Rewritten {
        rewritten: Vec<u8>,
        consumed: usize,
        next_mode: HttpParseMode,
    },
    Partial,
    NotHttp,
}

fn parse_and_rewrite_request(raw: &[u8], target: &str) -> ParseOutcome {
    let mut header_cap = 64;
    loop {
        let mut headers = vec![httparse::EMPTY_HEADER; header_cap];
        let mut req = httparse::Request::new(&mut headers);

        match req.parse(raw) {
            Ok(httparse::Status::Complete(offset)) => {
                let method = match req.method {
                    Some(m) => m,
                    None => return ParseOutcome::NotHttp,
                };
                let path = match req.path {
                    Some(p) => p,
                    None => return ParseOutcome::NotHttp,
                };
                let version = match req.version {
                    Some(1) => "HTTP/1.1",
                    _ => "HTTP/1.0",
                };

                let mut is_upgrade = method.eq_ignore_ascii_case("CONNECT");
                let mut is_chunked = false;
                let mut content_length: Option<usize> = None;
                let mut host_written = false;

                let mut out = Vec::with_capacity(offset + 64);
                out.extend_from_slice(method.as_bytes());
                out.push(b' ');
                out.extend_from_slice(path.as_bytes());
                out.push(b' ');
                out.extend_from_slice(version.as_bytes());
                out.extend_from_slice(b"\r\n");

                for header in req.headers.iter() {
                    if header.name.eq_ignore_ascii_case("Host") {
                        if !host_written {
                            out.extend_from_slice(b"Host: ");
                            out.extend_from_slice(target.as_bytes());
                            out.extend_from_slice(b"\r\n");
                            host_written = true;
                        }
                    } else {
                        if header.name.eq_ignore_ascii_case("Upgrade") {
                            is_upgrade = true;
                        } else if header.name.eq_ignore_ascii_case("Connection") {
                            if let Ok(val) = std::str::from_utf8(header.value) {
                                if val
                                    .split(',')
                                    .any(|token| token.trim().eq_ignore_ascii_case("upgrade"))
                                {
                                    is_upgrade = true;
                                }
                            }
                        } else if header.name.eq_ignore_ascii_case("Transfer-Encoding") {
                            if let Ok(val) = std::str::from_utf8(header.value) {
                                if val
                                    .split(',')
                                    .any(|token| token.trim().eq_ignore_ascii_case("chunked"))
                                {
                                    is_chunked = true;
                                }
                            }
                        } else if header.name.eq_ignore_ascii_case("Content-Length") {
                            let parsed_len = std::str::from_utf8(header.value)
                                .ok()
                                .and_then(|val| val.trim().parse::<usize>().ok());
                            match (parsed_len, content_length) {
                                (Some(len), None) => content_length = Some(len),
                                (Some(len), Some(prev)) if len == prev => {}
                                _ => return ParseOutcome::NotHttp,
                            }
                        }

                        out.extend_from_slice(header.name.as_bytes());
                        out.extend_from_slice(b": ");
                        out.extend_from_slice(header.value);
                        out.extend_from_slice(b"\r\n");
                    }
                }

                if !host_written && req.version == Some(1) {
                    out.extend_from_slice(b"Host: ");
                    out.extend_from_slice(target.as_bytes());
                    out.extend_from_slice(b"\r\n");
                }

                out.extend_from_slice(b"\r\n");

                let next_mode = if is_upgrade {
                    HttpParseMode::Passthrough
                } else if is_chunked {
                    HttpParseMode::ChunkHeader
                } else if let Some(len) = content_length.filter(|&l| l > 0) {
                    HttpParseMode::FixedBody(len)
                } else {
                    HttpParseMode::Headers
                };

                return ParseOutcome::Rewritten {
                    rewritten: out,
                    consumed: offset,
                    next_mode,
                };
            }
            Ok(httparse::Status::Partial) => return ParseOutcome::Partial,
            Err(httparse::Error::TooManyHeaders) if header_cap < 256 => {
                header_cap *= 2;
            }
            Err(_) => return ParseOutcome::NotHttp,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn test_http_rewrites_keepalive_chunked_and_fragmented_requests() {
        let (mut in_writer, in_reader) = tokio::io::duplex(4096);
        let (out_writer, mut out_reader) = tokio::io::duplex(4096);

        let initial = b"GET /one HTTP/1.1\r\nHost: 127.0.0.1:18080\r\n".to_vec();

        let task = tokio::spawn(async move {
            forward_and_rewrite_http(initial, in_reader, out_writer, "127.0.0.1:80")
                .await
                .unwrap();
        });

        // Finish 1st request headers, then send 2nd (Content-Length), 3rd (chunked), 4th (Upgrade)
        in_writer
            .write_all(b"X-Test: 1\r\n\r\n")
            .await
            .unwrap();
        in_writer
            .write_all(
                b"POST /two HTTP/1.1\r\nHost: localhost:18080\r\nContent-Length: 5\r\n\r\nhello",
            )
            .await
            .unwrap();
        in_writer
            .write_all(
                b"POST /three HTTP/1.1\r\nHost: localhost:18080\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nworld\r\n0\r\n\r\n",
            )
            .await
            .unwrap();
        in_writer
            .write_all(
                b"GET /ws HTTP/1.1\r\nHost: localhost:18080\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n\r\nRAW_WS_DATA",
            )
            .await
            .unwrap();
        in_writer.shutdown().await.unwrap();

        let mut output = Vec::new();
        out_reader.read_to_end(&mut output).await.unwrap();
        task.await.unwrap();

        let expected = concat!(
            "GET /one HTTP/1.1\r\nHost: 127.0.0.1:80\r\nX-Test: 1\r\n\r\n",
            "POST /two HTTP/1.1\r\nHost: 127.0.0.1:80\r\nContent-Length: 5\r\n\r\nhello",
            "POST /three HTTP/1.1\r\nHost: 127.0.0.1:80\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nworld\r\n0\r\n\r\n",
            "GET /ws HTTP/1.1\r\nHost: 127.0.0.1:80\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n\r\nRAW_WS_DATA"
        );
        assert_eq!(String::from_utf8(output).unwrap(), expected);
    }

    #[test]
    fn test_http_preserves_non_utf8_and_many_headers() {
        let mut raw = b"GET / HTTP/1.1\r\nHost: 127.0.0.1:18080\r\n".to_vec();
        for i in 0..70 {
            raw.extend_from_slice(format!("X-H-{}: val\r\n", i).as_bytes());
        }
        raw.extend_from_slice(b"X-Bin: \xff\xfe\r\n\r\n");

        match parse_and_rewrite_request(&raw, "127.0.0.1:80") {
            ParseOutcome::Rewritten { rewritten, .. } => {
                assert!(rewritten
                    .windows(b"Host: 127.0.0.1:80\r\n".len())
                    .any(|w| w == b"Host: 127.0.0.1:80\r\n"));
                assert!(rewritten
                    .windows(b"X-Bin: \xff\xfe\r\n".len())
                    .any(|w| w == b"X-Bin: \xff\xfe\r\n"));
            }
            _ => panic!("Expected HTTP request to parse and rewrite"),
        }
    }

    #[tokio::test]
    async fn test_http_handles_chunk_size_overflow_without_panic() {
        let (mut in_writer, in_reader) = tokio::io::duplex(4096);
        let (out_writer, mut out_reader) = tokio::io::duplex(4096);

        let initial =
            b"POST /up HTTP/1.1\r\nHost: 127.0.0.1:18080\r\nTransfer-Encoding: chunked\r\n\r\nffffffffffffffff\r\nrest"
                .to_vec();

        let task = tokio::spawn(async move {
            forward_and_rewrite_http(initial, in_reader, out_writer, "127.0.0.1:80")
                .await
                .unwrap();
        });

        in_writer.shutdown().await.unwrap();
        let mut output = Vec::new();
        out_reader.read_to_end(&mut output).await.unwrap();
        task.await.unwrap();

        assert_eq!(
            String::from_utf8(output).unwrap(),
            "POST /up HTTP/1.1\r\nHost: 127.0.0.1:80\r\nTransfer-Encoding: chunked\r\n\r\nffffffffffffffff\r\nrest"
        );
    }
}
