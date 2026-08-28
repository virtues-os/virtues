//! The pairing handoff — a paired laptop enrolls a phone that cannot reach the
//! box, and hands it the finished identity in one QR.
//!
//! The sibling of [`crate::pair_door`], and the answer to the network the door
//! could not survive. The door needs the phone to open a socket to the laptop,
//! which coworking and hotel wifi routinely forbid (client isolation: every
//! device reaches the internet, none reach each other). This needs no network
//! between them at all — only a camera pointed at a screen.
//!
//! ```text
//!   laptop (paired, anywhere)                        box (at home)
//!     1. mint a fresh iroh identity FOR the phone
//!     2. POST /api/devices/enroll-peer ──over relay──► allowlist the pubkey
//!        (authenticated as the laptop)               ◄── reach info back
//!     3. display ONE QR: seed + that reach info
//!   phone
//!     4. scan it, write the record, dial the box over the relay ✓
//! ```
//!
//! Nothing is invented here. `enroll_peer` already existed (routed, and until
//! now with zero callers), `mint_identity` already minted seeds, and
//! `finish_consume` already turned a response plus an identity into a stored
//! `PairedBox` — and `enroll_peer` answers with exactly the fields it parses.
//! This module is the wiring and the payload format.
//!
//! # The trade, stated where it cannot be missed
//!
//! **The QR carries a private key.** Whoever photographs the laptop's screen
//! during the window gets a credential to the box. That is the cost of one
//! scan in the natural direction (phone at screen); the alternative — the
//! phone minting its own key and showing its PUBLIC half for the laptop to
//! scan — carries no secret but asks someone to hold a phone up to a webcam.
//!
//! It is the same exposure as a WhatsApp Web QR, and it is bounded the same
//! way: a short window, the seed never written to disk on the laptop (minted,
//! displayed, dropped), the device visible and revocable in Devices the
//! instant it is enrolled, and the QR withdrawn as soon as the phone appears.
//! What it must NEVER become is a QR that lives on a wall or in a screenshot.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::pair::MintedIdentity;

/// What travels in the QR. Versioned because it crosses a device boundary and
/// the two ends update independently — a phone on an older build must be able
/// to say "I don't understand this" instead of installing a half-record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffPayload {
    /// Format version. Bump on any field change that isn't purely additive.
    pub v: u8,
    /// The phone's iroh seed, minted by the laptop. The secret.
    pub seed: String,
    /// The box's answer to `enroll-peer`, verbatim — `device_id`,
    /// `box_node_id`, `relay_url`, `box_direct_addrs`, `applet_ids`. Kept as
    /// the raw JSON string so `finish_consume` parses the box's own words
    /// rather than a re-encoding of them.
    pub box_json: String,
}

pub const HANDOFF_VERSION: u8 = 1;

impl HandoffPayload {
    pub fn new(identity: &MintedIdentity, box_json: String) -> Self {
        Self {
            v: HANDOFF_VERSION,
            seed: identity.secret_hex_for_handoff().to_string(),
            box_json,
        }
    }

    /// Compact JSON — this goes in a QR, where every byte is a module.
    pub fn encode(&self) -> Result<String> {
        serde_json::to_string(self).context("encode handoff payload")
    }

    pub fn decode(s: &str) -> Result<Self> {
        let p: HandoffPayload = serde_json::from_str(s.trim()).context("decode handoff payload")?;
        if p.v != HANDOFF_VERSION {
            return Err(anyhow!(
                "this pairing code was made by a different version of Virtues (format {}, this app reads {}) — update both and try again",
                p.v,
                HANDOFF_VERSION
            ));
        }
        if p.seed.trim().is_empty() || p.box_json.trim().is_empty() {
            return Err(anyhow!("pairing code is incomplete"));
        }
        Ok(p)
    }

    /// Rebuild the identity this payload carries. The node id is derived from
    /// the seed, never taken on trust — see `from_handoff_secret`.
    pub fn identity(&self) -> Result<MintedIdentity> {
        MintedIdentity::from_handoff_secret(&self.seed)
    }
}

/// The request body for `POST /api/devices/enroll-peer`.
#[derive(Debug, Serialize)]
pub struct EnrollPeerRequest<'a> {
    pub peer_node_id: &'a str,
    pub kind: &'a str,
    pub label: Option<&'a str>,
    pub device_info: Option<serde_json::Value>,
}

