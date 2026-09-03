//! Box-side iroh reach integration.
//!
//! The box is an **iroh `Endpoint`** whose Ed25519 `EndpointId` *is* its identity
//! (mutual-key auth, no CA). It serves the box's existing axum app over iroh
//! ([`virtues_iroh::serve`]) and, in prod, homes on our relay so any paired
//! device reaches it by `EndpointId` — LAN-direct, hole-punched, or via the relay
//! — with **no public inbound port**. The plain-HTTP `:8000` listener stays for
//! LAN/loopback and the desktop `:7117` helper.
//!
//! Security is layered: iroh enforces an [`AllowPolicy`](virtues_iroh::AllowPolicy)
//! over paired-device EndpointIds at the transport; the app-layer bearer/cookie
//! remains the authorization keystone on top.
//!
//! Config (env for now; atlas `/relay/config` → relay_url and DB-backed allowlist
//! land in Steps 4/5):
//! - `VIRTUES_RELAY_URL`   — our relay, e.g. `https://relay.virtues.ch`. Unset =
//!   dev mode (n0 relays + discovery).
//! - `VIRTUES_IROH_ALLOW`  — comma-separated device EndpointIds allowed to connect
//!   (interim until pairing populates `app_device.node_id`).

use anyhow::{Context, Result};
use sqlx::PgPool;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use tokio::sync::Notify;
use virtues_iroh::{
    build_endpoint, iroh_port, serve, AllowPolicy, Endpoint, EndpointId, RelayUrl, SecretKey,
    StaticAllow,
};

/// `box_secrets` key holding this box's persistent iroh secret key (hex of the
/// 32-byte seed) — so the box keeps a stable `EndpointId` across restarts.
const BOX_IROH_SECRET: &str = "iroh_secret_key";

/// Process-wide "iroh endpoint is bound and homed on the relay" flag. Read by
/// pairing to advertise the box's reach ticket only when it's actually up.
static ENDPOINT_UP: OnceLock<Arc<AtomicBool>> = OnceLock::new();
/// Set when `maybe_spawn` gives up (secret load or endpoint bind failed) so
/// `/api/setup/state` can report an honest failure instead of leaving the
/// `remote_access` step reading "Connecting…" forever — the endpoint task
/// exits on either failure and nothing else would ever move the state again.
///
/// Holds a FIXED, operator-facing sentence — never the underlying error. This
/// value is rendered by `/api/setup/state`, which is public-on-LAN (the wizard
/// and appliance panel read it pre-auth) and documents itself as carrying only
/// booleans and step copy. The real cause is logged via `tracing::error!` at
/// each failure site, where it belongs.
static ENDPOINT_ERROR: OnceLock<RwLock<Option<&'static str>>> = OnceLock::new();
/// This box's own `EndpointId` (hex), set once the endpoint binds — handed to
/// devices at pairing so they can dial by it.
static BOX_ENDPOINT_ID: OnceLock<RwLock<Option<String>>> = OnceLock::new();
/// The relay URL this box homed on (if any) — the other half of the reach ticket.
static BOX_RELAY_URL: OnceLock<RwLock<Option<String>>> = OnceLock::new();
/// The box's iroh direct socket addresses (LAN/VPN `IP:quic-port`), handed to
/// devices at pairing so they can dial LAN-direct — no relay, no discovery, no
/// third party — when on the same network. Refreshed on the reconcile tick.
static BOX_DIRECT_ADDRS: OnceLock<RwLock<Vec<String>>> = OnceLock::new();
/// Wakes the reach loop to tear down and rebind its endpoint. Fired when relay
/// config lands on a box that is already up — the screen-2 account link — so
/// relay reach activates in-process instead of on the next service restart.
static REBIND: OnceLock<Arc<Notify>> = OnceLock::new();

fn rebind_notify() -> Arc<Notify> {
    REBIND.get_or_init(|| Arc::new(Notify::new())).clone()
}

/// Ask the reach loop to rebind its endpoint with freshly-resolved relay
/// config. Safe to call from anywhere, any number of times: a `Notify` permit
/// is stored if the loop is busy (the request is never lost), and the loop
/// drops requests whose resolved relay matches the one it is already on.
pub fn request_rebind() {
    rebind_notify().notify_one();
}

fn endpoint_up_flag() -> Arc<AtomicBool> {
    ENDPOINT_UP.get_or_init(|| Arc::new(AtomicBool::new(false))).clone()
}

