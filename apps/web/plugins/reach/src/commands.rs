use tauri::{command, AppHandle, Runtime};

use crate::models::{DiscoverResponse, FoundServer, PairRequest, ProvisionJoinResult, ReachStatus};
use crate::ReachExt;
use crate::Result;
use virtues_reach_client::outbox;

#[command]
pub(crate) async fn pair<R: Runtime>(
  app: AppHandle<R>,
  payload: PairRequest,
) -> Result<ReachStatus> {
  app.reach().pair(&payload.server, &payload.code).await
}

#[command]
pub(crate) async fn reach_status<R: Runtime>(app: AppHandle<R>) -> Result<ReachStatus> {
  Ok(app.reach().status().await)
}

#[command]
pub(crate) async fn forget<R: Runtime>(app: AppHandle<R>) -> Result<()> {
  app.reach().forget()
}

#[command]
pub(crate) async fn discover<R: Runtime>(_app: AppHandle<R>) -> Result<DiscoverResponse> {
  // Subnet scan (mDNS-free) — reliable across iOS's flaky Bonjour + APs that
  // filter multicast. Returns boxes as IP origins.
  let ips = virtues_reach_client::local_private_ipv4s();
  let servers = virtues_reach_client::scan_subnet()
    .await
    .into_iter()
    .map(|b| FoundServer {
      name: b.name,
      origin: b.origin,
    })
    .collect::<Vec<_>>();
  let debug = if ips.is_empty() {
    "no LAN IP — turn on Local Network in Settings, or check Wi-Fi".to_string()
  } else {
    format!("scanned from {}", ips.join(", "))
  };
  Ok(DiscoverResponse { servers, debug })
}

// ─── Wifi provisioning over the box's setup AP ───────────────────────────────
//
// These three drive an APPLIANCE through wifi setup from the app, while the
// phone is joined to the box's own `Virtues-XXXX` network. See
// `virtues_reach_client::provision` for why this runs in the app at all rather
// than being left to the box's captive portal — short version: the owner's home
// wifi password deserves a native field, and the app can hold the setup session
// across the network handoff that follows.
//
// They go through Rust, not `fetch` in the webview, for the same reason `pair`
// does: plain HTTP to `10.42.0.1` from a `tauri://` origin is what App
// Transport Security exists to block.

/// Is this box in setup mode and reachable from where we are standing?
///
/// The box's own gates answer it: `/api/provision/*` exists only for a caller
/// on the AP subnet talking to an *unclaimed* box, so a 200 means both at once.
/// Cheap enough to run against every candidate the subnet scan turned up.
#[command]
pub(crate) async fn provision_open<R: Runtime>(
  _app: AppHandle<R>,
  server: String,
) -> Result<bool> {
  Ok(virtues_reach_client::provision::is_open(&crate::normalize_server(&server)).await)
}

/// Networks the BOX can see — not the phone's list. Different antenna, possibly
/// a different room; offering the phone's would let someone pick a network the
/// box cannot hear and produce a failure with no explanation.
#[command]
pub(crate) async fn provision_networks<R: Runtime>(
  _app: AppHandle<R>,
  server: String,
) -> Result<Vec<virtues_reach_client::provision::Network>> {
  virtues_reach_client::provision::networks(&crate::normalize_server(&server))
    .await
    .map_err(crate::Error::from)
}

/// Put the box on the owner's network.
///
/// Returns one of `"joined" | "failed" | "unknown"`, and **`"unknown"` is the
/// expected outcome, not an edge case**: the box drops its AP as the first step
/// of the join, so the phone issuing this request usually loses its socket
/// mid-flight — on the success path as often as the failure path. The caller
/// must treat it as "go and look", never as an error.
#[command]
pub(crate) async fn provision_join<R: Runtime>(
  _app: AppHandle<R>,
  server: String,
  ssid: String,
  psk: Option<String>,
) -> Result<ProvisionJoinResult> {
  use virtues_reach_client::provision::JoinOutcome;
  let outcome = virtues_reach_client::provision::join(
    &crate::normalize_server(&server),
    &ssid,
    psk.as_deref(),
  )
  .await
  .map_err(crate::Error::from)?;
  Ok(match outcome {
    JoinOutcome::Joined => ProvisionJoinResult { outcome: "joined".into(), detail: None },
    JoinOutcome::Failed(d) => {
      ProvisionJoinResult { outcome: "failed".into(), detail: Some(d) }
    }
    JoinOutcome::Unknown => ProvisionJoinResult { outcome: "unknown".into(), detail: None },
  })
}

/// Sync-queue health for a stream (device screen's Sync section).
#[command]
pub(crate) async fn outbox_stats<R: Runtime>(
  _app: AppHandle<R>,
  stream: String,
) -> Result<outbox::OutboxStats> {
  Ok(outbox::stats(&stream).unwrap_or_default())
}

/// Drain the outbox to the box immediately ("Sync now"). Returns records sent.
#[command]
pub(crate) async fn drain_now<R: Runtime>(app: AppHandle<R>) -> Result<usize> {
  app.reach().drain_now().await
}

/// Radio-hygiene counters (device screen's Sync section) — the on-device
/// battery A/B harness. `parked` = no warm endpoint right now (radio idle).
#[command]
pub(crate) async fn radio_stats<R: Runtime>(_app: AppHandle<R>) -> Result<serde_json::Value> {
  let s = crate::stats::snapshot();
  let mut v = serde_json::to_value(&s).unwrap_or_else(|_| serde_json::json!({}));
  if let Some(obj) = v.as_object_mut() {
    obj.insert("parked".into(), serde_json::Value::Bool(crate::warm_client().is_none()));
  }
  Ok(v)
}
