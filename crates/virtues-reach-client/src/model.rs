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
    /// Device-anchored webhook applet ids from consume, e.g.
    /// `{"ios_ingest": "applet_…"}`. The upload coordinator POSTs collector
    /// batches to `/webhook/{applet_id}`. Empty on the desktop (it only proxies).
    ///
    /// `alias = "action_ids"` is not decoration — it is a data migration.
    /// `e174f130` renamed this field on 2026-07-30 ("drop the deprecation shims
    /// — break it properly"), reasoning that nothing pre-launch needed
    /// humouring. That was true of the wire and false of every pairing record
    /// already written to a Keychain: `serde(default)` turned the missing key
    /// into an empty map, silently. The device stayed paired, the link stayed
    /// active, and every upload aborted with "no ingest action id in pairing"
    /// while the outbox filled — nearly 500 records on one phone before anyone
    /// noticed. No compiler or test catches this; a defaulted rename of a
    /// PERSISTED field is invisible by construction.
    ///
    /// Keep the alias. Renaming a stored field costs one line to carry the old
    /// records across, and a re-pair from every device if you skip it.
    #[serde(default, alias = "action_ids")]
    pub applet_ids: std::collections::HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pairing record written before the 2026-07-30 rename must still
    /// deserialize with its ingest ids intact. Without the alias this silently
    /// yields an empty map — the device stays paired and uploads nothing.
    #[test]
    fn legacy_action_ids_still_load() {
        let legacy = r#"{
            "box_url": "http://10.0.0.5:8000",
            "action_ids": {"ios_ingest": "applet_ios_ingest_dev_abc"}
        }"#;
        let rec: PairedBox = serde_json::from_str(legacy).expect("legacy record");
        assert_eq!(
            rec.applet_ids.get("ios_ingest").map(String::as_str),
            Some("applet_ios_ingest_dev_abc"),
            "the alias is what keeps an existing pairing working"
        );
    }

    #[test]
    fn current_applet_ids_load() {
        let current = r#"{
            "box_url": "http://10.0.0.5:8000",
            "applet_ids": {"ios_ingest": "applet_x"}
        }"#;
        let rec: PairedBox = serde_json::from_str(current).expect("current record");
        assert_eq!(rec.applet_ids.get("ios_ingest").map(String::as_str), Some("applet_x"));
    }
}
