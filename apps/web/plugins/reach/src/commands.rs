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

// ─── Improv BLE setup (iOS: CoreBluetooth, see ios/Sources/ImprovClient.swift) ─
//
// The PRIMARY setup path since 2026-08-10: the box serves the Improv Wi-Fi
// GATT service while unclaimed (virtues-core maintenance::ble_provision), and
// the app drives it from here. The phone never leaves its own network; the
// join is watched live over BLE instead of inferred from a dead socket.
//
// On platforms without the client (Android for now, desktop), these return
// empty/error and the connect screen falls back to LAN discovery + SoftAP.

/// Scan for unclaimed boxes advertising Improv. Returns `{boxes: [...]}` with
/// `id` (opaque, for the calls below), `name`, `improvState` (0x02 = needs
/// wifi, 0x04 = already online), `rssi`.
#[command]
pub(crate) async fn improv_discover<R: Runtime>(
  app: AppHandle<R>,
  seconds: Option<f64>,
) -> Result<serde_json::Value> {
  #[cfg(target_os = "ios")]
  {
    use tauri::Manager;
    let handle = app.state::<crate::IosPluginHandle<R>>();
    return handle
      .0
      .run_mobile_plugin("improv_discover", serde_json::json!({ "seconds": seconds }))
      .map_err(|e| crate::Error::Reach(e.to_string()));
  }
  #[cfg(not(target_os = "ios"))]
  {
    let _ = (app, seconds);
    Ok(serde_json::json!({ "boxes": [] }))
  }
}

/// Ask THAT BOX what wifi it can see, over BLE (Improv RPC 0x04).
#[command]
pub(crate) async fn improv_wifi_scan<R: Runtime>(
  app: AppHandle<R>,
  id: String,
) -> Result<serde_json::Value> {
  #[cfg(target_os = "ios")]
  {
    use tauri::Manager;
    let handle = app.state::<crate::IosPluginHandle<R>>();
    return handle
      .0
      .run_mobile_plugin("improv_wifi_scan", serde_json::json!({ "id": id }))
      .map_err(|e| crate::Error::Reach(e.to_string()));
  }
  #[cfg(not(target_os = "ios"))]
  {
    let _ = (app, id);
    Err(crate::Error::Reach("BLE setup is iOS-only for now".into()))
  }
}

/// Send credentials and watch the join (Improv RPC 0x01). Progress arrives as
/// `improv-progress` plugin events; the returned value is the outcome.
#[command]
pub(crate) async fn improv_provision<R: Runtime>(
  app: AppHandle<R>,
  id: String,
  ssid: String,
  password: String,
) -> Result<serde_json::Value> {
  #[cfg(target_os = "ios")]
  {
    use tauri::Manager;
    let handle = app.state::<crate::IosPluginHandle<R>>();
    return handle
      .0
      .run_mobile_plugin(
        "improv_provision",
        serde_json::json!({ "id": id, "ssid": ssid, "password": password }),
      )
      .map_err(|e| crate::Error::Reach(e.to_string()));
  }
  #[cfg(not(target_os = "ios"))]
  {
    let _ = (app, id, ssid, password);
    Err(crate::Error::Reach("BLE setup is iOS-only for now".into()))
  }
}

/// Drop the BLE connection. Safe to always call on leaving the setup flow.
#[command]
pub(crate) async fn improv_disconnect<R: Runtime>(app: AppHandle<R>) -> Result<()> {
  #[cfg(target_os = "ios")]
  {
    use tauri::Manager;
    let handle = app.state::<crate::IosPluginHandle<R>>();
    handle
      .0
      .run_mobile_plugin::<serde_json::Value>("improv_disconnect", serde_json::json!({}))
      .map_err(|e| crate::Error::Reach(e.to_string()))?;
    return Ok(());
  }
  #[cfg(not(target_os = "ios"))]
  {
    let _ = app;
    Ok(())
  }
}

/// Join a wifi network whose SSID starts with `ssid_prefix`, natively.
///
/// iOS only — `NEHotspotConfiguration` (NOT Personal Hotspot, NOT the
/// gated `NEHotspotHelper`; see `ios/Sources/ReachPlugin.swift`). Used by the
/// connect screen to put the phone on a box's `Virtues-XXXX` setup network
/// without a trip to Settings, a camera banner, or a captive sheet — the three
/// OS surfaces that each failed on hardware 2026-08-10. The user types only
/// the passphrase off the box's display; the prefix join finds the SSID.
///
/// Raises one system dialog ("Wants to Join…"). `joinOnce` on the Swift side
/// scopes the config to this app session, so nothing lingers in the phone's
/// network list after setup.
#[command]
pub(crate) async fn wifi_join<R: Runtime>(
  app: AppHandle<R>,
  ssid_prefix: String,
  passphrase: String,
) -> Result<serde_json::Value> {
  #[cfg(target_os = "ios")]
  {
    use tauri::Manager;
    let handle = app.state::<crate::IosPluginHandle<R>>();
    return handle
      .0
      .run_mobile_plugin(
        "wifi_join",
        serde_json::json!({ "ssidPrefix": ssid_prefix, "passphrase": passphrase }),
      )
      .map_err(|e| crate::Error::Reach(e.to_string()));
  }
  #[cfg(not(target_os = "ios"))]
  {
    let _ = (app, ssid_prefix, passphrase);
    // Android's equivalent is WifiNetworkSpecifier — not built yet. The
    // connect screen treats this error as "fall back to manual join".
    Err(crate::Error::Reach("programmatic wifi join is iOS-only for now".into()))
  }
}

/// Drop any setup-network config this app added (prefix-matched).
#[command]
pub(crate) async fn wifi_forget<R: Runtime>(
  app: AppHandle<R>,
  ssid_prefix: String,
) -> Result<serde_json::Value> {
  #[cfg(target_os = "ios")]
  {
    use tauri::Manager;
    let handle = app.state::<crate::IosPluginHandle<R>>();
    return handle
      .0
      .run_mobile_plugin("wifi_forget", serde_json::json!({ "ssidPrefix": ssid_prefix }))
      .map_err(|e| crate::Error::Reach(e.to_string()));
  }
  #[cfg(not(target_os = "ios"))]
  {
    let _ = (app, ssid_prefix);
    Ok(serde_json::json!({ "removed": false }))
  }
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
