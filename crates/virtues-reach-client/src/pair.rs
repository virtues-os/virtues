//! Pair flow — consume a one-time pair token from the box.
//!
//! The device generates its own iroh key, submits its EndpointId (so the box
//! allowlists it), and gets back the box's reach ticket (`{box_node_id,
//! relay_url, box_direct_addrs}`) + this device's `device_id`. We persist a
//! [`PairedBox`] via the caller's [`BoxStore`]. Consume itself is plain HTTP to
//! the box's LAN origin — the device isn't allowlisted yet, so it can't use
//! iroh until after this succeeds.
//!
//! Field names on the wire are fixed by `virtues-core::api::pair` — keep in sync.

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;

use crate::model::PairedBox;
use crate::store::BoxStore;

/// Body the box's `POST /api/pair/consume` accepts.
#[derive(Debug, Serialize)]
struct ConsumeRequest {
    /// The raw pair token (the 6-digit code, whitespace stripped).
    token: String,
    /// Device kind — determines the device label + per-credential action fan-out.
    /// e.g. `"desktop_app"` or `"mobile_app"`.
    kind: String,
    /// Free-form device metadata for the Devices page.
    device_info: serde_json::Value,
    /// This device's iroh EndpointId (hex) — allowlisted by the box. THIS is the
    /// credential; no bearer.
    device_node_id: String,
}

/// Response from `POST /api/pair/consume`.
#[derive(Debug, Deserialize)]
struct ConsumeResponse {
    #[serde(default)]
    device_id: Option<String>,
    #[serde(default)]
    box_node_id: Option<String>,
    #[serde(default)]
    relay_url: Option<String>,
    #[serde(default)]
    box_direct_addrs: Vec<String>,
    /// Device-anchored webhook mapping, e.g. `{"ios_ingest": "act_…"}`.
    #[serde(default)]
    applet_ids: std::collections::HashMap<String, String>,
}

/// A device identity minted for a pairing whose consume exchange rides another
/// transport (the box's BLE RPC 0x83): the `node_id` travels to the box for
/// allowlisting; the secret stays here until [`finish_consume`] persists it
/// alongside the box's response.
pub struct MintedIdentity {
    pub node_id: String,
    secret_hex: String,
}

impl MintedIdentity {
    /// The raw seed, for the ONE case that has to move an identity off the
    /// machine that minted it: the pairing handoff, where a paired laptop
    /// mints a phone's identity, enrolls its public half with the box, and
    /// hands the seed to the phone in a QR (see `pair_door`'s sibling flow).
    ///
    /// Deliberately narrow and deliberately named. Everywhere else the seed
    /// stays inside this type on the device that generated it — which is why
    /// the field is private and this is a method you have to reach for.
    pub fn secret_hex_for_handoff(&self) -> &str {
        &self.secret_hex
    }

    /// Rebuild an identity from a seed handed over by a paired device. The
    /// node id is DERIVED, never carried: a payload claiming a public key that
    /// doesn't match its seed would otherwise install a record that can never
    /// dial anything, failing later and somewhere else.
    pub fn from_handoff_secret(secret_hex: &str) -> Result<Self> {
        let bytes = hex::decode(secret_hex.trim()).context("decode handoff seed")?;
        let seed: [u8; 32] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| anyhow!("handoff seed must be 32 bytes, got {}", bytes.len()))?;
        Ok(MintedIdentity {
            node_id: virtues_iroh::SecretKey::from_bytes(&seed).public().to_string(),
            secret_hex: hex::encode(seed),
        })
    }
}

/// Mint a fresh iroh identity for this device — the same generation
/// [`consume`] performs inline for the HTTP path.
pub fn mint_identity() -> MintedIdentity {
    let mut seed = [0u8; 32];
    {
        use rand::RngCore;
        rand::rng().fill_bytes(&mut seed);
    }
    MintedIdentity {
        node_id: virtues_iroh::SecretKey::from_bytes(&seed).public().to_string(),
        secret_hex: hex::encode(seed),
    }
}

