//! Pair flow — consume a one-time pair URL from the box.
//!
//! Today the pair URL the box prints looks like:
//!
//! ```text
//! http://localhost:8000/pair#t=<token>
//! ```
//!
//! …when run on the box itself, or `http://<box-ip>:8000/pair#t=<token>` when
//! pair tokens are minted for a remote device. The fragment portion (`#...`) is
//! NOT sent to the server by browsers, so the box web UI extracts it client-side
//! and POSTs it to `/api/pair/consume`. We do the same here — parse the URL,
//! pluck the fragment params, and POST.
//!
//! The pair flow on the box mints a credential row + (on Linux) a WG peer +
//! returns the full [`virtues_protocol::PairingBundle`]. We persist that bundle
//! in the OS keychain and we're done.

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;
use virtues_protocol::PairingBundle;

use crate::keychain;
use virtues_tunnel::generate_keypair;

/// The box's SPKI fingerprint (`sha256-<base64>`) derived from the WG server
/// public key in the bundle — the value compared against the out-of-band QR
/// fingerprint and the TOFU pin.
fn bundle_spki(bundle: &PairingBundle) -> Result<String> {
    use base64::Engine as _;
    let raw = base64::engine::general_purpose::STANDARD
        .decode(bundle.wg.server_public_key.trim())
        .context("decode bundle server_public_key")?;
    let arr: [u8; 32] = raw
        .as_slice()
        .try_into()
        .map_err(|_| anyhow!("bundle server_public_key is not 32 bytes"))?;
    Ok(virtues_protocol::spki_fingerprint(&arr))
}

/// Body the box's `/api/pair/consume` endpoint accepts. The shape is fixed by
/// `virtues-core::api::pair::ConsumeRequest` — keep these field names in sync.
#[derive(Debug, Serialize)]
struct ConsumeRequest {
    /// The raw pair token from the URL fragment.
    token: String,
    /// What kind of device is consuming. Determines the device label, default
    /// `allowed_ips`, and whether a WG peer is provisioned.
    kind: &'static str,
    /// Optional device_info JSON (free-form metadata). We send hostname + OS
    /// so the box's Devices page shows something useful.
    device_info: serde_json::Value,
    /// The client's freshly-minted WG public key, base64. The box installs
    /// this as a peer at pair time; the matching private key stays in this
    /// machine's OS keychain.
    wg_public_key: String,
}

/// Response the box returns from `/api/pair/consume`. The box puts the bundle
/// behind a `bundle: Option<PairingBundle>` field (None when WG isn't applicable,
/// e.g. on the macOS dev box where the WG engine is Linux-only). For the
/// desktop_app kind in production we always expect Some.
#[derive(Debug, Deserialize)]
struct ConsumeResponse {
    bundle: Option<PairingBundle>,
    /// The revocable credential row id. This — not the device id — is what
    /// `revoke` sends to `DELETE /api/credentials/:id`. Absent for browser
    /// pairings (cookie sessions have no revocable row id). Other response
    /// fields (`device_id`, `bearer`, `action_ids`) are ignored by this client.
    #[serde(default)]
    credential_id: Option<String>,
}

pub async fn run(pair_url: &str) -> Result<()> {
    let (origin, token, fpr) = parse_pair_url(pair_url)?;

    if keychain::load_bundle()?.is_some() {
        eprintln!("warning: this machine is already paired. Pairing again will");
        eprintln!("         overwrite the existing creds. To unpair first, run");
        eprintln!("         `virtues-client revoke`.");
    }

    eprintln!("pairing with {origin} …");
    consume(origin, token, fpr).await
}

/// Pair using a short 6-character display code (e.g. "ABC DEF") instead of a
/// full URL. The server origin is either discovered via mDNS or supplied by
/// the caller.
pub async fn run_with_code(server_origin: &str, code: &str) -> Result<()> {
    // Strip spaces so "ABC DEF" and "ABCDEF" both work.
    let token = code.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let origin = server_origin.trim_end_matches('/').to_string();

    if keychain::load_bundle()?.is_some() {
        eprintln!("warning: this machine is already paired. Pairing again will");
        eprintln!("         overwrite the existing creds. To unpair first, run");
        eprintln!("         `virtues-client revoke`.");
    }

    eprintln!("pairing with {origin} using code {code} …");
    // A bare display code carries no out-of-band fingerprint, so verification
    // falls back to TOFU (pin first-seen, verify after) — same as iOS.
    consume(origin, token, None).await
}

