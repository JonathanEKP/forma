//! `forma-mcp`: the MCP front end for Forma.
//!
//! It is a client of `forma-server`'s REST API, never of the database
//! (plan section 5). V1 speaks stdio; HTTP is reserved for the remote phase.

mod client;
mod http;
mod server;

use anyhow::Context as _;
use rmcp::transport::stdio;
use rmcp::ServiceExt;
use tracing_subscriber::EnvFilter;

use crate::client::ServerClient;
use crate::server::FormaMcp;

fn env_or(key: &str, fallback: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| fallback.to_string())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut transport = "stdio".to_string();
    let mut listen = env_or("FORMA_MCP_BIND", "127.0.0.1:4766");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--transport" => {
                transport = args.next().context("--transport needs a value")?;
            }
            "--listen" => {
                listen = args.next().context("--listen needs a value")?;
            }
            "--help" | "-h" => {
                println!("forma-mcp [--transport stdio|http] [--listen ADDR]");
                println!();
                println!("Environment:");
                println!(
                    "  FORMA_SERVER_URL    base URL of forma-server (default http://127.0.0.1:4765)"
                );
                println!("  FORMA_MCP_BIND      listen address for --transport http (default 127.0.0.1:4766)");
                println!("  FORMA_REMOTE_TOKEN  bearer token; required by --transport http");
                println!("  FORMA_DOMAIN        public hostname to accept in the Host header,");
                println!("                      for --transport http behind a reverse proxy");
                println!("  FORMA_LOG           log filter (default warn)");
                return Ok(());
            }
            other => anyhow::bail!("unknown argument: {other}"),
        }
    }

    // stdout carries the MCP protocol on stdio, so logs must go to stderr. HTTP
    // does not need that, but keeping one destination means a deployment reads
    // the same either way.
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(env_or("FORMA_LOG", "warn")))
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();

    let base_url = env_or("FORMA_SERVER_URL", "http://127.0.0.1:4765");
    let token = std::env::var("FORMA_REMOTE_TOKEN")
        .ok()
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty());

    match transport.as_str() {
        "stdio" => {
            let service = FormaMcp::new(ServerClient::new(base_url))
                .with_stdio_credential(token)
                .serve(stdio())
                .await
                .context("failed to start the MCP stdio service")?;

            service.waiting().await?;
        }
        "http" => {
            // Behind a reverse proxy the inbound Host is the public name, and
            // rmcp validates it. The deployment names itself here.
            let public_host = std::env::var("FORMA_DOMAIN")
                .ok()
                .map(|host| host.trim().to_string())
                .filter(|host| !host.is_empty());
            http::serve(&listen, base_url, token, public_host).await?
        }
        other => anyhow::bail!("unknown transport {other:?}; use stdio or http"),
    }

    Ok(())
}
