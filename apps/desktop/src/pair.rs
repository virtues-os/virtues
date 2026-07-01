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
//! In the relay model the box hands back a bearer + its canonical reachable URL
//! (`https://<boxhash>.boxes.virtues.com`); there's no tunnel to bring up and no
//! key material to install. We persist a small [`crate::keychain::PairedBox`] and
//! we're done — the browser reaches the box at that URL directly.

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
    /// helper can reach the box over iroh.
    device_node_id: String,
}

/// Response the box returns from `/api/pair/consume`. We need the bearer, the
/// revocable credential id, and the box's iroh reach ticket.
#[derive(Debug, Deserialize)]
struct ConsumeResponse {
    #[serde(default)]
    bearer: Option<String>,
    #[serde(default)]
    credential_id: Option<String>,
    /// The box's iroh EndpointId (hex) — dialed by the helper. Absent on a
    /// LAN-only box.
    #[serde(default)]
    box_node_id: Option<String>,
    /// The relay URL to reach `box_node_id` through.
    #[serde(default)]
    relay_url: Option<String>,
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
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .with_context(|| format!("POST {url}"))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        bail!("pair failed: {status} — {text}");
    }

    let parsed: ConsumeResponse = resp
        .json()
        .await
        .context("decode /api/pair/consume response")?;

    let bearer = parsed.bearer.ok_or_else(|| {
        anyhow!(
            "server returned no bearer — a desktop pairing must yield one. \
             (Browser pairings use a cookie session and have none.)"
        )
    })?;

    // LAN fallback origin (the box's own :8000) for when there's no relay reach.
    let box_url = origin.clone();
    let box_node_id = parsed.box_node_id.filter(|s| !s.is_empty());
    let relay_url = parsed.relay_url.filter(|s| !s.is_empty());

    let rec = PairedBox {
        box_url: box_url.clone(),
        bearer,
        credential_id: parsed.credential_id,
        box_node_id: box_node_id.clone(),
        relay_url: relay_url.clone(),
        device_secret_hex: Some(device_secret_hex),
    };
    keychain::save_box(&rec).context("store paired box")?;

    println!();
    println!("✓ paired with {origin}");
    match (&box_node_id, &relay_url) {
        (Some(n), Some(r)) => println!("  iroh reach:    {n} via {r}"),
        _ => println!("  reach:         LAN only ({box_url})"),
    }
    println!("  creds stored:  OS keychain (service = 'virtues-client') + ~/.virtues/box.json");
    println!();
    println!("next: run `virtues-client up` to serve the box at http://localhost:7117");

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