/// Whether the box's iroh endpoint is bound (and reachable). Kept under the old
/// name so pairing call sites are unchanged; Step 7 renames to `endpoint_up`.
pub fn is_relay_registered() -> bool {
    ENDPOINT_UP.get().map(|f| f.load(Ordering::Relaxed)).unwrap_or(false)
}

/// The reason the reach loop last failed, if it has not since recovered.
/// `None` while still starting up, and again once a bind succeeds —
/// [`is_relay_registered`] distinguishes those two.
pub fn endpoint_error() -> Option<&'static str> {
    ENDPOINT_ERROR.get().and_then(|c| c.read().ok().and_then(|g| *g))
}

fn set_endpoint_error(msg: &'static str) {
    let cell = ENDPOINT_ERROR.get_or_init(|| RwLock::new(None));
    if let Ok(mut g) = cell.write() {
        *g = Some(msg);
    }
}

/// Clear the last failure after a bind succeeds.
///
/// Without this the field was a one-way latch on a path that RETRIES: the bind
/// loop backs off and tries again, so the very transient it exists to survive
/// — losing the race with the OS releasing the pinned UDP port — left
/// `/api/setup/state` reporting "Couldn't start reach networking on this box"
/// for the life of the process, while reach worked. A status surface built to
/// stop onboarding from lying must not itself latch a stale failure.
fn clear_endpoint_error() {
    if let Some(cell) = ENDPOINT_ERROR.get() {
        if let Ok(mut g) = cell.write() {
            *g = None;
        }
    }
}

/// This box's iroh `EndpointId` (hex), once bound — for the pairing reach ticket.
pub fn box_endpoint_id() -> Option<String> {
    BOX_ENDPOINT_ID.get().and_then(|c| c.read().ok().and_then(|g| g.clone()))
}

/// The relay URL this box homed on, if any — the other half of the reach ticket.
pub fn box_relay_url() -> Option<String> {
    BOX_RELAY_URL.get().and_then(|c| c.read().ok().and_then(|g| g.clone()))
}

/// The box's iroh direct socket addresses (LAN/VPN), for LAN-direct reach.
pub fn box_direct_addrs() -> Vec<String> {
    BOX_DIRECT_ADDRS
        .get()
        .and_then(|c| c.read().ok().map(|g| g.clone()))
        .unwrap_or_default()
}

/// Snapshot the endpoint's current direct addresses into `BOX_DIRECT_ADDRS`.
/// Called after bind and on each reconcile so a DHCP lease change is picked up.
fn refresh_direct_addrs(endpoint: &Endpoint) {
    let addrs: Vec<String> = endpoint.addr().ip_addrs().map(|a| a.to_string()).collect();
    if addrs.is_empty() {
        return; // not yet discovered — keep the last known set
    }
    let cell = BOX_DIRECT_ADDRS.get_or_init(|| RwLock::new(Vec::new()));
    if let Ok(mut g) = cell.write() {
        *g = addrs;
    }
}

/// Derive the box's stable EndpointId from its stored iroh secret, without
/// binding an endpoint. `virtues doctor` runs in a separate process that never
/// binds, so it can't use the in-memory getters above. `None` before first boot
/// provisions the secret.
async fn endpoint_id_from_secret(db: &PgPool) -> Option<String> {
    let (hex_seed, _) = crate::box_secrets::get(db, BOX_IROH_SECRET).await.ok()??;
    let bytes = hex::decode(hex_seed.trim()).ok()?;
    let arr: [u8; 32] = bytes.as_slice().try_into().ok()?;
    Some(SecretKey::from_bytes(&arr).public().to_string())
}

/// Honest, per-leg reach state for `virtues doctor`, read from the DB (doctor
/// never binds the endpoint). Each field is independently optional so the report
/// can show *exactly* which leg is unprovisioned — no more "works regardless".
pub struct ReachReport {
    /// Whether we could actually read the box database. `false` means every other
    /// field is meaningless (we never reached the DB) — doctor must say "unknown"
    /// rather than report authoritative-looking zeros. The usual cause is running
    /// `virtues doctor` as a user that can't read the box's env file, so the DB
    /// URL falls back to `postgres:///virtues` and connects as the wrong role.
    pub db_reachable: bool,
    /// The box's EndpointId (from its stored secret), or `None` pre-provision.
    pub endpoint_id: Option<String>,
    /// The stored relay URL, or `None` when LAN-only (unclaimed / atlas down).
    pub relay_url: Option<String>,
    /// How many device keys are currently on the allowlist, or `None` when the
    /// query failed. Optional like every other leg: a bare `0` here reads as
    /// "no devices paired", which is a different and much more alarming
    /// statement than "we could not check".
    pub allowlisted_devices: Option<usize>,
}

