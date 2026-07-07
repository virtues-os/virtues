//! Pair flow — consume a one-time pair URL from the box.
//!
//! The pair URL the box prints looks like:
//!
//! ```text
//! http://localhost:8000/pair#t=<token>
//! ```
//!
//! …when run on the box itself, or `http://<box-ip>:8000/pair#t=<token>` for a
//! remote device. The fragment (`#...`) is NOT sent to servers by browsers, so
//! the box web UI extracts it client-side and POSTs it to `/api/pair/consume`.
//! We do the same — parse the URL, pluck the token, and POST.
//!
//! In the iroh model this client generates its own device iroh key, submits its
//! EndpointId (so the box allowlists it), and gets back a bearer + the box's reach
//! ticket (`{box_node_id, relay_url}`). We persist a small
//! [`crate::keychain::PairedBox`]; `virtues-client up` then dials the box over
//! iroh and serves it to the browser on `localhost:7117`. There's no tunnel to
//! bring up and no long-lived transport keys beyond the device seed.

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;

use crate::keychain::{self, PairedBox};

/// Body the box's `/api/pair/consume` endpoint accepts. Field names are fixed by
/// `virtues-core::api::pair::ConsumeRequest` — keep them in sync.
#[derive(Debug, Serialize)]
struct ConsumeRequest {
    /// The raw pair token from the URL fragment.
    token: String,
    /// What kind of device is consuming — determines the device label + the
    /// per-credential action fan-out.
    kind: &'static str,
    /// Free-form device metadata; we send hostname + OS for the Devices page.
    device_info: serde_json::Value,
    /// This device's iroh EndpointId (hex) — the box allowlists it so the `:7117`
    /// helper can reach the box over iroh. This IS the credential; no bearer.
    device_node_id: String,
}

/// Response the box returns from `/api/pair/consume`: this device's id (for
/// self-revoke) + the box's iroh reach ticket. Auth is the allowlisted key, so
/// there's no bearer to carry back.
#[derive(Debug, Deserialize)]
struct ConsumeResponse {
    #[serde(default)]
    device_id: Option<String>,
    /// The box's iroh EndpointId (hex) — dialed by the helper. Absent on a
    /// LAN-only box.
    #[serde(default)]
    box_node_id: Option<String>,
    /// The relay URL to reach `box_node_id` through.
    #[serde(default)]
    relay_url: Option<String>,
    /// The box's iroh direct socket addresses (LAN/VPN) for zero-third-party
    /// reach on the same network.
    #[serde(default)]
    box_direct_addrs: Vec<String>,
}

pub async fn run(pair_url: &str) -> Result<()> {
    let (origin, token) = parse_pair_url(pair_url)?;
    warn_if_paired();
    eprintln!("pairing with {origin} …");
    consume(origin, token).await
}

/// Pair using a short display code instead of a full URL. The origin is either
/// discovered via mDNS or supplied by the caller.
pub async fn run_with_code(server_origin: &str, code: &str) -> Result<()> {
    let token = code.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let origin = server_origin.trim_end_matches('/').to_string();
    warn_if_paired();
    eprintln!("pairing with {origin} using code {code} …");
    consume(origin, token).await
}

fn warn_if_paired() {
    if matches!(keychain::load_box(), Ok(Some(_))) {
        eprintln!("warning: this machine is already paired. Pairing again will");
        eprintln!("         overwrite the existing creds. To unpair first, run");
        eprintln!("         `virtues-client revoke`.");
    }
}

