//! Telling the box where it can reach this device.
//!
//! The device already has a KEY on the box — its iroh EndpointId, which it
//! proves on every connection. This is the other half of the pair: the
//! ADDRESS the box uses to reach the device, which on iOS is the APNs token.
//! The box stores it on the same `app_device` row, so revoking the device kills
//! both at once. See `agents/plan/reminders-plan.md`.

use anyhow::{Context, Result};
use serde::Serialize;

/// The body `POST /api/devices/self/push-address` accepts. `None` serializes as
/// `null`, which is the "I can no longer be reached" report — notifications
/// turned off, or registration failed — and is not an error on the box.
#[derive(Serialize)]
struct PushAddressBody<'a> {
    push_address: Option<&'a str>,
}

/// Build the raw HTTP request that reports this device's push address.
///
/// Raw bytes for the same reason as `handoff::enroll_request`: the only route
/// to the box from the phone is the warm iroh client's `request`, which speaks
/// HTTP/1 over a bi-stream. Split out so the wire shape is testable without a
/// box — this is a request the box authenticates by the proven iroh key, so a
/// malformed one fails as a 400 nobody on the phone would ever see.
pub fn push_address_request(address: Option<&str>) -> Result<Vec<u8>> {
    let body = serde_json::to_vec(&PushAddressBody {
        push_address: address,
    })
    .context("encode push-address body")?;

    let mut raw = Vec::with_capacity(body.len() + 160);
    raw.extend_from_slice(b"POST /api/devices/self/push-address HTTP/1.1\r\n");
    raw.extend_from_slice(b"Host: virtues\r\n");
    raw.extend_from_slice(b"Content-Type: application/json\r\n");
    raw.extend_from_slice(format!("Content-Length: {}\r\n", body.len()).as_bytes());
    raw.extend_from_slice(b"Connection: close\r\n\r\n");
    raw.extend_from_slice(&body);
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body_of(raw: &[u8]) -> serde_json::Value {
        let split = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        serde_json::from_slice(&raw[split + 4..]).unwrap()
    }

    #[test]
    fn an_address_is_reported_as_the_route_the_box_serves() {
        let raw = push_address_request(Some("a1b2c3")).unwrap();
        let s = String::from_utf8_lossy(&raw);
        assert!(s.starts_with("POST /api/devices/self/push-address HTTP/1.1\r\n"));
        assert_eq!(body_of(&raw)["push_address"], "a1b2c3");
    }

    /// Clearing must send an explicit `null`, not drop the field: the box reads
    /// absent-or-null as "cannot be reached" either way, but an explicit null is
    /// what says the phone MEANT it, and it is the report that keeps the box
    /// from holding a token that silently drops everything.
    #[test]
    fn unreachable_is_an_explicit_null() {
        let raw = push_address_request(None).unwrap();
        let body = body_of(&raw);
        assert!(body.get("push_address").is_some(), "the field must be present");
        assert!(body["push_address"].is_null());
    }

    #[test]
    fn content_length_matches_the_body() {
        let raw = push_address_request(Some("ff")).unwrap();
        let s = String::from_utf8_lossy(&raw);
        let len: usize = s
            .lines()
            .find_map(|l| l.strip_prefix("Content-Length: "))
            .unwrap()
            .parse()
            .unwrap();
        let split = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        assert_eq!(len, raw.len() - split - 4);
    }
}
