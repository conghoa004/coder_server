mod client;
mod config;
mod protocol;
mod server;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use config::{ClientConfig, ServerConfig, TunnelRule};
use protocol::TunnelProtocol;
use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(
    name = "wsb",
    version,
    about = "High-performance multi-protocol (TCP, UDP, HTTP, SOCKS5) tunnel bridge over WebSocket in Rust",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run as WebSocket Bridge Server (on remote host / Kaggle)
    Server(ServerArgs),
    /// Run as Tunnel Client (on local machine)
    Client(ClientArgs),
    /// Generate a cryptographically secure 256-bit random token
    GenToken,
}

#[derive(Args, Debug)]
struct ServerArgs {
    /// Path to server config TOML file
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Listening address for WebSocket server (e.g., 0.0.0.0:8766)
    #[arg(short, long, env = "WS_BRIDGE_LISTEN")]
    listen: Option<String>,

    /// Shared secret token for client authentication
    #[arg(short, long, env = "WS_BRIDGE_TOKEN")]
    token: Option<String>,

    /// Allowed target host:port whitelist (can be specified multiple times)
    #[arg(long = "allow")]
    allowed: Vec<String>,
}

#[derive(Args, Debug)]
struct ClientArgs {
    /// Path to client config TOML file
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Remote WebSocket server URL (e.g., wss://xxx.trycloudflare.com or ws://127.0.0.1:8766)
    #[arg(short = 'w', long = "ws", env = "WS_BRIDGE_URL")]
    ws_url: Option<String>,

    /// Shared secret token for authentication
    #[arg(short, long, env = "WS_BRIDGE_TOKEN")]
    token: Option<String>,

    /// TCP tunnel rule in format: LISTEN_ADDR=TARGET_ADDR (e.g., 127.0.0.1:13306=127.0.0.1:3306)
    #[arg(long = "tcp", value_name = "LISTEN=TARGET")]
    tcp_tunnels: Vec<String>,

    /// UDP tunnel rule in format: LISTEN_ADDR=TARGET_ADDR (e.g., 127.0.0.1:10053=1.1.1.1:53)
    #[arg(long = "udp", value_name = "LISTEN=TARGET")]
    udp_tunnels: Vec<String>,

    /// HTTP tunnel rule in format: LISTEN_ADDR=TARGET_ADDR (e.g., 127.0.0.1:18080=127.0.0.1:80)
    #[arg(long = "http", value_name = "LISTEN=TARGET")]
    http_tunnels: Vec<String>,

    /// Enable SOCKS5 dynamic proxy on local address (e.g., 127.0.0.1:1080)
    #[arg(long = "socks5", value_name = "LISTEN_ADDR")]
    socks5: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "wsb=info,info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Server(args) => {
            let mut config = if let Some(path) = args.config {
                ServerConfig::from_file(path)?
            } else {
                ServerConfig::default()
            };

            if let Some(listen) = args.listen {
                config.listen = listen;
            }
            if let Some(token) = args.token {
                if token.eq_ignore_ascii_case("auto") || token.eq_ignore_ascii_case("generate") {
                    config.token = config::generate_secure_token();
                } else {
                    config.token = token;
                }
            } else if config.token.eq_ignore_ascii_case("auto")
                || config.token.is_empty()
                || config.token == "12345"
            {
                config.token = config::generate_secure_token();
            }
            if !args.allowed.is_empty() {
                config.allowed_targets = Some(args.allowed);
            }

            server::run_server(config).await?;
        }
        Commands::GenToken => {
            let token = config::generate_secure_token();
            println!("{}", token);
        }
        Commands::Client(args) => {
            let mut config = if let Some(path) = args.config {
                ClientConfig::from_file(path)?
            } else {
                ClientConfig {
                    ws_url: String::new(),
                    token: "12345".to_string(),
                    tunnels: Vec::new(),
                    socks5: None,
                }
            };

            if let Some(ws_url) = args.ws_url {
                config.ws_url = ws_url;
            }
            if let Some(token) = args.token {
                config.token = token;
            }
            if let Some(socks5) = args.socks5 {
                config.socks5 = Some(socks5);
            }

            // Parse CLI tunnel flags
            for spec in args.tcp_tunnels {
                config
                    .tunnels
                    .push(parse_tunnel_spec(&spec, TunnelProtocol::Tcp)?);
            }
            for spec in args.udp_tunnels {
                config
                    .tunnels
                    .push(parse_tunnel_spec(&spec, TunnelProtocol::Udp)?);
            }
            for spec in args.http_tunnels {
                config
                    .tunnels
                    .push(parse_tunnel_spec(&spec, TunnelProtocol::Http)?);
            }

            if config.ws_url.is_empty() {
                anyhow::bail!(
                    "WebSocket URL is required. Provide via --ws <URL>, config file, or WS_BRIDGE_URL env"
                );
            }

            client::run_client(config).await?;
        }
    }

    Ok(())
}

fn parse_tunnel_spec(spec: &str, protocol: TunnelProtocol) -> Result<TunnelRule> {
    let parts: Vec<&str> = spec.split('=').collect();
    if parts.len() != 2 {
        anyhow::bail!(
            "Invalid tunnel spec '{}'. Expected format: LISTEN_ADDR=TARGET_ADDR (e.g. 127.0.0.1:13306=127.0.0.1:3306)",
            spec
        );
    }
    let listen = parts[0].trim().to_string();
    let target = parts[1].trim().to_string();
    let name = format!("{}-{}", protocol, target.replace(':', "-"));

    Ok(TunnelRule {
        name,
        protocol,
        listen,
        target,
    })
}
