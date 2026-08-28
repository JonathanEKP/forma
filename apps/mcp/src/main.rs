//! `forma-mcp`: the MCP front end for Forma.
//!
//! It is a client of `forma-server`'s REST API, never of the database
//! (plan section 5). V1 speaks stdio; HTTP is reserved for the remote phase.

mod client;
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
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--transport" => {
                transport = args.next().context("--transport needs a value")?;
            }
            "--help" | "-h" => {
                println!("forma-mcp [--transport stdio]");
                println!();
                println!("Environment:");
                println!(
                    "  FORMA_SERVER_URL  base URL of forma-server (default http://127.0.0.1:4765)"
                );
                println!("  FORMA_LOG         log filter (default warn)");
                return Ok(());
            }
            other => anyhow::bail!("unknown argument: {other}"),
        }
    }

    if transport != "stdio" {
        // Plan section 56 reserves HTTP for the remote deploy phase.
        anyhow::bail!("transport {transport:?} is not implemented in V1; use --transport stdio");
    }

    // stdout carries the MCP protocol, so logs must go to stderr.
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(env_or("FORMA_LOG", "warn")))
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();

    let base_url = env_or("FORMA_SERVER_URL", "http://127.0.0.1:4765");
    let service = FormaMcp::new(ServerClient::new(base_url))
        .serve(stdio())
        .await
        .context("failed to start the MCP stdio service")?;

    service.waiting().await?;
    Ok(())
}
