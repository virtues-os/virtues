//! BLE wifi provisioning — the Improv Wi-Fi service, served by the box.
//!
//! This replaces the SoftAP + captive dance as the PRIMARY wifi-setup path.
//! The week of 2026-08-10 established why with unusual thoroughness: every
//! failure the SoftAP flow produced — captive sheets that render blank, iOS
//! caching stale portals per-SSID, camera QR banners that never appear, scans
//! that return nothing while a client is associated, the blind switchover —
//! shared one root cause: *provisioning rode the same radio it was trying to
//! configure*. BLE severs that coupling. The phone never leaves its own
//! network, the box never hosts an AP, the wifi radio is free to scan and
//! join, and the app watches the join happen live over a channel that
//! survives it.
//!
//! **The protocol is Improv (improv-wifi.com), implemented faithfully — not an
//! in-house dialect.** Improv is the open standard from the Home Assistant /
//! ESPHome world, which is this product's nearest neighborhood. Speaking it
//! exactly keeps our client code boring, and extensions ride the reserved
//! command space. Note the base Improv wifi commands are now behind the phrase
//! gate (see below), so the improv-wifi.com web tester — which cannot send the
//! `0x86 ClaimSetup` that opens a session — gets `NotAuthorized` rather than
//! provisioning the box; the generic-client interop is a non-goal, not a
//! feature.
//!
//! **Lifecycle: advertised while the box is UNCLAIMED — and again whenever a
//! claimed box has been OFFLINE for [`OFFLINE_GRACE_SECS`].**
//!
//! Unclaimed: deliberately *not* "while offline". An unclaimed box on ethernet
//! still advertises, in the `Provisioned` state, because the advertisement
//! doubles as discovery — the app can read the box's URL over BLE instead of
//! subnet-scanning, which was its own source of flakiness.
//!
//! Claimed and offline: the moved box. Set up at home, switched on at the
//! office, it has no network it knows, so no relay and no LAN can reach it,
//! and until 2026-09-25 nothing could — the owner saw "Can't reach your
//! server" with no way out short of a cable or a reset. Now it advertises
//! `AuthorizationRequired`, and ONLY an already-paired device can open it, by
//! signing a challenge with the iroh key the box allowlisted at pairing (`0x88`
//! / `0x89`). The owner session that buys may scan and join wifi and nothing
//! else. The phrase opens nothing on a claimed box. The moment it is back
//! online the service goes quiet again.
//!
//! **Authorization: a four-word phrase gate replaces Improv's own
//! authorization-required state.** `0x86 ClaimSetup` must present the words
//! shown on the panel before any wifi/pair/link command is served (see "the
//! gate" in `handle_rpc`) — so an attacker in radio range cannot even set the
//! box's network without line of sight to the screen. Pairing then needs no
//! code on any surface: `0x83` is codeless and session-authorized, so the box
//! fetches its own standing code and redeems it over loopback (`api/display.rs`
//! deliberately does not render it, and `0x85` — which used to hand it to the
//! app — was deleted 2026-08-24). The earlier posture — "we skip authorization;
//! the credential is on the screen" — predates the gate; do not trust it.
//!
//! The GATT plumbing is Linux-only (`bluer` → BlueZ). The protocol layer is
//! platform-free and unit-tested everywhere.

#![allow(dead_code)] // the protocol layer is used only from the linux half

/// How long a CLAIMED box must be offline before it advertises for its owner.
///
/// Long enough that a router reboot or a slow DHCP at boot never lights the
/// radio; short enough that someone who just carried the box to a new place
/// finds it asking by the time they have the app open.
pub const OFFLINE_GRACE_SECS: u64 = 90;

/// How long an owner challenge (`0x88`) stays redeemable. One round trip plus
/// a signature — seconds, not minutes.
const CHALLENGE_TTL_SECS: u64 = 60;

/// Most owner challenges outstanding at once. A household has a handful of
/// devices; the cap only exists so radios in range cannot grow the table.
const MAX_CHALLENGES: usize = 16;

/// Check an owner proof's CRYPTOGRAPHY: that `signature_hex` is `endpoint_id`'s
/// signature over [`owner_proof_message`] of `nonce`. Returns the parsed id so
/// the caller can check it against the allowlist — which this deliberately
/// does not do, so it can be tested without a database.
///
/// `None` for every failure alike (bad hex, wrong length, bad signature): the
/// box answers all of them with the same `NotAuthorized`.
pub(crate) fn verify_owner_signature(
    nonce: &[u8],
    endpoint_id: &str,
    signature_hex: &str,
) -> Option<virtues_iroh::EndpointId> {
    use std::str::FromStr;
    let id = virtues_iroh::EndpointId::from_str(endpoint_id.trim()).ok()?;
    let bytes: [u8; 64] = hex::decode(signature_hex.trim()).ok()?.try_into().ok()?;
    let sig = virtues_iroh::Signature::from_bytes(&bytes);
    id.verify(&owner_proof_message(nonce), &sig).ok()?;
    Some(id)
}

// ─── Improv protocol ────────────────────────────────────────────────────────
//
// The wire format lives in `virtues-improv`, shared with the DESKTOP client
// that drives it (`virtues_improv::client`). It was implemented here first and
// again in Swift; a third copy for the desktop app is where that stopped being
// tolerable, so the Rust side became one module with a round-trip test that
// builds every command as a client and parses it as the box. Swift keeps its
// own copy — nothing can be done about that — and its tests live beside it.
//
// The crate's `client` feature is OFF here on purpose: the box is a GATT
// server and must not carry a BLE client stack it will never use.
pub use virtues_improv::protocol::{
    build_result, chunk_for_results, owner_proof_message, parse_rpc, service_data, Command,
    ImprovError, State,
    CHAR_CAPABILITIES, CHAR_CURRENT_STATE, CHAR_ERROR_STATE, CHAR_RPC_COMMAND, CHAR_RPC_RESULT,
    SERVICE_DATA_UUID_16, SERVICE_UUID,
};

