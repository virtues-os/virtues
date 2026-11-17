use tauri::{command, AppHandle, Runtime};

use crate::models::{DiscoverResponse, FoundServer, PairRequest, ReachStatus};
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
      claimed: b.claimed,
    })
    .collect::<Vec<_>>();
  let debug = if ips.is_empty() {
    "no LAN IP — turn on Local Network in Settings, or check Wi-Fi".to_string()
  } else {
    format!("scanned from {}", ips.join(", "))
  };
  Ok(DiscoverResponse { servers, debug })
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

// ─── The pairing door ────────────────────────────────────────────────────────
//
// Lets an already-paired computer stand in for the box while a phone that
// cannot reach it enrolls — see `virtues_reach_client::pair_door` for why this
// exists and why it is not the loopback proxy. Mobile-hosted doors are refused:
// a phone is not a stable address for another device to type, and the phone
// shells have no "Add device" surface.

/// Open the door on this machine's LAN address. Returns `{origin, ttlSecs}`;
/// `origin` is the `host:port` to show the user.
#[command]
pub(crate) async fn pair_door_open<R: Runtime>(
  app: AppHandle<R>,
  ttl_secs: Option<u64>,
) -> Result<serde_json::Value> {
  #[cfg(any(target_os = "ios", target_os = "android"))]
  {
    let _ = (&app, ttl_secs);
    return Err(crate::Error::Reach(
      "the pairing door is opened from a computer, not a phone".into(),
    ));
  }
  #[cfg(not(any(target_os = "ios", target_os = "android")))]
  {
    // Ten minutes matches the countdown the Add-device sheet already shows.
    let ttl = ttl_secs.unwrap_or(600).clamp(60, 1800);
    let (origin, secs) = app.reach().open_pair_door(ttl).await?;
    Ok(serde_json::json!({ "origin": origin, "ttlSecs": secs }))
  }
}

#[command]
pub(crate) async fn pair_door_close<R: Runtime>(app: AppHandle<R>) -> Result<()> {
  app.reach().close_pair_door();
  Ok(())
}

/// `{open, origin?, attemptsUsed?}` — lets the sheet show the address and
/// notice a door that closed itself when its window ran out.
#[command]
pub(crate) async fn pair_door_status<R: Runtime>(app: AppHandle<R>) -> Result<serde_json::Value> {
  Ok(match app.reach().pair_door_status() {
    Some((origin, used)) => {
      serde_json::json!({ "open": true, "origin": origin, "attemptsUsed": used })
    }
    None => serde_json::json!({ "open": false }),
  })
}

// ─── The pairing handoff ─────────────────────────────────────────────────────
//
// The door's sibling, for the network the door cannot survive (coworking wifi
// that isolates clients). The laptop mints the phone's identity, enrolls its
// public half with the box over the relay, and hands the result over as one QR
// the phone scans — no network between the two devices at all.

/// LAPTOP: returns `{qrSvg, nodeId}`. Desktop-only — a phone has no second
/// device to enroll and no Add-device surface.
#[command]
pub(crate) async fn pair_handoff_create<R: Runtime>(
  app: AppHandle<R>,
  label: Option<String>,
) -> Result<serde_json::Value> {
  #[cfg(any(target_os = "ios", target_os = "android"))]
  {
    let _ = (&app, label);
    return Err(crate::Error::Reach(
      "a pairing code is created on a computer, not a phone".into(),
    ));
  }
  #[cfg(not(any(target_os = "ios", target_os = "android")))]
  {
    let (qr_svg, node_id, device_id) = app.reach().create_handoff(label).await?;
    Ok(serde_json::json!({ "qrSvg": qr_svg, "nodeId": node_id, "deviceId": device_id }))
  }
}

/// PHONE: adopt a scanned handoff payload. Returns the same ReachStatus every
/// other pairing path returns, so the airlock's existing success handling works
/// unchanged.
#[command]
pub(crate) async fn pair_handoff_accept<R: Runtime>(
  app: AppHandle<R>,
  payload: String,
) -> Result<ReachStatus> {
  app.reach().accept_handoff(&payload).await
}

// ─── The owner proof ─────────────────────────────────────────────────────────

/// Sign one HTTP request for the box with this device's key: `method` and
/// `path` exactly as the request carries them, and the `nonce` the box issued
/// at `POST /api/owner-proof/nonce`. Returns the value for the
/// `X-Virtues-Owner-Proof` header.
///
/// The box asks for this on the routes a process on this machine must not
/// reach just by connecting to the app's loopback port: revealing a password
/// or a key, adding a device, changing the AI key, importing an applet
/// package. Only the app holds the key, so only the app can answer. The same
/// signing works on the Mac and on iOS; the seed never leaves Rust.
#[command]
pub(crate) async fn owner_proof_sign<R: Runtime>(
  app: AppHandle<R>,
  method: String,
  path: String,
  nonce: String,
) -> Result<String> {
  app
    .reach()
    .sign_http_owner_proof(&method, &path, &nonce)
    .map_err(|e| crate::Error::Reach(format!("{e:#}")))
}