async fn consume(origin: String, token: String, expected_fpr: Option<String>) -> Result<()> {
    let keypair = generate_keypair();
    let wg_public_b64 = keypair.public_key_b64.clone();

    let device_info = serde_json::json!({
        "device_name": hostname(),
        "os":          std::env::consts::OS,
        "arch":        std::env::consts::ARCH,
        "client":      "virtues-client",
        "version":     env!("CARGO_PKG_VERSION"),
    });

    let body = ConsumeRequest {
        token,
        kind: "desktop_app",
        device_info,
        wg_public_key: wg_public_b64.clone(),
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    let url = format!("{origin}/api/pair/consume");
    let resp = client.post(&url).json(&body).send().await
        .with_context(|| format!("POST {url}"))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        bail!("pair failed: {status} — {text}");
    }

    let parsed: ConsumeResponse = resp.json().await
        .context("decode /api/pair/consume response")?;

    let bundle = parsed.bundle.ok_or_else(|| {
        anyhow!(
            "server returned no PairingBundle — is the server running on Linux? \
             The WG engine is Linux-only; macOS dev servers can pair but won't \
             return a bundle until they're moved to a real appliance."
        )
    })?;

    // ── SPKI verification (mirror iOS `verifyAndStoreBundle`) ──────────────
    // The bundle's WG server key arrived over spoofable HTTP; the WG Noise
    // handshake only proves we reached *whoever owns that key*. So before we
    // trust it: (1) if the QR carried an out-of-band fingerprint, the key must
    // hash to it; (2) TOFU — the key must match the one pinned at first pair.
    // Either mismatch aborts pairing (a different box, or a MITM) — we never
    // silently re-pin a changed identity.
    let actual_fpr = bundle_spki(&bundle)?;
    if let Some(expected) = expected_fpr.as_deref() {
        if actual_fpr != expected {
            bail!(
                "SPKI fingerprint mismatch — the box's key does not match the \
                 pairing code. Someone may be intercepting the connection.\n  \
                 expected: {expected}\n  got:      {actual_fpr}"
            );
        }
    }
    if let Some(pinned) = keychain::load_server_pin()? {
        if pinned != actual_fpr {
            bail!(
                "the box's identity key changed since this device first paired.\n  \
                 pinned: {pinned}\n  now:    {actual_fpr}\n\
                 If you reinstalled the box, run `virtues-client revoke` first; \
                 otherwise this may be an attack."
            );
        }
    }

    // Persist private key BEFORE the bundle. If the bundle write fails we want
    // a leftover private key in the keychain (harmless garbage we'll overwrite
    // on retry) — we do NOT want a stored bundle that references a private key
    // we threw away.
    keychain::save_wg_private(&keypair.private_key_b64)?;
    keychain::save_bundle(&bundle)?;
    // Pin the verified identity for TOFU on the next pair/reconnect.
    keychain::save_server_pin(&actual_fpr)?;

    // Save the credential ID — what revoke() sends to DELETE
    // /api/credentials/:id (a different id space from device_id).
    if let Some(ref id) = parsed.credential_id {
        keychain::save_credential_id(id)?;
    }

    // Write bundle to ~/.virtues/bundle.json so the root LaunchDaemon can read
    // it (the OS keychain is user-specific and unavailable to the root process).
    if let Err(e) = write_daemon_bundle(&bundle) {
        tracing::warn!("could not write daemon bundle file: {e}");
    }

    println!();
    println!("✓ paired with {origin}");
    println!("  server ID:     {}", &bundle.rendezvous.publish_id);
    println!("  server addr:   {}", &bundle.internal_ip);
    println!("  device pubkey: {wg_public_b64}");
    println!("  bundle stored: OS keychain (service = 'virtues-client')");
    println!();
    println!("next: run `virtues-client up` to start the local proxy.");

    Ok(())
}

