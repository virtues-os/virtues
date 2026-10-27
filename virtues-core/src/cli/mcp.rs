//! `virtues mcp`: this box's MCP tools on stdio, for an AI agent.
//!
//! The same bridge the Mac app runs for an agent on the owner's Mac
//! (`virtues_reach_client::mcp`), carried over loopback to the server on this
//! machine instead of over iroh. The server authenticates a loopback caller as
//! the local console. An agent on another machine runs it over SSH with an
//! agent key, whose forced command (`agent-exec`) allows the `mcp` verb and
//! passes the key's name, which the server's audit line records.
//! agents/plan/mcp-plan.md, tier 1.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use virtues_reach_client::mcp::{serve as relay, Loopback};

use crate::server::api::console::AGENT_KEY_HEADER;

/// Relays stdio to `/api/mcp` until stdin closes. `agent_key` is the agent
/// key the call came in on, when `agent-exec` runs it.
pub async fn serve(agent_key: Option<String>) -> Result<(), String> {
    let server = SocketAddr::from((Ipv4Addr::LOCALHOST, super::types::default_port()));
    let headers = agent_key.map(|key| vec![(AGENT_KEY_HEADER.to_string(), key)]).unwrap_or_default();
    relay(Arc::new(Loopback::new(server)), headers).await.map_err(|e| format!("{e:#}"))
}