/// Complete a pairing whose consume exchange the box carried itself (BLE RPC
/// 0x83 relays the code to the box's own consume endpoint and streams the
/// response back). Parses that response and persists the same [`PairedBox`]
/// the HTTP path stores.
///
/// `box_url` is derived rather than known: over BLE there is no origin the
/// phone dialed. The box's first IPv4 direct address stands in (LAN HTTP is
/// only the fallback path — iroh is the real reach), with the mDNS default
/// when the box reported none.
pub fn finish_consume(
    store: &dyn BoxStore,
    response_json: &str,
    identity: MintedIdentity,
) -> Result<PairedBox> {
    let parsed: ConsumeResponse =
        serde_json::from_str(response_json).context("decode BLE-relayed consume response")?;
    let box_url = parsed
        .box_direct_addrs
        .iter()
        .filter_map(|a| a.parse::<std::net::SocketAddr>().ok())
        .find(|a| a.is_ipv4())
        .map(|a| format!("http://{}:8000", a.ip()))
        .unwrap_or_else(|| "http://virtues.local:8000".to_string());
    let rec = PairedBox {
        box_url,
        device_id: parsed.device_id,
        box_node_id: parsed.box_node_id.filter(|s| !s.is_empty()),
        relay_url: parsed.relay_url.filter(|s| !s.is_empty()),
        box_direct_addrs: parsed.box_direct_addrs,
        device_secret_hex: Some(identity.secret_hex),
        applet_ids: parsed.applet_ids,
    };
    store.save(&rec).context("store paired box")?;
    Ok(rec)
}

/// Consume a pair token against `origin` (e.g. `http://10.0.0.5:8000`).
///
/// Generates this device's iroh seed, submits its EndpointId, persists the
/// resulting [`PairedBox`] via `store`, refreshes reach if the box handed back
/// none, and returns the stored record. `kind` is the device kind string
/// (`"desktop_app"` / `"mobile_app"`); `device_info` is caller-supplied metadata.
pub async fn consume(
    store: &dyn BoxStore,
    origin: &str,
    token: &str,
    kind: &str,
    device_info: serde_json::Value,
) -> Result<PairedBox> {
    let origin = origin.trim_end_matches('/').to_string();
    let token: String = token.chars().filter(|c| !c.is_whitespace()).collect();

    // Generate this device's iroh identity. EndpointId → box (allowlisted); the
    // secret is persisted so the reach layer can build its endpoint.
    let mut seed = [0u8; 32];
    {
        use rand::RngCore;
        rand::rng().fill_bytes(&mut seed);
    }
    let device_secret_hex = hex::encode(seed);
    let device_node_id = virtues_iroh::SecretKey::from_bytes(&seed).public().to_string();

    let body = ConsumeRequest {
        token,
        kind: kind.to_string(),
        device_info,
        device_node_id,
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    let url = format!("{origin}/api/pair/consume");
    // Retry ONCE on a transport error (lost response). Idempotent enough: a
    // second consume of an already-consumed token just fails and the user
    // re-pairs with a fresh code.
    let resp = match client.post(&url).json(&body).send().await {
        Ok(r) => r,
        Err(_) => client
            .post(&url)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("POST {url}"))?,
    };

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        bail!("pair failed: {status} — {text}");
    }

    let parsed: ConsumeResponse = resp
        .json()
        .await
        .context("decode /api/pair/consume response")?;

    let rec = PairedBox {
        box_url: origin,
        device_id: parsed.device_id,
        box_node_id: parsed.box_node_id.filter(|s| !s.is_empty()),
        relay_url: parsed.relay_url.filter(|s| !s.is_empty()),
        box_direct_addrs: parsed.box_direct_addrs,
        device_secret_hex: Some(device_secret_hex),
        applet_ids: parsed.applet_ids,
    };
    store.save(&rec).context("store paired box")?;

    // If the box handed us no reach at all (its endpoint wasn't up yet at
    // consume time), pick it up now so we're not stuck until the next launch.
    // Having direct addrs is enough for same-network use.
    if rec.box_node_id.is_none() || (rec.relay_url.is_none() && rec.box_direct_addrs.is_empty()) {
        let _ = refresh_reach(store).await;
        if let Ok(Some(refreshed)) = store.load() {
            return Ok(refreshed);
        }
    }
    Ok(rec)
}