// ─── BlueZ plumbing (Linux only) ────────────────────────────────────────────

#[cfg(target_os = "linux")]
mod server {
    use super::*;
    use futures::FutureExt;
    use sqlx::PgPool;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::Mutex;

    /// How often to reconcile the BLE service against the box's claimed state.
    const RECONCILE_SECS: u64 = 15;

    /// Shared mutable half: the state machine + live notifier handles.
    struct Improv {
        state: State,
        error: ImprovError,
        last_result: Vec<u8>,
        state_tx: Option<bluer::gatt::local::CharacteristicNotifier>,
        error_tx: Option<bluer::gatt::local::CharacteristicNotifier>,
        result_tx: Option<bluer::gatt::local::CharacteristicNotifier>,
        /// The one live setup session: which peer proved the phrase, and when it
        /// last did something. Every configuring command is gated on this.
        ///
        /// Keyed by BLE address and expired by inactivity rather than tied to a
        /// connection object, because BlueZ gives us no disconnect signal here.
        /// The approximation is sound: a dropped connection sends no more
        /// commands, so the session ages out. The address is not the security —
        /// the phrase (or the owner's signature) is; this only decides *which*
        /// proven peer is mid-setup.
        session: Option<(String, std::time::Instant, SessionKind)>,
        /// Outstanding owner challenges, one per BLE peer: the nonce and when
        /// it was issued. Single use — taken, not read, by `0x89`.
        ///
        /// Per peer, not one slot for the box: with one slot, any radio in
        /// range could ask for a challenge every second and keep replacing the
        /// owner's before they could answer it.
        challenges: std::collections::HashMap<String, ([u8; 32], std::time::Instant)>,
    }