/// Read each reach leg's actual state for `virtues doctor`.
pub async fn reach_report(db: &PgPool) -> ReachReport {
    // Probe connectivity first: an unreachable DB makes every leg below collapse
    // to None/0, which is indistinguishable from a genuinely fresh box. Establish
    // the difference explicitly so doctor can report "unknown" honestly.
    if sqlx::query("SELECT 1").execute(db).await.is_err() {
        return ReachReport {
            db_reachable: false,
            endpoint_id: None,
            relay_url: None,
            allowlisted_devices: None,
        };
    }
    ReachReport {
        db_reachable: true,
        endpoint_id: endpoint_id_from_secret(db).await,
        relay_url: crate::virtues_api::relay::load(db)
            .await
            .ok()
            .flatten()
            .map(|rc| rc.relay_url),
        allowlisted_devices: allowed_ids(db).await.ok().map(|ids| ids.len()),
    }
}

/// How often the background reconcile runs to catch drift (15 min).
const RECONCILE_INTERVAL_SECS: u64 = 900;

/// Spawn the iroh reach subsystem: bind the endpoint and serve `app` over it.
/// `app` is the box's fully-built axum `Router` (cloned from the one served on
/// `:8000`). No-op-safe: logs and exits on fatal setup errors (box stays LAN-only).
///
/// Runs as a bind → serve → (maybe rebind) supervision loop. The rebind leg
/// exists for exactly one event: relay config landing on a box that is already
/// up — the screen-2 account link. The endpoint keeps whatever relay it bound
/// with, so without this, a box linked mid-life stayed LAN-only until its next
/// restart (observed live 2026-08-11: linked at an office, still unreachable).
/// [`request_rebind`] wakes the loop; it tears the endpoint down and rebinds on
/// the same pinned port with the new relay. The `EndpointId` never changes —
/// only the homing does — so pairs and allowlists survive untouched.
pub fn maybe_spawn(db: PgPool, app: axum::Router) {
    tokio::spawn(async move {
        let secret = match load_or_create_secret(&db).await {
            Ok(s) => s,
            Err(e) => {
                let msg = format!("{e:#}");
                tracing::error!(error = %msg, "iroh: failed to load/create secret key — reach disabled");
                set_endpoint_error(
                    "Couldn't set up this box's reach identity. See the box logs; a restart is needed.",
                );
                return;
            }
        };
        // The EndpointId is the secret's public half, so it is known without
        // binding. Set it now: pairing advertises this box's reach ticket from
        // it, and should not have to wait for the endpoint to come up.
        set_box_endpoint_id(&secret.public().to_string());

        let mut bind_backoff: u64 = 1;
        loop {
            let relay_url = resolve_relay_url(&db).await;
            set_box_relay_url(relay_url.as_ref().map(|u| u.to_string()));
            // The box pins its UDP port so its `IP:port` stays stable across restarts
            // — LAN peers resolve the IP (mDNS) and dial by NodeId, nothing frozen.
            // A bind failure must NOT end the supervision task. It used to
            // `return`, which meant a rebind that lost its race with the OS
            // releasing the pinned UDP port left the box linked, its endpoint
            // closed, and reach dead until someone restarted the service —
            // strictly worse than the LAN-only state the rebind was meant to
            // improve on. Retry with backoff instead; the previous endpoint is
            // already gone, so there is nothing to preserve by giving up.
            let endpoint = match build_endpoint(secret.clone(), relay_url.clone(), Some(iroh_port()))
                .await
            {
                Ok(e) => e,
                Err(e) => {
                    let msg = format!("{e:#}");
                    tracing::error!(error = %msg, backoff_secs = bind_backoff, "iroh: failed to bind endpoint — retrying");
                    set_endpoint_error(
                        "Couldn't start reach networking on this box. Retrying; see the box logs.",
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(bind_backoff)).await;
                    bind_backoff = (bind_backoff * 2).min(60);
                    continue;
                }
            };
            bind_backoff = 1;
            let eid = endpoint.id().to_string();
            set_box_endpoint_id(&eid);
            endpoint_up_flag().store(true, Ordering::Relaxed);
            clear_endpoint_error();
            match &relay_url {
                Some(u) => tracing::info!(endpoint_id = %eid, relay = %u, port = iroh_port(), "iroh endpoint bound; box reachable by EndpointId via our relay (+ LAN-direct)"),
                None => tracing::info!(endpoint_id = %eid, port = iroh_port(), "iroh endpoint bound; box reachable by EndpointId LAN-direct (no relay)"),
            }

            // Capture direct addresses for zero-third-party LAN reach (dial the box
            // by its EndpointId at these LAN/VPN sockets — no relay, no discovery).
            // Time-boxed so slow address discovery never blocks bringup; the
            // reconcile loop refreshes them (e.g. after a DHCP lease change).
            let _ = tokio::time::timeout(std::time::Duration::from_secs(5), endpoint.online()).await;
            refresh_direct_addrs(&endpoint);
            let ep_handle = endpoint.clone();

            let allow = load_allowlist(&db).await;
            // Serve the existing axum app over iroh. Hold the router handle until a
            // rebind (dropping it aborts the accept loop).
            let router = serve(endpoint, app.clone(), allow);
            // Periodic reconcile catches drift the event-driven path (after_pairing_change)
            // can't: atlas restarting and losing our registration, a device that paired
            // while atlas was unreachable, or relay config that only became available
            // after we bound. Idempotent + best-effort.
            let mut tick =
                tokio::time::interval(std::time::Duration::from_secs(RECONCILE_INTERVAL_SECS));
            tick.tick().await; // consume the immediate first tick — startup already reconciled above
            let rebind = rebind_notify();
            loop {
                tokio::select! {
                    _ = tick.tick() => {
                        refresh_direct_addrs(&ep_handle);
                        reconcile(&db).await;
                    }
                    _ = rebind.notified() => {
                        // Only a CHANGED relay justifies dropping live connections;
                        // a stale or duplicate request resolves to what we already
                        // have and is dropped here.
                        if resolve_relay_url(&db).await == relay_url {
                            tracing::debug!("iroh: rebind requested but relay config unchanged — ignoring");
                            continue;
                        }
                        break;
                    }
                }
            }

            // Tear down for rebind. The next bind reuses the same pinned UDP port,
            // so the endpoint must be closed (not just dropped) to release it.
            endpoint_up_flag().store(false, Ordering::Relaxed);
            tracing::info!("iroh: relay config changed — rebinding endpoint");
            router.shutdown().await.ok();
            ep_handle.close().await;
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    });
}

fn set_box_relay_url(url: Option<String>) {
    let cell = BOX_RELAY_URL.get_or_init(|| RwLock::new(None));
    if let Ok(mut g) = cell.write() {
        *g = url;
    }
}

fn set_box_endpoint_id(eid: &str) {
    let cell = BOX_ENDPOINT_ID.get_or_init(|| RwLock::new(None));
    if let Ok(mut g) = cell.write() {
        *g = Some(eid.to_string());
    }
}

/// The relay every box homes on unless told otherwise. A compiled-in default,
/// not a fetched one, because the alternative is the bootstrap problem: a box
/// cannot fetch the address of the thing it needs in order to be reachable,
/// and requiring an account link to learn it is the coupling the open-relay
/// work deleted (open-relay-plan §Work 2). This is the industry-normal shape —
/// Tailscale bakes its DERP list, Syncthing its relay pool, iroh its n0
/// relays. What it reveals is only "this pubkey is online at this IP": the
/// relay is open-admission, e2e-blind, and reports to no one.
pub const DEFAULT_RELAY_URL: &str = "https://relay.virtues.ch";

/// `VIRTUES_RELAY_URL=off` (or `none`/`disabled`): run relay-less. The off
/// switch must be an explicit word — an *empty* env var falls through to the
/// baked default, so "unset" and "off" stay different states.
fn relay_disabled_by_env(raw: &str) -> bool {
    matches!(raw.to_ascii_lowercase().as_str(), "off" | "none" | "disabled")
}

/// Resolve our relay URL: the atlas-provisioned config (stored at claim/link)
/// first, then the `VIRTUES_RELAY_URL` env override (or off switch), then —
/// on a box install only — [`DEFAULT_RELAY_URL`], so a box that never signs
/// in is still reachable from its first boot. `None` → relay-less.
///
/// The default is gated on the box-install marker: a dev checkout on a
/// laptop must not home on the production relay just because someone ran
/// `make dev` (same guard, same reasoning as the sudo re-exec in main.rs).
async fn resolve_relay_url(db: &PgPool) -> Option<RelayUrl> {
    // Self-heal a missing relay config (box claimed before the relay existed, or
    // a claim-time fetch that 503'd) before we bind.
    ensure_relay_config(db).await;
    if let Ok(Some(rc)) = crate::virtues_api::relay::load(db).await {
        if relay_disabled_by_env(&rc.relay_url) {
            // The stored config can also carry the off word (Settings writes
            // it there so the choice survives upgrades and env rewrites).
            tracing::info!("iroh: relay disabled by stored config");
            return None;
        }
        if let Ok(u) = RelayUrl::from_str(&rc.relay_url) {
            return Some(u);
        }
    }
    if let Some(raw) = std::env::var("VIRTUES_RELAY_URL").ok().filter(|s| !s.is_empty()) {
        if relay_disabled_by_env(&raw) {
            tracing::info!("iroh: relay disabled by VIRTUES_RELAY_URL");
            return None;
        }
        match RelayUrl::from_str(&raw) {
            Ok(u) => return Some(u),
            Err(e) => {
                tracing::warn!(error = %e, url = %raw, "VIRTUES_RELAY_URL is not a valid relay URL — ignoring");
            }
        }
    }
    if std::path::Path::new("/var/lib/virtues/virtues.env").exists() {
        return RelayUrl::from_str(DEFAULT_RELAY_URL).ok();
    }
    None
}

/// Fetch + store this box's relay config from atlas if it isn't stored yet.
/// Best-effort and idempotent (no-op once homed). A freshly-fetched relay only
/// takes effect on the next endpoint bind — the running endpoint keeps the relay
/// it bound with. The link path calls [`request_rebind`] to trigger that bind
/// immediately; this reconcile-time fetch is the slower self-heal for a box
/// that came up LAN-only (atlas down at boot, claim-time fetch that 503'd).
async fn ensure_relay_config(db: &PgPool) {
    if matches!(crate::virtues_api::relay::load(db).await, Ok(Some(_))) {
        return;
    }
    let Ok(Some(api_key)) = crate::virtues_api::renew::read_api_key(db).await else {
        return;
    };
    let http = crate::http_client::virtues_api_client();
    let atlas = crate::virtues_api::atlas_url();
    match crate::virtues_api::relay::fetch_and_store(db, &http, &atlas, &api_key).await {
        Ok(()) => {
            tracing::info!("iroh: fetched relay config from atlas");
            // REBIND NOW, not at the next restart. A freshly-fetched relay
            // only takes effect on the next endpoint bind, and without this
            // a box that missed relay config at link time (atlas 503, boot
            // race) stayed RelayMode::Disabled — LAN-only — until someone
            // power-cycled it, invisibly (audit finding, 2026-08-24).
            request_rebind();
        }
        Err(e) => tracing::debug!(error = %format!("{e:#}"), "iroh: relay-config fetch skipped (LAN-only for now)"),
    }
}

/// Load the box's persistent iroh secret key from `box_secrets`, generating and
/// persisting a fresh one on first boot so the `EndpointId` is stable.
async fn load_or_create_secret(db: &PgPool) -> Result<SecretKey> {
    if let Some((hex_seed, _meta)) = crate::box_secrets::get(db, BOX_IROH_SECRET).await? {
        let bytes = hex::decode(hex_seed.trim()).context("decode stored iroh secret")?;
        let arr: [u8; 32] = bytes.as_slice().try_into().context("iroh secret is not 32 bytes")?;
        return Ok(SecretKey::from_bytes(&arr));
    }
    let mut seed = [0u8; 32];
    {
        use rand::RngCore;
        rand::rng().fill_bytes(&mut seed);
    }
    let secret = SecretKey::from_bytes(&seed);
    crate::box_secrets::put(db, BOX_IROH_SECRET, &hex::encode(seed), &serde_json::json!({}))
        .await
        .context("persist iroh secret key")?;
    tracing::info!("iroh: generated + persisted new box secret key");
    Ok(secret)
}

/// The live allowlist handle, so pairing/revocation can hot-swap it without
/// restarting the endpoint (both this and the `Arc<dyn AllowPolicy>` handed to
/// `serve` share the same inner set).
static ALLOW: OnceLock<StaticAllow> = OnceLock::new();

/// Non-revoked device EndpointIds from the DB, plus any `VIRTUES_IROH_ALLOW`
/// (dev/manual). The box's own EndpointId is implicitly trusted (it never dials
/// itself) so it's not included.
///
/// **Returns `Err` rather than an empty list when the query fails.** This used
/// to log a warning and hand back whatever it had accumulated — which is
/// nothing — and every caller then installed that empty set as the live
/// allowlist. One transient database error therefore refused *every* paired
/// device at the transport (`HttpHandler::accept` closes a non-allowlisted
/// connection before any HTTP) until the next reconcile 15 minutes later. The
/// box was up, the relay was up, and nothing anywhere said why. Absence and
/// failure are different answers and only the caller can tell them apart.
async fn allowed_ids(db: &PgPool) -> Result<Vec<EndpointId>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT node_id FROM app_device WHERE node_id IS NOT NULL AND revoked_at IS NULL",
    )
    .fetch_all(db)
    .await?;
    let mut ids = Vec::with_capacity(rows.len());
    for (nid,) in rows {
        if let Ok(id) = EndpointId::from_str(nid.trim()) {
            ids.push(id);
        }
    }
    if let Ok(raw) = std::env::var("VIRTUES_IROH_ALLOW") {
        for tok in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            match EndpointId::from_str(tok) {
                Ok(id) => ids.push(id),
                Err(e) => tracing::warn!(error = %e, token = %tok, "VIRTUES_IROH_ALLOW: skipping invalid EndpointId"),
            }
        }
    }
    Ok(ids)
}

