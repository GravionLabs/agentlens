//! `agentlens-mcp`: the AgentLens analyses as an MCP server on stdio. The client starts the process and
//! talks JSON-RPC over stdin and stdout; nothing listens on a port and nothing is sent anywhere.

use agentlens_mcp::AgentLensServer;
use anyhow::{Context, Result};
use rmcp::{ServiceExt, transport::stdio};

const HELP: &str = "agentlens-mcp: the AgentLens analyses as a read-only MCP server (stdio).

Usage: agentlens-mcp [--version | --help]

Started by an MCP client, it reads the session files of the current user and answers tool calls on
stdin and stdout. `agentlens mcp-setup` prints how to register it in a client.";

#[tokio::main]
async fn main() -> Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("--version" | "-V") => {
            println!("agentlens-mcp {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--help" | "-h") => {
            println!("{HELP}");
            return Ok(());
        }
        Some(other) => anyhow::bail!("unknown argument `{other}`; try --help"),
        None => {}
    }
    let server = AgentLensServer::system().map_err(anyhow::Error::msg)?;
    let service = server
        .serve(stdio())
        .await
        .context("cannot start the MCP server")?;
    service.waiting().await.context("the MCP server stopped")?;
    Ok(())
}