    /// What proved a session, and so what it may do.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum SessionKind {
        /// The four-word phrase on an unclaimed box: wifi, grant, pair.
        Setup,
        /// A paired device's signature on a claimed box: wifi only.
        Owner,
    }

    /// How long a claimed setup session survives without a command. Long enough
    /// to type a wifi password and wait out a join, short enough that a box left
    /// alone returns to refusing everything.
    const SESSION_IDLE_TIMEOUT_SECS: u64 = 600;

    impl Improv {
        fn new(initial: State) -> Self {
            Self {
                state: initial,
                error: ImprovError::None,
                last_result: Vec::new(),
                state_tx: None,
                error_tx: None,
                result_tx: None,
                session: None,
                challenges: std::collections::HashMap::new(),
            }
        }

        async fn set_state(&mut self, s: State) {
            self.state = s;
            if let Some(tx) = &mut self.state_tx {
                let _ = tx.notify(vec![s as u8]).await;
            }
        }

        async fn set_error(&mut self, e: ImprovError) {
            self.error = e;
            if let Some(tx) = &mut self.error_tx {
                let _ = tx.notify(vec![e as u8]).await;
            }
        }

        /// Is `peer` the live session, opened by `kind`? Refreshes its idle
        /// clock, so an active setup never times out mid-flow.
        fn session_is(&mut self, peer: &str, kind: SessionKind) -> bool {
            let timeout = Duration::from_secs(SESSION_IDLE_TIMEOUT_SECS);
            match &self.session {
                Some((addr, last, k)) if addr == peer && *k == kind && last.elapsed() < timeout => {
                    self.session = Some((peer.to_string(), std::time::Instant::now(), kind));
                    true
                }
                _ => false,
            }
        }

        /// Open the session for `peer`, replacing any stale one.
        fn claim_session(&mut self, peer: &str, kind: SessionKind) {
            self.session = Some((peer.to_string(), std::time::Instant::now(), kind));
        }

        /// Issue a fresh owner challenge to `peer`, replacing only ITS
        /// outstanding one. Expired challenges are dropped on the way, and the
        /// table is capped so a crowd of radios cannot grow it without bound.
        fn issue_challenge(&mut self, peer: &str) -> [u8; 32] {
            use rand::RngCore;
            let ttl = Duration::from_secs(CHALLENGE_TTL_SECS);
            self.challenges.retain(|_, (_, at)| at.elapsed() < ttl);
            if self.challenges.len() >= MAX_CHALLENGES && !self.challenges.contains_key(peer) {
                // Evict the oldest. A real owner answers in seconds, so the
                // oldest entry is the one least likely to be theirs.
                if let Some(oldest) =
                    self.challenges.iter().min_by_key(|(_, (_, at))| *at).map(|(k, _)| k.clone())
                {
                    self.challenges.remove(&oldest);
                }
            }
            let mut nonce = [0u8; 32];
            rand::rng().fill_bytes(&mut nonce);
            self.challenges.insert(peer.to_string(), (nonce, std::time::Instant::now()));
            nonce
        }

        /// Take `peer`'s outstanding challenge if it is still fresh. Taking it
        /// even on the way to a failed proof is the point: one nonce, one try.
        fn take_challenge(&mut self, peer: &str) -> Option<[u8; 32]> {
            let (nonce, at) = self.challenges.remove(peer)?;
            (at.elapsed() < Duration::from_secs(CHALLENGE_TTL_SECS)).then_some(nonce)
        }

        /// Whether some OTHER peer currently holds the session — used only to
        /// log, never to leak who.
        fn session_held_elsewhere(&self, peer: &str) -> bool {
            let timeout = Duration::from_secs(SESSION_IDLE_TIMEOUT_SECS);
            matches!(&self.session, Some((addr, last, _)) if addr != peer && last.elapsed() < timeout)
        }

        async fn send_result(&mut self, packet: Vec<u8>) {
            self.last_result = packet.clone();
            if let Some(tx) = &mut self.result_tx {
                let _ = tx.notify(packet).await;
            }
        }
    }

    /// Start fetching the newest release, right now, because the box has just
    /// come online during setup.
    ///
    /// ## Why here and not on the ordinary timer
    ///
    /// `api::updates` fetches on a 6-hour loop that waits
    /// `PREPARE_FIRST_DELAY` (5 minutes) after boot, so a box would not begin
    /// downloading until well after the owner had finished. That is the right
    /// default — a box coming up has migrations to finish and collectors
    /// reconnecting, and a 120 MB transfer into the middle of that makes an
    /// update the reason a restart felt slow — and it is exactly wrong for this
    /// one moment.
    ///
    /// A box is flashed at manufacture and then sits in a warehouse, so the
    /// build an owner unboxes is usually months old, and the app offers an
    /// update at the end of setup (`offerUpgrade` in `connect.html`). Without
    /// this the release is never staged in time, so that offer reads "Download
    /// and install" and the owner waits out a full transfer at the very end.
    /// With it, the download overlaps the account link and the pairing — the
    /// two steps that are pure human latency — and the offer becomes the
    /// restart it should be.
    ///
    /// Nothing waits on this and nothing is activated by it. `spawn_prepare`
    /// declines on its own for a dev checkout or a prerelease box, and a failure
    /// only means the owner sees the slower version of a screen that still works.
    fn prepare_release_now() {
        tokio::task::spawn_blocking(|| match crate::api::updates::spawn_prepare() {
            Ok(true) => tracing::info!("update: prepare started after the setup join"),
            Ok(false) => {}
            Err(e) => tracing::debug!("update: prepare did not start after the join: {e}"),
        });
    }

    /// One tokio task, mirroring `setup_ap::spawn`. Appliance-only for the
    /// same reason the AP is: a DIY box is someone's own server, and quietly
    /// standing up a radio service on it would be a rude surprise.
    pub fn spawn(pool: PgPool) {
        if !crate::maintenance::setup_ap::is_appliance() {
            tracing::debug!("ble_provision: not an appliance, not serving Improv");
            return;
        }
        tokio::spawn(async move {
            let mut serving: Option<ServeHandles> = None;
            // When a CLAIMED box was first seen offline, for the grace period.
            let mut offline_since: Option<std::time::Instant> = None;
            let mut tick = tokio::time::interval(Duration::from_secs(RECONCILE_SECS));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tick.tick().await;
                // Fails CLOSED (a DB blip must not re-advertise the SETUP
                // service on a claimed box). See `api::pair::is_unclaimed`. A
                // blip on a claimed, offline box lands on the owner-gated
                // service, which admits no one the database cannot vouch for.
                let claimed = !crate::api::pair::is_unclaimed(&pool).await;
                let online = crate::cli::link::has_internet();
                offline_since = match (claimed && !online, offline_since) {
                    (true, None) => Some(std::time::Instant::now()),
                    (true, since) => since,
                    (false, _) => None,
                };
                let asking_owner = offline_since
                    .map(|t| t.elapsed() >= Duration::from_secs(OFFLINE_GRACE_SECS))
                    .unwrap_or(false);
                let want = !claimed || asking_owner;

                // Claim state or connectivity changed since the service came
                // up? Re-serve, so the advertisement and state characteristic
                // tell the truth. (A claimed box going online is covered by
                // `want` turning false.)
                if let Some(h) = &serving {
                    if h.claimed_at_serve != claimed || (!claimed && h.online_at_serve != online) {
                        tracing::info!("ble_provision: claim or connectivity changed, re-serving with fresh state");
                        serving = None;
                    }
                }
                match (want, serving.is_some()) {
                    (false, true) => {
                        tracing::info!("ble_provision: box is claimed and online, stopping Improv service");
                        serving = None; // handles drop → unregister + stop advertising
                    }
                    (true, false) => match serve(pool.clone(), claimed).await {
                        Ok(h) => {
                            if claimed {
                                tracing::warn!("ble_provision: claimed box offline, advertising for its owner");
                            } else {
                                tracing::info!("ble_provision: Improv service up, advertising");
                            }
                            serving = Some(h);
                        }
                        Err(e) => {
                            // Not fatal, and worth being quiet about after the
                            // first time: a box with no BT module ends up here
                            // every tick.
                            tracing::debug!("ble_provision: cannot serve: {e:#}");
                        }
                    },
                    _ => {}
                }
            }
        });
    }

    /// Everything that must stay alive for the service to exist. Dropping it
    /// unregisters the GATT application and stops the advertisement.
    struct ServeHandles {
        _adv: bluer::adv::AdvertisementHandle,
        _app: bluer::gatt::local::ApplicationHandle,
        _session: bluer::Session,
        /// Connectivity at serve time. The advertisement's state byte is baked
        /// in at creation, so when this stops matching reality the whole
        /// service is re-served. Without it the box kept advertising "already
        /// online" for hours after losing its network — the app told the user
        /// to tap a chip that could not exist (seen live 2026-08-11).
        online_at_serve: bool,
        /// Whether this is the owner-gated service. The first pairing flips a
        /// box from one to the other, and the state byte must follow.
        claimed_at_serve: bool,
    }

    async fn serve(pool: PgPool, claimed: bool) -> bluer::Result<ServeHandles> {
        use bluer::gatt::local::{
            Application, Characteristic, CharacteristicNotify, CharacteristicNotifyMethod,
            CharacteristicRead, CharacteristicWrite, CharacteristicWriteMethod, Service,
        };

        let session = bluer::Session::new().await?;
        let adapter = session.default_adapter().await?;
        adapter.set_powered(true).await?;

        // Online now? Then we are already provisioned and the advertisement
        // says so — the app uses that to skip straight to discovery.
        // has_internet, not primary_ip: a captive guest network hands out
        // IPs while blocking traffic, and advertising Provisioned on one
        // routes the app away from the wifi picker the owner still needs.
        //
        // A claimed box only serves while offline and says so with the one
        // state the spec reserves for "prove yourself first".
        let online = crate::cli::link::has_internet();
        let initial = if claimed {
            State::AuthorizationRequired
        } else if online {
            State::Provisioned
        } else {
            State::Authorized
        };
        let improv = Arc::new(Mutex::new(Improv::new(initial)));

        let service_uuid: bluer::Uuid = SERVICE_UUID.parse().expect("static uuid");
        // Same name the SoftAP would broadcast — one recognizable identity
        // per box across every setup surface.
        let name = crate::maintenance::setup_ap::ap_ssid();

        // SET THE ADAPTER ALIAS TOO, not just the advertisement's local name.
        //
        // macOS reads back "virtues" — the BlueZ adapter alias — rather than
        // the name below, so every box in range presents as the same word and
        // the codename never reaches the client (measured 2026-08-13 by logging
        // what CoreBluetooth actually hands the app). That is the exact failure
        // the codename was introduced to fix: two boxes in one house showing as
        // identical chips.
        //
        // The cause is packet budget. A legacy LE advertisement is 31 bytes and
        // this one already carries flags, a 128-bit service UUID (18 bytes) and
        // service data; a 22-character name cannot also fit, so it is dropped
        // and the client falls back to the adapter's name. Setting the alias
        // makes that fallback correct instead of anonymous — cheaper and more
        // reliable than fighting for room in the packet.
        if let Err(e) = adapter.set_alias(name.clone()).await {
            tracing::warn!(error = %e, "ble_provision: could not set adapter alias — the box may advertise as its hostname");
        }

        let adv = bluer::adv::Advertisement {
            advertisement_type: bluer::adv::Type::Peripheral,
            service_uuids: [service_uuid].into_iter().collect(),
            service_data: [(
                bluer::Uuid::from_u128((SERVICE_DATA_UUID_16 as u128) << 96 | 0x1000_8000_0080_5F9B_34FB),
                service_data(initial),
            )]
            .into_iter()
            .collect(),
            discoverable: Some(true),
            local_name: Some(name.clone()),
            ..Default::default()
        };
        let adv_handle = adapter.advertise(adv).await?;

        let app = Application {
            services: vec![Service {
                uuid: service_uuid,
                primary: true,
                characteristics: vec![
                    // capabilities: static read
                    Characteristic {
                        uuid: CHAR_CAPABILITIES.parse().expect("static uuid"),
                        read: Some(CharacteristicRead {
                            read: true,
                            fun: Box::new(|_req| async move { Ok(vec![0x00u8]) }.boxed()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    // current state: read + notify
                    Characteristic {
                        uuid: CHAR_CURRENT_STATE.parse().expect("static uuid"),
                        read: Some(CharacteristicRead {
                            read: true,
                            fun: {
                                let improv = improv.clone();
                                Box::new(move |_req| {
                                    let improv = improv.clone();
                                    async move { Ok(vec![improv.lock().await.state as u8]) }
                                        .boxed()
                                })
                            },
                            ..Default::default()
                        }),
                        notify: Some(CharacteristicNotify {
                            notify: true,
                            method: CharacteristicNotifyMethod::Fun({
                                let improv = improv.clone();
                                Box::new(move |notifier| {
                                    let improv = improv.clone();
                                    async move {
                                        improv.lock().await.state_tx = Some(notifier);
                                    }
                                    .boxed()
                                })
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    // error state: read + notify
                    Characteristic {
                        uuid: CHAR_ERROR_STATE.parse().expect("static uuid"),
                        read: Some(CharacteristicRead {
                            read: true,
                            fun: {
                                let improv = improv.clone();
                                Box::new(move |_req| {
                                    let improv = improv.clone();
                                    async move { Ok(vec![improv.lock().await.error as u8]) }
                                        .boxed()
                                })
                            },
                            ..Default::default()
                        }),
                        notify: Some(CharacteristicNotify {
                            notify: true,
                            method: CharacteristicNotifyMethod::Fun({
                                let improv = improv.clone();
                                Box::new(move |notifier| {
                                    let improv = improv.clone();
                                    async move {
                                        improv.lock().await.error_tx = Some(notifier);
                                    }
                                    .boxed()
                                })
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    // rpc command: write
                    Characteristic {
                        uuid: CHAR_RPC_COMMAND.parse().expect("static uuid"),
                        write: Some(CharacteristicWrite {
                            write: true,
                            write_without_response: true,
                            method: CharacteristicWriteMethod::Fun({
                                let improv = improv.clone();
                                let pool = pool.clone();
                                Box::new(move |value, req| {
                                    let improv = improv.clone();
                                    let pool = pool.clone();
                                    let peer = req.device_address.to_string();
                                    async move {
                                        handle_rpc(improv, pool, value, peer).await;
                                        Ok(())
                                    }
                                    .boxed()
                                })
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    // rpc result: read + notify
                    Characteristic {
                        uuid: CHAR_RPC_RESULT.parse().expect("static uuid"),
                        read: Some(CharacteristicRead {
                            read: true,
                            fun: {
                                let improv = improv.clone();
                                Box::new(move |_req| {
                                    let improv = improv.clone();
                                    async move { Ok(improv.lock().await.last_result.clone()) }
                                        .boxed()
                                })
                            },
                            ..Default::default()
                        }),
                        notify: Some(CharacteristicNotify {
                            notify: true,
                            method: CharacteristicNotifyMethod::Fun({
                                let improv = improv.clone();
                                Box::new(move |notifier| {
                                    let improv = improv.clone();
                                    async move {
                                        improv.lock().await.result_tx = Some(notifier);
                                    }
                                    .boxed()
                                })
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        };
        let app_handle = adapter.serve_gatt_application(app).await?;

        Ok(ServeHandles {
            _adv: adv_handle,
            _app: app_handle,
            _session: session,
            online_at_serve: online,
            claimed_at_serve: claimed,
        })
    }

    /// Execute one RPC. Runs inside the BLE write callback; the join itself is
    /// spawned so a slow `nmcli` cannot stall the GATT event loop.
    async fn handle_rpc(
        improv: Arc<Mutex<Improv>>,
        pool: PgPool,
        packet: Vec<u8>,
        peer: String,
    ) {
        let cmd = match parse_rpc(&packet) {
            Ok(c) => c,
            Err(e) => {
                improv.lock().await.set_error(e).await;
                return;
            }
        };
        // A new command clears the previous error — the client is acting again.
        improv.lock().await.set_error(ImprovError::None).await;

        // ── which box are we? ──
        //
        // Asked per command, not fixed at serve time: the first pairing flips
        // an unclaimed box to claimed mid-conversation. Fails closed — a DB
        // error reads as claimed, the stricter of the two gates.
        let claimed = !crate::api::pair::is_unclaimed(&pool).await;
        if claimed {
            owner_gate(improv, pool, cmd, peer).await;
            return;
        }
        if matches!(cmd, Command::OwnerChallenge | Command::OwnerProve { .. }) {
            // Nobody owns an unclaimed box yet; its door is the phrase.
            improv.lock().await.set_error(ImprovError::NotAuthorized).await;
            return;
        }
        dispatch(improv, pool, cmd, peer, SessionKind::Setup).await;
    }

    /// The claimed box's door. Only the owner challenge and proof are open;
    /// wifi, the scan and device info need an OWNER session; setup's own
    /// commands (phrase, grant, pair) are refused outright — a claimed box is
    /// set up, and none of them has a meaning here that is not an attack.
    ///
    /// The scan and device info are gated too, unlike on an unclaimed box: a
    /// claimed box belongs to someone, and which networks it can see and what
    /// it runs are theirs, not the corridor's.
    async fn owner_gate(improv: Arc<Mutex<Improv>>, pool: PgPool, cmd: Command, peer: String) {
        match cmd {
            Command::OwnerChallenge => {
                let nonce = improv.lock().await.issue_challenge(&peer);
                improv
                    .lock()
                    .await
                    .send_result(build_result(0x88, &[&hex::encode(nonce)]))
                    .await;
            }
            Command::OwnerProve { endpoint_id, signature } => {
                let Some(nonce) = improv.lock().await.take_challenge(&peer) else {
                    tracing::warn!("ble_provision: owner proof with no live challenge");
                    improv.lock().await.set_error(ImprovError::NotAuthorized).await;
                    return;
                };
                let Some(id) = verify_owner_signature(&nonce, &endpoint_id, &signature) else {
                    tracing::warn!("ble_provision: owner proof failed its signature");
                    improv.lock().await.set_error(ImprovError::NotAuthorized).await;
                    return;
                };
                // The signature proves the key; the allowlist proves the key is
                // one of ours and not revoked. A query error refuses — it must
                // never read as "paired".
                match crate::relay::is_paired_endpoint(&pool, &id).await {
                    Ok(true) => {
                        let mut g = improv.lock().await;
                        g.claim_session(&peer, SessionKind::Owner);
                        g.set_state(State::Authorized).await;
                        g.send_result(build_result(0x89, &["ok"])).await;
                        tracing::info!(device = %id.fmt_short(), "ble_provision: owner session opened");
                    }
                    Ok(false) => {
                        tracing::warn!(device = %id.fmt_short(), "ble_provision: owner proof from a device this box does not trust");
                        improv.lock().await.set_error(ImprovError::NotAuthorized).await;
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "ble_provision: owner proof — device lookup failed, refusing");
                        improv.lock().await.set_error(ImprovError::NotAuthorized).await;
                    }
                }
            }
            Command::WifiSettings { .. }
            | Command::EnterpriseSettings { .. }
            | Command::ScanWifi
            | Command::DeviceInfo
            | Command::Identify => {
                if !improv.lock().await.session_is(&peer, SessionKind::Owner) {
                    tracing::warn!("ble_provision: refusing a command on a claimed box — no owner session");
                    improv.lock().await.set_error(ImprovError::NotAuthorized).await;
                    return;
                }
                dispatch(improv, pool, cmd, peer, SessionKind::Owner).await;
            }
            Command::ClaimSetup { .. } | Command::ClaimGrant { .. } | Command::PairConsume { .. } => {
                tracing::warn!("ble_provision: refusing a setup command on a claimed box");
                improv.lock().await.set_error(ImprovError::NotAuthorized).await;
            }
        }
    }

    /// Run a command the gate has already admitted. `kind` is the session the
    /// caller's gate requires for configuring commands; the owner gate checks
    /// its own before calling, so for it this re-check is a no-op refresh.
    async fn dispatch(
        improv: Arc<Mutex<Improv>>,
        pool: PgPool,
        cmd: Command,
        peer: String,
        kind: SessionKind,
    ) {
        // ── the gate ──
        //
        // Everything that CONFIGURES the box requires a claimed setup session,
        // and a session is only opened by proving the four-word phrase printed
        // on the box's own panel (`api::setup_phrase`). Without this, a box
        // advertising Improv while unclaimed would take orders from anyone in
        // radio range — and radio range passes through walls, which is the whole
        // reason the phrase exists.
        //
        // DeviceInfo, Identify and ScanWifi stay open: they are what a client
        // needs to show a useful picker BEFORE the person has typed anything,
        // and none of them change the box. The scan does leak which networks the
        // box can see, which is a small, deliberate cost for a picker that works
        // before authorization.
        let needs_session = matches!(
            cmd,
            Command::WifiSettings { .. }
                | Command::EnterpriseSettings { .. }
                | Command::ClaimGrant { .. }
                // Session-authorized AND codeless: the session is the whole
                // proof of presence, and the box hands its own standing code
                // to itself (see the handler). 0x84/0x85 — the RPCs that used
                // to hand codes to the app — were deleted 2026-08-24.
                | Command::PairConsume { .. }
        );
        if needs_session && !improv.lock().await.session_is(&peer, kind) {
            let held = improv.lock().await.session_held_elsewhere(&peer);
            tracing::warn!(
                held_by_another = held,
                "ble_provision: refusing a configuring command — no setup session"
            );
            improv.lock().await.set_error(ImprovError::NotAuthorized).await;
            return;
        }
        if needs_session && kind == SessionKind::Setup {
            // Authorized work is happening: keep the panel's "setting up with…"
            // line alive. Empty label — the name came with the claim and is
            // held there; see `setup_phrase::note_session`.
            crate::api::setup_phrase::note_session("");
        }

        match cmd {
            Command::WifiSettings { ssid, password } => {
                {
                    improv.lock().await.set_state(State::Provisioning).await;
                }
                let improv = improv.clone();
                tokio::spawn(async move {
                    // Same join the SoftAP portal used — one implementation of
                    // the switchover. With no AP hosted, this is just a plain
                    // nmcli connect and the radio is free the whole time.
                    let psk = (!password.is_empty()).then_some(password.as_str());
                    match crate::api::provision::perform_join_full(&ssid, psk, None).await {
                        None => {
                            let url = crate::cli::link::primary_ip()
                                .map(|ip| format!("http://{ip}:8000"))
                                .unwrap_or_default();
                            let mut g = improv.lock().await;
                            g.send_result(build_result(0x01, &[&url])).await;
                            g.set_state(State::Provisioned).await;
                            tracing::info!(%ssid, "ble_provision: joined via Improv");
                            drop(g);
                            prepare_release_now();
                        }
                        Some(detail) => {
                            let mut g = improv.lock().await;
                            g.set_error(ImprovError::UnableToConnect).await;
                            g.set_state(State::Authorized).await;
                            tracing::warn!(%ssid, %detail, "ble_provision: join failed");
                        }
                    }
                });
            }
            Command::EnterpriseSettings { ssid, identity, password } => {
                {
                    improv.lock().await.set_state(State::Provisioning).await;
                }
                let improv = improv.clone();
                tokio::spawn(async move {
                    match crate::api::provision::perform_join_full(
                        &ssid,
                        (!password.is_empty()).then_some(password.as_str()),
                        Some(&identity),
                    )
                    .await
                    {
                        None => {
                            let url = crate::cli::link::primary_ip()
                                .map(|ip| format!("http://{ip}:8000"))
                                .unwrap_or_default();
                            let mut g = improv.lock().await;
                            g.send_result(build_result(0x81, &[&url])).await;
                            g.set_state(State::Provisioned).await;
                            tracing::info!(%ssid, "ble_provision: enterprise join via Improv-ext");
                            drop(g);
                            prepare_release_now();
                        }
                        Some(detail) => {
                            let mut g = improv.lock().await;
                            g.set_error(ImprovError::UnableToConnect).await;
                            g.set_state(State::Authorized).await;
                            tracing::warn!(%ssid, %detail, "ble_provision: enterprise join failed");
                        }
                    }
                });
            }
            Command::ClaimGrant { grant } => {
                let improv = improv.clone();
                tokio::spawn(async move {
                    // REFUSE once this box already holds an account key. BLE is
                    // unauthenticated and reaches through walls, so without this
                    // anyone in radio range could bind a stranger's box to their
                    // own atlas account — or replace the link its owner just paid
                    // for — with a single write. `store_api_key` UPDATEs rather
                    // than refusing, so the overwrite would succeed silently.
                    // Unlinking is a deliberate act, never a side effect of a
                    // packet.
                    if crate::virtues_api::renew::read_api_key(&pool)
                        .await
                        .ok()
                        .flatten()
                        .is_some()
                    {
                        tracing::warn!("ble_provision: refusing claim grant — box is already linked");
                        improv.lock().await.set_error(ImprovError::NotAuthorized).await;
                        return;
                    }
                    match crate::virtues_api::link::inject_grant(&pool, &grant).await {
                        Ok(()) => {
                            // ACK the store immediately — the redeem may outlive
                            // this BLE session (the box may not even have wifi
                            // yet; grant-then-join and join-then-grant are both
                            // legal orders).
                            {
                                let mut g = improv.lock().await;
                                g.send_result(build_result(0x82, &["accepted"])).await;
                            }
                            tracing::info!("ble_provision: claim grant accepted, awaiting redeem");
                            redeem_grant(pool, improv).await;
                        }
                        Err(e) => {
                            tracing::warn!(error = %format!("{e:#}"), "ble_provision: claim grant rejected");
                            improv.lock().await.set_error(ImprovError::Unknown).await;
                        }
                    }
                });
            }
            Command::PairConsume { kind, source, label, endpoint_id } => {
                let improv = improv.clone();
                tokio::spawn(async move {
                    let body = pair_over_ble(pool, kind, source, label, endpoint_id).await;
                    let mut g = improv.lock().await;
                    for chunk in chunk_for_results(&body) {
                        g.send_result(build_result(0x83, &[&chunk])).await;
                    }
                    // Empty terminator — same stream shape as ScanWifi.
                    g.send_result(build_result(0x83, &[])).await;
                });
            }
            Command::ClaimSetup { phrase, label } => {
                let improv = improv.clone();
                tokio::spawn(async move {
                    if crate::api::setup_phrase::verify(&pool, &phrase).await {
                        let mut g = improv.lock().await;
                        g.claim_session(&peer, SessionKind::Setup);
                        // The words are spent, so they leave the panel and this
                        // name takes their place — confirmation ON THE BOX that
                        // what the owner typed landed here, and a race they did
                        // not start showing up as a name they don't know.
                        crate::api::setup_phrase::note_session(&label);
                        g.send_result(build_result(0x86, &["ok"])).await;
                        tracing::info!(device = %label, "ble_provision: setup session claimed");
                    } else {
                        // Deliberately says nothing about WHY: wrong words and a
                        // spent attempt budget look identical from outside, so a
                        // guesser learns nothing from the shape of the refusal.
                        tracing::warn!("ble_provision: setup phrase rejected");
                        improv.lock().await.set_error(ImprovError::NotAuthorized).await;
                    }
                });
            }
            // Handled by `owner_gate`; an unclaimed box refused them above.
            Command::OwnerChallenge | Command::OwnerProve { .. } => {}
            // 0x84 (LinkCode) and 0x85 (PairCode) handlers were deleted
            // 2026-08-24 with their opcodes — the grant (0x82) and the codeless
            // 0x83 made both code hand-offs pointless. parse_rpc answers the
            // dead opcodes with UnknownCommand before dispatch reaches here.
            Command::Identify => {
                // No LED, no sound. Acknowledged silently; the display is the
                // box's face and belongs to its own subsystem.
            }
            Command::DeviceInfo => {
                let version = crate::VERSION.to_string();
                let host = crate::cli::link::mdns_host();
                let packet =
                    build_result(0x03, &["Virtues", &version, "Dragon Q6A", &host]);
                improv.lock().await.send_result(packet).await;
            }
            Command::ScanWifi => {
                let improv = improv.clone();
                tokio::spawn(async move {
                    // One result packet per network, then an empty terminator —
                    // the streaming shape ESPHome clients expect.
                    let nets = crate::api::provision::scan_or_cached().await.unwrap_or_default();
                    for n in &nets {
                        let rssi = n.signal.to_string();
                        // "ENT" extends Improv's YES/NO — 802.1X networks need
                        // a username the base protocol cannot carry, so the
                        // client must know to route them elsewhere. Foreign
                        // Improv clients render the string harmlessly.
                        let auth = if n.enterprise {
                            "ENT"
                        } else if n.secured {
                            "YES"
                        } else {
                            "NO"
                        };
                        let packet = build_result(0x04, &[&n.ssid, &rssi, auth]);
                        improv.lock().await.send_result(packet).await;
                    }
                    improv.lock().await.send_result(build_result(0x04, &[])).await;
                });
            }
        }
    }

    /// Redeem a pair code arriving over BLE against the box's own consume
    /// endpoint. Loopback on purpose: `POST /api/pair/consume` is the ONE
    /// implementation of enrollment (token claim, device row, allowlist,
    /// collector fan-out, reach ticket), and this leg must not fork it — BLE
    /// is just the wire for LANs that block peer-to-peer.
    ///
    /// Returns the response body to stream back: the consume JSON on success,
    /// or `error:<code>` on failure.
    ///
    /// The consume handler's per-IP rate limiter EXEMPTS loopback (a header-
    /// less local caller isn't remotely reachable), which this path would
    /// otherwise turn into a free brute-force budget for anyone in radio
    /// range — so BLE brings its own: same 10-per-30-minutes the LAN leg
    /// enforces, process-wide.
    async fn pair_over_ble(
        pool: PgPool,
        kind: String,
        source: String,
        label: String,
        endpoint_id: String,
    ) -> String {
        use std::time::Instant;
        static ATTEMPTS: std::sync::Mutex<Vec<Instant>> = std::sync::Mutex::new(Vec::new());
        {
            let mut a = ATTEMPTS.lock().expect("ble pair limiter");
            a.retain(|t| t.elapsed() < Duration::from_secs(1800));
            if a.len() >= 10 {
                tracing::warn!("ble_provision: pair attempts rate-limited");
                return "error:too_many_attempts".into();
            }
            a.push(Instant::now());
        }

        // ── COMPLETE THE TICKET BEFORE ANSWERING ──
        //
        // Pair is the LAST word this radio ever says: a successful pair claims
        // the box and the reconciler tears the BLE service down within one
        // tick, so there is no "refresh it later" — whatever reach ticket
        // rides in this answer is the only one the device will ever hold. A
        // ticket frozen without the relay stranded a real setup on an
        // isolating LAN (2026-08-24): direct addrs unroutable, relay unknown,
        // no repair channel. So, bounded, in order:
        //   1. a grant redeem in flight → wait for it to land (the link is
        //      what buys the relay at all);
        //   2. linked → wait for the post-link rebind to bring the relay up.
        // A box with no link and none in flight waits for neither — LAN-only
        // is the skip path's honest story, and nothing here blocks it.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(45);
        loop {
            let linked = crate::virtues_api::renew::read_api_key(&pool)
                .await
                .ok()
                .flatten()
                .is_some();
            if linked {
                // Wait for the rebind to home on the relay; give up quietly at
                // the deadline — a late relay is degraded, not fatal, and
                // `refresh_reach` over LAN can still repair it on sane networks.
                if crate::relay::box_relay_url().is_some() && crate::relay::is_relay_registered() {
                    break;
                }
            } else {
                let inflight = crate::virtues_api::link::inflight(&pool)
                    .await
                    .ok()
                    .flatten()
                    .is_some();
                if !inflight {
                    break; // skip-link path: nothing to wait for
                }
                // Grant redeem in flight — keep waiting for it to land.
            }
            if tokio::time::Instant::now() >= deadline {
                tracing::warn!("ble_provision: pairing with an incomplete reach ticket (relay/link still settling)");
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // The session already proved line of sight, so the box supplies its
        // OWN standing code — the same one the panel would print — and hands
        // it to itself. Same transaction, rate-limit story, and device row as
        // every other pairing; the transcription ceremony is what died.
        let code = match crate::api::pair::ensure_standing(&pool).await {
            Ok(m) => m.token,
            Err(e) => {
                tracing::warn!(error = %format!("{e:#}"), "ble_provision: no standing pair code");
                return "error:internal".into();
            }
        };

        let opt = |s: String| (!s.is_empty()).then_some(s);
        let body = serde_json::json!({
            "token": code,
            "kind": kind,
            "source": opt(source),
            "label": opt(label),
            "device_node_id": opt(endpoint_id),
        });
        let client = match reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
        {
            Ok(c) => c,
            Err(_) => return "error:internal".into(),
        };
        // The box's own listener; the same literal the 0x01 result URL uses.
        let resp = client
            .post("http://127.0.0.1:8000/api/pair/consume")
            .json(&body)
            .send()
            .await;
        match resp {
            Ok(r) if r.status().is_success() => match r.text().await {
                Ok(t) => {
                    tracing::info!("ble_provision: device paired over BLE");
                    t
                }
                Err(_) => "error:internal".into(),
            },
            Ok(r) => {
                let code = r
                    .json::<serde_json::Value>()
                    .await
                    .ok()
                    .and_then(|v| v["error"].as_str().map(str::to_string))
                    .unwrap_or_else(|| "internal".into());
                tracing::warn!(%code, "ble_provision: pair consume refused");
                format!("error:{code}")
            }
            Err(e) => {
                tracing::warn!(error = %e, "ble_provision: pair consume unreachable");
                "error:internal".into()
            }
        }
    }

    /// Redeem an injected claim grant: wait for internet (the grant usually
    /// arrives before or seconds after the wifi credentials), then drive the
    /// normal link poll to a terminal state. On `Ready` the poll machinery
    /// stores the api key, fetches relay config, and requests the endpoint
    /// rebind — this task adds nothing to that path, it only supplies the
    /// heartbeat that the display's screen-2 loop would otherwise be.
    ///
    /// Results are best-effort notified over BLE (0x82 "linked") for the app
    /// that is still connected; the authoritative signal is the box's own
    /// state (`linked`, and the advertisement flipping at claim).
    async fn redeem_grant(pool: PgPool, improv: Arc<Mutex<Improv>>) {
        // Generous ceiling: the owner may be slow picking wifi after the app
        // sent the grant. Atlas's own grant expiry is the real limit; this one
        // only stops a box that never gets online from polling forever.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(1800);
        while !crate::cli::link::has_internet() {
            if tokio::time::Instant::now() >= deadline {
                tracing::warn!("ble_provision: claim grant never got online — giving up (grant stays in-flight for the display loop)");
                return;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        let http = crate::http_client::virtues_api_client();
        let atlas = crate::virtues_api::atlas_url();
        loop {
            if tokio::time::Instant::now() >= deadline {
                tracing::warn!("ble_provision: claim grant redeem timed out");
                return;
            }
            match crate::virtues_api::link::poll(&pool, &http, &atlas).await {
                Ok(crate::virtues_api::link::LinkStatus::Ready) => {
                    tracing::info!("ble_provision: box linked via app claim grant");
                    let mut g = improv.lock().await;
                    g.send_result(build_result(0x82, &["linked"])).await;
                    return;
                }
                Ok(crate::virtues_api::link::LinkStatus::Expired) => {
                    tracing::warn!("ble_provision: claim grant expired or was denied");
                    improv.lock().await.set_error(ImprovError::NotAuthorized).await;
                    return;
                }
                // Cleared by someone else (a display-loop redeem finishing
                // first lands here) — nothing left to drive.
                Ok(crate::virtues_api::link::LinkStatus::None) => return,
                Ok(crate::virtues_api::link::LinkStatus::Pending) => {}
                Err(e) => {
                    tracing::debug!(error = %format!("{e:#}"), "ble_provision: grant poll failed; retrying");
                }
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    }
}

#[cfg(target_os = "linux")]
pub use server::spawn;

/// On non-Linux hosts (dev Macs) the service does not exist; the spawn is a
/// no-op so `server::run` can call it unconditionally.
#[cfg(not(target_os = "linux"))]
pub fn spawn(_pool: sqlx::PgPool) {}

#[cfg(test)]
mod tests {
    use super::*;
    use virtues_iroh::SecretKey;

    fn device() -> SecretKey {
        SecretKey::from_bytes(&[7u8; 32])
    }

    fn sign(key: &SecretKey, nonce: &[u8]) -> String {
        hex::encode(key.sign(&owner_proof_message(nonce)).to_bytes())
    }

    #[test]
    fn a_paired_devices_signature_over_the_nonce_verifies() {
        let key = device();
        let nonce = [42u8; 32];
        let id = verify_owner_signature(&nonce, &key.public().to_string(), &sign(&key, &nonce));
        assert_eq!(id, Some(key.public()));
    }

    #[test]
    fn a_signature_over_another_nonce_is_refused() {
        // The replay case: a proof captured for one challenge must be worthless
        // for the next. The box never reissues a nonce, and this pins that a
        // proof is bound to the nonce it signed.
        let key = device();
        let sig = sign(&key, &[1u8; 32]);
        assert_eq!(verify_owner_signature(&[2u8; 32], &key.public().to_string(), &sig), None);
    }

    #[test]
    fn someone_elses_key_cannot_sign_for_a_paired_device() {
        let paired = device();
        let stranger = SecretKey::from_bytes(&[9u8; 32]);
        let nonce = [3u8; 32];
        let forged = sign(&stranger, &nonce);
        assert_eq!(verify_owner_signature(&nonce, &paired.public().to_string(), &forged), None);
    }

    #[test]
    fn a_bare_signature_without_the_context_is_refused() {
        // Domain separation: a signature over the raw nonce (what some other
        // protocol might one day ask this key to sign) must not open the box.
        let key = device();
        let nonce = [4u8; 32];
        let raw = hex::encode(key.sign(&nonce).to_bytes());
        assert_eq!(verify_owner_signature(&nonce, &key.public().to_string(), &raw), None);
    }

    #[test]
    fn malformed_proofs_are_refused_not_panicked_on() {
        let key = device();
        let id = key.public().to_string();
        assert_eq!(verify_owner_signature(&[0u8; 32], &id, "zz"), None);
        assert_eq!(verify_owner_signature(&[0u8; 32], &id, "abcd"), None);
        assert_eq!(verify_owner_signature(&[0u8; 32], "not-a-key", &"00".repeat(64)), None);
    }
}