/// Build + install the live allowlist for `serve`.
///
/// A failed query leaves the previous set in place: on a rebind that is the
/// working allowlist, and on a first bind it is empty either way. Never
/// replace a good set with the result of a query that did not run.
async fn load_allowlist(db: &PgPool) -> Arc<dyn AllowPolicy> {
    let allow = ALLOW.get_or_init(StaticAllow::default).clone();
    match allowed_ids(db).await {
        Ok(ids) => {
            tracing::info!(count = ids.len(), "iroh allowlist loaded");
            allow.replace(ids);
        }
        Err(e) => tracing::error!(
            error = %e,
            "iroh allowlist query failed — binding with the last known allowlist"
        ),
    }
    Arc::new(allow)
}

/// Fire-and-forget reconcile after a pairing or revocation. Non-blocking so the
/// pairing handlers don't wait on the atlas round-trip.
pub fn after_pairing_change(db: PgPool) {
    tokio::spawn(async move {
        reconcile(&db).await;
    });
}

/// The one place that makes the box's live reach state match the DB. Idempotent
/// + best-effort, safe to run at startup, on a timer, and after any pairing or
/// revocation:
///   1. relay config present (self-heal a late / failed claim-time fetch)
///   2. iroh allowlist == non-revoked device keys (hot-swapped into `serve`)
///   3. model files present (health signal only — the installer owns fetching)
pub async fn reconcile(db: &PgPool) {
    ensure_relay_config(db).await;

    match allowed_ids(db).await {
        Ok(ids) => {
            if let Some(allow) = ALLOW.get() {
                tracing::debug!(count = ids.len(), "iroh allowlist refreshed");
                allow.replace(ids);
            }
        }
        // Keep serving the live allowlist. Replacing it with an empty set here
        // is what locked whole fleets out for a reconcile interval.
        Err(e) => tracing::error!(
            error = %e,
            "iroh allowlist query failed — keeping the live allowlist"
        ),
    }

    let report = crate::inference_report::resolution_report();
    let missing = report.missing();
    if !missing.is_empty() {
        tracing::warn!(?missing, "reconcile: model files missing — re-run the installer to fetch");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A database error must reach the caller, not arrive disguised as "no
    /// devices are paired". Every installer of the allowlist branches on this
    /// distinction, and the empty-list answer refused the whole fleet at the
    /// transport for a reconcile interval.
    ///
    /// The failure is induced by closing the pool, which is the cheapest thing
    /// that makes a real query fail for a real reason.
    #[sqlx::test(migrations = "./migrations")]
    async fn allowlist_query_failure_is_not_an_empty_allowlist(pool: PgPool) {
        sqlx::query(
            "INSERT INTO app_device (id, user_id, kind, label, node_id) \
             VALUES ('dev_alw1', $1, 'desktop_app', 'Test', \
                     'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa')",
        )
        .bind(crate::middleware::http::OWNER_USER_ID)
        .execute(&pool)
        .await
        .expect("seed a paired device");

        let live = allowed_ids(&pool).await.expect("query succeeds while open");
        assert_eq!(live.len(), 1, "the seeded device is on the allowlist");

        pool.close().await;

        match allowed_ids(&pool).await {
            Err(_) => {}
            Ok(ids) => panic!(
                "a failed query returned Ok({}) — the caller cannot tell this \
                 from a box with no paired devices, and installs it as the \
                 live allowlist",
                ids.len()
            ),
        }
    }
}