/// Build the raw HTTP request that enrolls `node_id` with the box.
///
/// Raw bytes rather than a client call because the only route to the box from
/// here is the warm iroh client's `request`, which speaks HTTP/1 over a
/// bi-stream. Split out so it can be unit-tested without a box.
pub fn enroll_request(node_id: &str, kind: &str, label: Option<&str>) -> Result<Vec<u8>> {
    let body = serde_json::to_vec(&EnrollPeerRequest {
        peer_node_id: node_id,
        kind,
        label,
        device_info: None,
    })
    .context("encode enroll-peer body")?;

    let mut raw = Vec::with_capacity(body.len() + 160);
    raw.extend_from_slice(b"POST /api/devices/enroll-peer HTTP/1.1\r\n");
    raw.extend_from_slice(b"Host: virtues\r\n");
    raw.extend_from_slice(b"Content-Type: application/json\r\n");
    raw.extend_from_slice(format!("Content-Length: {}\r\n", body.len()).as_bytes());
    raw.extend_from_slice(b"Connection: close\r\n\r\n");
    raw.extend_from_slice(&body);
    Ok(raw)
}

/// Split a raw HTTP/1 response into (status, body).
pub fn split_response(resp: &[u8]) -> Result<(u16, String)> {
    let split = resp
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| anyhow!("malformed response from box (no header terminator)"))?;
    let head = String::from_utf8_lossy(&resp[..split]);
    let status = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse::<u16>().ok())
        .ok_or_else(|| anyhow!("malformed status line from box"))?;
    Ok((status, String::from_utf8_lossy(&resp[split + 4..]).to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_round_trips() {
        let id = crate::pair::mint_identity();
        let p = HandoffPayload::new(&id, r#"{"device_id":"dev_1"}"#.to_string());
        let decoded = HandoffPayload::decode(&p.encode().unwrap()).unwrap();
        assert_eq!(decoded.box_json, r#"{"device_id":"dev_1"}"#);
        // The identity must survive the trip intact — same public key out as in.
        assert_eq!(decoded.identity().unwrap().node_id, id.node_id);
    }

    #[test]
    fn a_future_format_is_refused_in_words_a_person_can_act_on() {
        let mut p = HandoffPayload::new(&crate::pair::mint_identity(), "{}".to_string());
        p.v = 99;
        let err = HandoffPayload::decode(&p.encode().unwrap()).unwrap_err().to_string();
        assert!(err.contains("different version"), "{err}");
        assert!(err.contains("update both"), "{err}");
    }

    #[test]
    fn an_incomplete_payload_is_refused() {
        let mut p = HandoffPayload::new(&crate::pair::mint_identity(), "{}".to_string());
        p.seed = String::new();
        assert!(HandoffPayload::decode(&p.encode().unwrap()).is_err());
    }

    #[test]
    fn the_node_id_is_derived_not_trusted() {
        // A payload whose seed is replaced yields a DIFFERENT node id — proving
        // the public key is computed from the secret rather than carried, so a
        // tampered payload cannot install a record that dials as someone else.
        let a = crate::pair::mint_identity();
        let b = crate::pair::mint_identity();
        let mut p = HandoffPayload::new(&a, "{}".to_string());
        p.seed = b.secret_hex_for_handoff().to_string();
        assert_eq!(p.identity().unwrap().node_id, b.node_id);
        assert_ne!(p.identity().unwrap().node_id, a.node_id);
    }

    #[test]
    fn enroll_request_is_well_formed_and_carries_the_key() {
        let raw = enroll_request("abc123", "mobile_app", Some("Adam's iPhone")).unwrap();
        let s = String::from_utf8_lossy(&raw);
        assert!(s.starts_with("POST /api/devices/enroll-peer HTTP/1.1\r\n"));
        assert!(s.contains("abc123"));
        assert!(s.contains("mobile_app"));
        let len: usize = s
            .lines()
            .find_map(|l| l.strip_prefix("Content-Length: ")?.trim().parse().ok())
            .expect("content-length");
        let body = s.split("\r\n\r\n").nth(1).unwrap();
        assert_eq!(body.len(), len, "Content-Length must match the body");
    }

    #[test]
    fn split_response_reads_status_and_body() {
        let (status, body) =
            split_response(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}").unwrap();
        assert_eq!(status, 200);
        assert_eq!(body, "{}");
        assert!(split_response(b"no terminator here").is_err());
    }
}