/// Write the bundle to `~/.virtues/bundle.json` so the root LaunchDaemon can
/// read it. Holds the bearer + WG key material, so it must never be exposed.
///
/// The write is atomic and 0600 from creation: we write to a sibling temp file
/// opened with mode 0600 (so there is no world-readable window — unlike
/// write-then-chmod), then `rename` it into place (atomic on the same
/// filesystem, so a crash mid-write can't leave the daemon a truncated bundle).
/// Root bypasses the 0600 permission to read it.
fn write_daemon_bundle(bundle: &PairingBundle) -> Result<()> {
    let home = std::env::var("HOME").context("$HOME not set")?;
    let dir = std::path::PathBuf::from(home).join(".virtues");
    std::fs::create_dir_all(&dir).context("create ~/.virtues")?;
    let path = dir.join("bundle.json");
    let tmp = dir.join("bundle.json.tmp");
    let json = serde_json::to_string(bundle).context("serialize bundle")?;

    {
        use std::io::Write as _;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts
            .open(&tmp)
            .with_context(|| format!("create {}", tmp.display()))?;
        f.write_all(json.as_bytes())
            .with_context(|| format!("write {}", tmp.display()))?;
        f.sync_all().ok();
    }

    std::fs::rename(&tmp, &path)
        .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

/// Extract `(origin, token)` from a pair URL like
/// `http://10.0.0.5:8000/pair#t=<token>&ep=...&fpr=...`.
///
/// The token lives in the URL fragment so the browser-side JS can read it
/// without ever sending it to a third-party referer. We parse it the same way.
fn parse_pair_url(s: &str) -> Result<(String, String, Option<String>)> {
    let u = Url::parse(s).context("invalid pair URL")?;

    let origin = match (u.scheme(), u.host_str(), u.port_or_known_default()) {
        ("http" | "https", Some(host), Some(port)) => {
            format!("{}://{host}:{port}", u.scheme())
        }
        _ => bail!("pair URL must be http(s)://host[:port]/pair#t=..."),
    };

    let frag = u.fragment().ok_or_else(|| {
        anyhow!("pair URL has no fragment — token must be in `#t=<token>`")
    })?;
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

    // The box's SPKI fingerprint, embedded out-of-band in the QR so we can
    // verify the WG server key in the (HTTP-delivered, spoofable) bundle wasn't
    // substituted by a LAN MITM. Taken verbatim — it's `sha256-<base64>`, which
    // can contain `+`/`/`; percent-decoding would corrupt it. `f` is an alias
    // the iOS scanner also accepts. Optional (dev/pre-fpr boxes omit it).
    let fpr = params
        .get("fpr")
        .or_else(|| params.get("f"))
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());

    Ok((origin, token, fpr))
}

/// Best-effort machine hostname for the Devices page label.
///
/// `$HOSTNAME` is unset under systemd `--user` units (the daemon's main
/// deployment), so the env-var read alone produces "unknown" for most paired
/// devices. We layer fallbacks:
///   1. `$HOSTNAME` / `$COMPUTERNAME` if exported.
///   2. Kernel-reported name via `/proc/sys/kernel/hostname` (Linux).
///   3. `hostname` shell command (macOS, BSD, anywhere with the binary).
fn hostname() -> String {
    fn non_empty(s: String) -> Option<String> {
        let t = s.trim().to_string();
        if t.is_empty() { None } else { Some(t) }
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
        let (origin, token, fpr) =
            parse_pair_url("http://localhost:8000/pair#t=abc123&ep=foo").unwrap();
        assert_eq!(origin, "http://localhost:8000");
        assert_eq!(token, "abc123");
        assert_eq!(fpr, None);
    }

    #[test]
    fn parses_ip_literal_url() {
        let (origin, token, fpr) =
            parse_pair_url("http://10.0.0.5:8000/pair#t=xyz").unwrap();
        assert_eq!(origin, "http://10.0.0.5:8000");
        assert_eq!(token, "xyz");
        assert_eq!(fpr, None);
    }

    #[test]
    fn extracts_fpr_verbatim() {
        // The fingerprint is base64 and may contain `+`/`/`; it must come back
        // exactly as embedded (no percent-decoding).
        let (_, token, fpr) =
            parse_pair_url("http://localhost:8000/pair#t=abc&fpr=sha256-AbC+d/E").unwrap();
        assert_eq!(token, "abc");
        assert_eq!(fpr.as_deref(), Some("sha256-AbC+d/E"));
    }

    #[test]
    fn requires_fragment() {
        let err = parse_pair_url("http://localhost:8000/pair");
        assert!(err.is_err());
    }

    #[test]
    fn requires_token_param() {
        let err = parse_pair_url("http://localhost:8000/pair#ep=foo");
        assert!(err.is_err());
    }

    #[test]
    fn rejects_empty_token() {
        let err = parse_pair_url("http://localhost:8000/pair#t=");
        assert!(err.is_err());
    }

    #[test]
    fn handles_https() {
        let (origin, _, _) =
            parse_pair_url("https://example.com/pair#t=abc").unwrap();
        assert_eq!(origin, "https://example.com:443");
    }
}
