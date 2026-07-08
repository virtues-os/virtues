//! Pair flow — CLI wrapper over `virtues_reach_client::pair`.
//!
//! The reach core (seed gen, `POST /api/pair/consume`, reach refresh, persist)
//! lives in `virtues-reach-client`; this file keeps the desktop CLI surface:
//! parse the pair URL / short code, warn on re-pair, and print the human-readable
//! result. Credential storage is the OS keychain via [`keychain::KeychainStore`].

use anyhow::{Context, Result};

use crate::keychain::{self, KeychainStore};

/// Pair by consuming a one-time pair URL like
/// `http://10.0.0.5:8000/pair#t=<token>`.
pub async fn run(pair_url: &str) -> Result<()> {
    let (origin, token) = virtues_reach_client::pair::parse_pair_url(pair_url)?;
    warn_if_paired();
    eprintln!("pairing with {origin} …");
    finish(origin, token).await
}

/// Pair using a short display code; `server_origin` is discovered or supplied.
pub async fn run_with_code(server_origin: &str, code: &str) -> Result<()> {
    let origin = server_origin.trim_end_matches('/').to_string();
    warn_if_paired();
    eprintln!("pairing with {origin} using code {code} …");
    finish(origin, code.to_string()).await
}

/// Refresh the box's iroh reach ticket (best-effort). Called on `up` startup.
pub async fn refresh_reach() -> Result<()> {
    virtues_reach_client::pair::refresh_reach(&KeychainStore).await
}

async fn finish(origin: String, token: String) -> Result<()> {
    let device_info = serde_json::json!({
        "device_name": hostname(),
        "os":          std::env::consts::OS,
        "arch":        std::env::consts::ARCH,
        "client":      "virtues-client",
        "version":     env!("CARGO_PKG_VERSION"),
    });

    let rec = virtues_reach_client::pair::consume(
        &KeychainStore,
        &origin,
        &token,
        "desktop_app",
        device_info,
    )
    .await
    .context("consume pair token")?;

    println!();
    println!("✓ paired with {origin}");
    match (&rec.box_node_id, &rec.relay_url, rec.box_direct_addrs.is_empty()) {
        (Some(n), Some(r), _) => {
            println!("  iroh reach:    {n} via {r} (+ {} direct)", rec.box_direct_addrs.len())
        }
        (Some(n), None, false) => println!(
            "  iroh reach:    {n} LAN-direct ({} addrs, no relay)",
            rec.box_direct_addrs.len()
        ),
        _ => println!("  reach:         LAN only ({})", rec.box_url),
    }
    println!("  creds stored:  OS keychain (service = 'virtues-client') + ~/.virtues/box.json");
    println!();
    println!("next: run `virtues-client up` to serve the box at http://localhost:7117");
    Ok(())
}

fn warn_if_paired() {
    if matches!(keychain::load_box(), Ok(Some(_))) {
        eprintln!("warning: this machine is already paired. Pairing again will");
        eprintln!("         overwrite the existing creds. To unpair first, run");
        eprintln!("         `virtues-client revoke`.");
    }
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
