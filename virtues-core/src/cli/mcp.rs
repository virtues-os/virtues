//! `virtues mcp`: this box's MCP tools on stdio, for an AI agent.
//!
//! The same bridge the Mac app runs for an agent on the owner's Mac
//! (`virtues_reach_client::mcp`), carried to the server on this machine
//! through its console socket instead of over iroh. The server admits the
//! caller by its uid (`crate::console_socket`). An agent on another machine
//! runs it over SSH with an agent key, whose forced command (`agent-exec`)
//! allows the `mcp` verb and passes the key's name, which the server's audit
//! line records. agents/plan/mcp-plan.md, tier 1.
//!
//! Where there is no socket (a developer's checkout) it uses loopback TCP,
//! which that server takes as the owner under `ENVIRONMENT=dev`.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use virtues_reach_client::mcp::{serve as relay, Forwarder, Loopback};

use crate::server::api::console::AGENT_KEY_HEADER;

/// Relays stdio to `/api/mcp` until stdin closes. `agent_key` is the agent
/// key the call came in on, when `agent-exec` runs it.
pub async fn serve(agent_key: Option<String>) -> Result<(), String> {
    let headers = agent_key.map(|key| vec![(AGENT_KEY_HEADER.to_string(), key)]).unwrap_or_default();
    let socket = crate::console_socket::path();
    let carrier: Arc<dyn Forwarder> = if socket.exists() {
        Arc::new(ConsoleSocket(socket))
    } else {
        Arc::new(Loopback::new(SocketAddr::from((Ipv4Addr::LOCALHOST, super::types::default_port()))))
    };
    relay(carrier, headers).await.map_err(|e| format!("{e:#}"))
}

/// The server's console socket, one connection per request. The relay
/// writes whole requests that ask the server to close the connection, so
/// the answer is everything read until it does.
struct ConsoleSocket(std::path::PathBuf);

impl Forwarder for ConsoleSocket {
    fn forward(
        &self,
        raw: Vec<u8>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Vec<u8>>> + Send>> {
        let path = self.0.clone();
        Box::pin(async move {
            use anyhow::Context;
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut stream = tokio::net::UnixStream::connect(&path)
                .await
                .with_context(|| format!("connect to the server at {}; is it running?", path.display()))?;
            stream.write_all(&raw).await?;
            let mut response = Vec::new();
            stream.read_to_end(&mut response).await?;
            if response.is_empty() {
                anyhow::bail!(
                    "the server refused this user at {}. Run this as a login in the {} group, or with sudo",
                    path.display(),
                    crate::console_socket::CONSOLE_GROUP
                );
            }
            Ok(response)
        })
    }
}
