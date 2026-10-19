//! LAN discovery by subnet scan — the mDNS-free path.
//!
//! mDNS/Bonjour is unreliable on iOS (deprecated NetServiceBrowser + local-network
//! permission timing) and some APs filter multicast entirely. So instead of
//! trusting multicast, we probe every host on our private /24 for the box's
//! pairing endpoint. A Virtues box answers `POST /api/pair/consume` with a
//! 4xx *validation* error (400/401/422); a random `:8000` server 404s or refuses.
//! Empty body → no side effects.

use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::sync::Semaphore;

/// A box found on the LAN. `origin` is an IP URL (no DNS needed to reach it).
#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredBox {
  /// Human label ("Quaint Tern") from `/api/box/identity`; a generic
  /// "Virtues box" only when that endpoint is absent (pre-codename builds).
  pub name: String,
  pub origin: String,
  /// Whether a device has already paired. `None` when unknowable (old box).
  /// The chips render "ready to set up" vs "already set up" off this — the
  /// distinction that matters when two boxes share a LAN.
  pub claimed: Option<bool>,
}

/// The box port. Matches the Avahi advertisement + desktop reach.
const BOX_PORT: u16 = 8000;
/// Per-host probe timeout — short so an unresponsive host doesn't stall the sweep.
const PROBE_MS: u64 = 600;
/// Probe most of a /24 at once so the sweep finishes in ~1-2s (dominated by the
/// timeout of non-responsive hosts, not their count).
const CONCURRENCY: usize = 256;

/// The device's private IPv4s, as strings — for a UI diagnostic line so we can
/// tell "no LAN IP (Local Network permission off / no Wi-Fi)" apart from
/// "scanned but found nothing".
pub fn local_private_ipv4s() -> Vec<String> {
  private_ipv4s().iter().map(|i| i.to_string()).collect()
}

fn private_ipv4s() -> Vec<Ipv4Addr> {
  local_ip_address::list_afinet_netifas()
    .map(|list| {
      list
        .into_iter()
        .filter_map(|(_, ip)| match ip {
          IpAddr::V4(v4) if v4.is_private() && !v4.is_loopback() => Some(v4),
          _ => None,
        })
        .collect()
    })
    .unwrap_or_default()
}

/// Scan the device's private /24(s) for Virtues boxes.
pub async fn scan_subnet() -> Vec<DiscoveredBox> {
  let mut subnets: HashSet<[u8; 3]> = HashSet::new();
  let mut self_addrs: HashSet<Ipv4Addr> = HashSet::new();
  for ip in private_ipv4s() {
    let o = ip.octets();
    subnets.insert([o[0], o[1], o[2]]);
    self_addrs.insert(ip);
  }
  if subnets.is_empty() {
    return Vec::new();
  }

  let client = match reqwest::Client::builder()
    .timeout(Duration::from_millis(PROBE_MS))
    .build()
  {
    Ok(c) => c,
    Err(_) => return Vec::new(),
  };
  let sem = Arc::new(Semaphore::new(CONCURRENCY));

  let mut tasks = Vec::new();
  for base in subnets {
    for host in 1u8..=254 {
      let ip = Ipv4Addr::new(base[0], base[1], base[2], host);
      if self_addrs.contains(&ip) {
        continue;
      }
      let client = client.clone();
      let sem = sem.clone();
      tasks.push(tokio::spawn(async move {
        let _permit = sem.acquire().await.ok()?;
        let url = format!("http://{ip}:{BOX_PORT}/api/pair/consume");
        match client.post(&url).json(&serde_json::json!({})).send().await {
          Ok(r) if matches!(r.status().as_u16(), 400 | 401 | 422) => {
            let origin = format!("http://{ip}:{BOX_PORT}");
            // Ask who it is. Best-effort: a hit stays a hit even if the box
            // predates the identity endpoint.
            let (name, claimed) = match client
              .get(format!("{origin}/api/box/identity"))
              .send()
              .await
            {
              Ok(resp) if resp.status().is_success() => {
                match resp.json::<serde_json::Value>().await {
                  Ok(v) => (
                    v["label"].as_str().unwrap_or("Virtues box").to_string(),
                    v["claimed"].as_bool(),
                  ),
                  Err(_) => ("Virtues box".to_string(), None),
                }
              }
              _ => ("Virtues box".to_string(), None),
            };
            Some(DiscoveredBox { name, origin, claimed })
          }
          _ => None,
        }
      }));
    }
  }

  let mut out = Vec::new();
  for t in tasks {
    if let Ok(Some(b)) = t.await {
      out.push(b);
    }
  }
  out.sort_by(|a, b| a.origin.cmp(&b.origin));
  out.dedup_by(|a, b| a.origin == b.origin);
  out
}