async fn consume(origin: String, token: String) -> Result<()> {
    let device_info = serde_json::json!({
        "device_name": hostname(),
        "os":          std::env::consts::OS,
        "arch":        std::env::consts::ARCH,
        "client":      "virtues-client",
        "version":     env!("CARGO_PKG_VERSION"),
    });

    // Generate this device's iroh identity. Its EndpointId goes to the box (to be
    // allowlisted); the secret is persisted so the `:7117` helper can reach the
    // box over iroh.
    let mut seed = [0u8; 32];
    {
        use rand::RngCore;
        rand::rng().fill_bytes(&mut seed);
    }
    let device_secret_hex = hex::encode(seed);
    let device_node_id = virtues_iroh::SecretKey::from_bytes(&seed).public().to_string();

    let body = ConsumeRequest {
        token,
        kind: "desktop_app",
        device_info,
        device_node_id: device_node_id.clone(),
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    let url = format!("{origin}/api/pair/consume");
    // Retry ONCE on a transport error (lost response). Pairing is idempotent
    // enough: a second consume of an already-consumed token just fails and the
    // user re-pairs with a fresh code.
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

    // LAN fallback origin (the box's own :8000) for when there's no relay reach.
    let box_url = origin.clone();
    let box_node_id = parsed.box_node_id.filter(|s| !s.is_empty());
    let relay_url = parsed.relay_url.filter(|s| !s.is_empty());
    let direct = parsed.box_direct_addrs;

    let rec = PairedBox {
        box_url: box_url.clone(),
        device_id: parsed.device_id,
        box_node_id: box_node_id.clone(),
        relay_url: relay_url.clone(),
        box_direct_addrs: direct.clone(),
        device_secret_hex: Some(device_secret_hex),
    };
    keychain::save_box(&rec).context("store paired box")?;

    // If the box handed us no reach at all (no relay AND no direct addrs — its
    // endpoint wasn't up yet at consume time), pick it up now so we're not stuck
    // until the next launch. Having direct addrs is enough for same-network use.
    let (box_node_id, relay_url, direct) =
        if box_node_id.is_none() || (relay_url.is_none() && direct.is_empty()) {
            let _ = refresh_reach().await;
            match keychain::load_box() {
                Ok(Some(r)) => (r.box_node_id, r.relay_url, r.box_direct_addrs),
                _ => (box_node_id, relay_url, direct),
            }
        } else {
            (box_node_id, relay_url, direct)
        };

    println!();
    println!("✓ paired with {origin}");
    match (&box_node_id, &relay_url, direct.is_empty()) {
        (Some(n), Some(r), _) => println!("  iroh reach:    {n} via {r} (+ {} direct)", direct.len()),
        (Some(n), None, false) => println!("  iroh reach:    {n} LAN-direct ({} addrs, no relay)", direct.len()),
        _ => println!("  reach:         LAN only ({box_url})"),
    }
    println!("  creds stored:  OS keychain (service = 'virtues-client') + ~/.virtues/box.json");
    println!();
    println!("next: run `virtues-client up` to serve the box at http://localhost:7117");

    Ok(())
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
/// Devices freeze the ticket at pair time; if the box had no relay reach then (a
/// box claimed before the relay was live) or the relay URL later changed, this
/// picks up the current one. Best-effort + idempotent: a no-op if we already
/// have a ticket, aren't paired, or the box still isn't relay-ready. Called at
/// the end of `consume` and on `up` startup.
pub async fn refresh_reach() -> Result<()> {
    let Some(mut rec) = keychain::load_box()? else {
        return Ok(());
    };
    if rec.box_node_id.is_some() && rec.relay_url.is_some() {
        return Ok(());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let url = format!(
        "{}/api/devices/self/reach",
        rec.box_url.trim_end_matches('/')
    );
    // Anonymous: the reach ticket is the box's public address (see
    // get_self_reach). No credential needed to bootstrap the first iroh dial.
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
    // Enough to reach the box if we have its node id AND at least one path
    // (a relay for remote, or direct addrs for same-network).
    if node.is_some() && (relay.is_some() || !direct.is_empty()) {
        rec.box_node_id = node;
        rec.relay_url = relay;
        rec.box_direct_addrs = direct;
        keychain::save_box(&rec).context("persist refreshed reach ticket")?;
        eprintln!("↻ refreshed iroh reach from the box");
    }
    Ok(())
}

/// Extract `(origin, token)` from a pair URL like
/// `http://10.0.0.5:8000/pair#t=<token>`.
///
/// The token lives in the fragment so browser-side JS can read it without ever
/// sending it to a referer. We parse it the same way.
fn parse_pair_url(s: &str) -> Result<(String, String)> {
    let u = Url::parse(s).context("invalid pair URL")?;

    let origin = match (u.scheme(), u.host_str(), u.port_or_known_default()) {
        ("http" | "https", Some(host), Some(port)) => {
            format!("{}://{host}:{port}", u.scheme())
        }
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

/// Best-effort machine hostname for the Devices page label.
fn hostname() -> String {
    fn non_empty(s: String) -> Option<String> {
        let t = s.trim().to_string();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    }

    if let Some(h) = std::env::var("HOSTNAME").ok().and_then(non_empty) {
        return h;
    }
    if let Some(h) = std::env::var("COMPUTERNAME").ok().and_then(non_empty) {
        return h;
    }
    #[cfg(target_os = "linux")]
    {
        if let Some(h) = std::fs::read_to_string("/proc/sys/kernel/hostname")
            .ok()
            .and_then(non_empty)
        {
            return h;
        }
    }
    if let Ok(out) = std::process::Command::new("hostname").output() {
        if out.status.success() {
            if let Some(h) = non_empty(String::from_utf8_lossy(&out.stdout).into_owned()) {
                return h;
            }
        }
    }
    "unknown".to_string()
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
