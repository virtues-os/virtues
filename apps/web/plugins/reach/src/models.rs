use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairRequest {
  /// Box address as typed by the user: `10.0.0.5`, `10.0.0.5:8000`,
  /// `adam.local`, or a full `http://…:8000`. Normalized before use.
  pub server: String,
  /// The 6-digit pair code (spaces allowed).
  pub code: String,
}

/// A Virtues box found on the LAN via Bonjour.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundServer {
  pub name: String,
  pub origin: String,
  /// None = box too old to say. See DiscoveredBox::claimed.
  pub claimed: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverResponse {
  pub servers: Vec<FoundServer>,
  /// Diagnostic: what the scan saw (local IPs / subnets), shown small in the UI.
  #[serde(default)]
  pub debug: String,
}

/// Result of a provisioning join attempt.
///
/// `outcome` is `"joined" | "failed" | "unknown"`. Three values and not a
/// boolean because the third is real and common — the box takes its AP down to
/// perform the join, so the requesting phone routinely never sees the reply.
/// Collapsing that into `false` would tell an owner their box failed to connect
/// at the moment it succeeded.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvisionJoinResult {
  pub outcome: String,
  /// NetworkManager's own words on failure, passed through unreworded.
  pub detail: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReachStatus {
  pub paired: bool,
  /// "authed" | "rejected" | "unknown" | "unpaired" — mirrors the desktop
  /// connect-screen diagnosis.
  pub session: String,
  /// The loopback URL the webview should load once paired + reachable.
  pub loopback_url: String,
  /// LIVE: did a bounded `/auth/session` probe just succeed? Distinguishes a
  /// genuinely reachable box from a stale "paired" flag.
  pub reachable: bool,
  /// LIVE network path to the box right now: "direct" (LAN/hole-punched) |
  /// "relay" | "offline". Read from iroh after the probe (re)connects.
  pub path: String,
  /// Why this device cannot use the pairing it has, in words for the owner:
  /// on the Mac, the Keychain's sentence for a read that failed; that the
  /// pairing could not be read, with the system's text in parentheses; or that
  /// the device key is gone. `None` when the record loads whole, and when
  /// there is no record. `paired` stays true while this is set.
  pub problem: Option<String>,
  /// The device key is gone, so pairing again is the only fix. False for any
  /// other problem: the key may still be on this device, and quitting and
  /// reopening Virtues can bring it back. Pairing again would add a second
  /// device to the server's list while the first key still works.
  pub key_missing: bool,
}
