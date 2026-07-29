//! The persisted paired-box record — everything a thin client needs to reach
//! and authorize the box. Storage is host-specific (see [`crate::BoxStore`]);
//! this is just the shape.

use serde::{Deserialize, Serialize};

/// The persisted paired-box record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairedBox {
    /// LAN origin we paired against, e.g. `http://10.0.0.5:8000` — the reach
    /// fallback for a LAN-only box, and the host we re-resolve to dial by
    /// NodeId. Remote reach goes over iroh (see `box_node_id`/`relay_url`).
    pub box_url: String,
    /// This device's `app_device.id` — sent to `DELETE /api/devices/:id` to
    /// self-revoke. `None` for legacy pairings.
    #[serde(default)]
    pub device_id: Option<String>,
    /// The box's iroh **EndpointId** (hex) — dialed over iroh. `None` on a
    /// LAN-only box (no relay reach).
    #[serde(default)]
    pub box_node_id: Option<String>,
    /// The relay URL to reach `box_node_id` through. Paired with it as the ticket.
    #[serde(default)]
    pub relay_url: Option<String>,
    /// The box's iroh direct socket addresses (LAN/VPN `IP:port`). On the same
    /// network we dial these directly — no relay, no discovery, no third party.
    #[serde(default)]
    pub box_direct_addrs: Vec<String>,
    /// This device's own iroh secret key (hex 32-byte seed), generated at
    /// pairing. Its EndpointId is submitted to the box so it's allowlisted; the
    /// reach layer builds its iroh endpoint from this. `None` for legacy pairings.
    #[serde(default)]
    pub device_secret_hex: Option<String>,
    /// Device-anchored webhook action ids from consume, e.g.
    /// `{"ios_ingest": "act_…"}`. The upload coordinator POSTs collector batches
    /// to `/webhook/{applet_id}`. Empty on the desktop (it only proxies).
    #[serde(default)]
    pub action_ids: std::collections::HashMap<String, String>,
}