#[derive(serde::Deserialize)]
struct SelfReach {
    #[serde(default)]
    box_node_id: Option<String>,
    #[serde(default)]
    relay_url: Option<String>,
    #[serde(default)]
    box_direct_addrs: Vec<String>,
}

/// Refresh the box's iroh reach ticket from `GET /api/devices/self/reach`.
///
/// Devices freeze the ticket at pair time; if the box had no relay reach then,
/// or the relay URL later changed, this picks up the current one. Best-effort +
/// idempotent: a no-op if we already have a full ticket, aren't paired, or the
/// box still isn't relay-ready.
pub async fn refresh_reach(store: &dyn BoxStore) -> Result<()> {
    let Some(mut rec) = store.load()? else {
        return Ok(());
    };
    if rec.box_node_id.is_some() && rec.relay_url.is_some() {
        return Ok(());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let url = format!("{}/api/devices/self/reach", rec.box_url.trim_end_matches('/'));
    // Anonymous: the reach ticket is the box's public address. No credential
    // needed to bootstrap the first iroh dial.
    let resp = match client.get(&url).send().await {
        Ok(r) if r.status().is_success() => r,
        _ => return Ok(()), // box unreachable or not relay-ready yet
    };
    let reach: SelfReach = match resp.json().await {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    let node = reach.box_node_id.filter(|s| !s.is_empty());
    let relay = reach.relay_url.filter(|s| !s.is_empty());
    let direct = reach.box_direct_addrs;
    // Enough to reach the box if we have its node id AND at least one path.
    if node.is_some() && (relay.is_some() || !direct.is_empty()) {
        rec.box_node_id = node;
        rec.relay_url = relay;
        rec.box_direct_addrs = direct;
        store.save(&rec).context("persist refreshed reach ticket")?;
        tracing::info!("refreshed iroh reach from the box");
    }
    Ok(())
}

/// Extract `(origin, token)` from a pair URL like
/// `http://10.0.0.5:8000/pair#t=<token>`. The token lives in the fragment so
/// browser-side JS can read it without ever sending it to a referer.
pub fn parse_pair_url(s: &str) -> Result<(String, String)> {
    let u = Url::parse(s).context("invalid pair URL")?;

    let origin = match (u.scheme(), u.host_str(), u.port_or_known_default()) {
        ("http" | "https", Some(host), Some(port)) => format!("{}://{host}:{port}", u.scheme()),
        _ => bail!("pair URL must be http(s)://host[:port]/pair#t=..."),
    };

    let frag = u
        .fragment()
        .ok_or_else(|| anyhow!("pair URL has no fragment — token must be in `#t=<token>`"))?;
    let params: HashMap<_, _> = frag
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .collect();
    let token = params
        .get("t")
        .ok_or_else(|| anyhow!("pair URL fragment has no `t=` token"))?
        .to_string();
    if token.is_empty() {
        bail!("pair URL token is empty");
    }

    Ok((origin, token))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_localhost_url() {
        let (origin, token) = parse_pair_url("http://localhost:8000/pair#t=abc123&ep=foo").unwrap();
        assert_eq!(origin, "http://localhost:8000");
        assert_eq!(token, "abc123");
    }

    #[test]
    fn parses_ip_literal_url() {
        let (origin, token) = parse_pair_url("http://10.0.0.5:8000/pair#t=xyz").unwrap();
        assert_eq!(origin, "http://10.0.0.5:8000");
        assert_eq!(token, "xyz");
    }

    #[test]
    fn requires_fragment() {
        assert!(parse_pair_url("http://localhost:8000/pair").is_err());
    }

    #[test]
    fn requires_token_param() {
        assert!(parse_pair_url("http://localhost:8000/pair#ep=foo").is_err());
    }

    #[test]
    fn rejects_empty_token() {
        assert!(parse_pair_url("http://localhost:8000/pair#t=").is_err());
    }

    #[test]
    fn handles_https() {
        let (origin, _) = parse_pair_url("https://example.com/pair#t=abc").unwrap();
        assert_eq!(origin, "https://example.com:443");
    }
}
