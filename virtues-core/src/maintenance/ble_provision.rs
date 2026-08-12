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
//! exactly buys interop with existing tooling (the improv-wifi.com web tester
//! can provision this box from Chrome via Web Bluetooth, with no Virtues app
//! involved) and keeps our client code boring. Extensions can ride the
//! reserved command space later; the base protocol stays stock.
//!
//! **Lifecycle: advertised while the box is UNCLAIMED, gone once claimed.**
//! Deliberately *not* "while offline": an unclaimed box on ethernet still
//! advertises, in the `Provisioned` state, because the advertisement doubles
//! as discovery — the app can read the box's URL over BLE instead of
//! subnet-scanning, which was its own source of flakiness. Claiming is what
//! ends setup, exactly as with the display's screens and the (frozen) SoftAP.
//!
//! **Authorization: we skip Improv's authorization-required state.** It exists
//! for devices with a button to prove physical possession; this box's only
//! input surface is proximity itself. An attacker in radio range of an
//! unclaimed box can, at worst, put it on a network of their choosing — which
//! breaks the owner's setup *visibly* and confers nothing else, because the
//! actual credential (pairing) still requires the code off the screen. This
//! matches the SoftAP posture and the consumer-IoT norm (Sonos, eero).
//!
//! The GATT plumbing is Linux-only (`bluer` → BlueZ). The protocol layer is
//! platform-free and unit-tested everywhere.

#![allow(dead_code)] // the protocol layer is used only from the linux half

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
    build_result, chunk_for_results, parse_rpc, service_data, Command, ImprovError, State,
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
        /// the phrase is; this only decides *which* proven peer is mid-setup.
        session: Option<(String, std::time::Instant)>,
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

        /// Is `peer` the live setup session? Refreshes its idle clock, so an
        /// active setup never times out mid-flow.
        fn session_is(&mut self, peer: &str) -> bool {
            let timeout = Duration::from_secs(SESSION_IDLE_TIMEOUT_SECS);
            match &self.session {
                Some((addr, last)) if addr == peer && last.elapsed() < timeout => {
                    self.session = Some((peer.to_string(), std::time::Instant::now()));
                    true
                }
                _ => false,
            }
        }

        /// Open the session for `peer`, replacing any stale one.
        fn claim_session(&mut self, peer: &str) {
            self.session = Some((peer.to_string(), std::time::Instant::now()));
        }

        /// Whether some OTHER peer currently holds the session — used only to
        /// log, never to leak who.
        fn session_held_elsewhere(&self, peer: &str) -> bool {
            let timeout = Duration::from_secs(SESSION_IDLE_TIMEOUT_SECS);
            matches!(&self.session, Some((addr, last)) if addr != peer && last.elapsed() < timeout)
        }

        async fn send_result(&mut self, packet: Vec<u8>) {
            self.last_result = packet.clone();
            if let Some(tx) = &mut self.result_tx {
                let _ = tx.notify(packet).await;
            }
        }
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
            let mut tick = tokio::time::interval(Duration::from_secs(RECONCILE_SECS));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tick.tick().await;
                let claimed = crate::api::pair::paired_device_count(&pool).await > 0;
                // Connectivity changed since the service came up? Re-serve, so
                // the advertisement and state characteristic tell the truth.
                if let Some(h) = &serving {
                    if !claimed && h.online_at_serve != crate::cli::link::has_internet() {
                        tracing::info!("ble_provision: connectivity changed, re-serving with fresh state");
                        serving = None;
                    }
                }
                match (claimed, serving.is_some()) {
                    (true, true) => {
                        tracing::info!("ble_provision: box is claimed, stopping Improv service");
                        serving = None; // handles drop → unregister + stop advertising
                    }
                    (false, false) => match serve(pool.clone()).await {
                        Ok(h) => {
                            tracing::info!("ble_provision: Improv service up, advertising");
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
    }

    async fn serve(pool: PgPool) -> bluer::Result<ServeHandles> {
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
        let initial = if crate::cli::link::has_internet() {
            State::Provisioned
        } else {
            State::Authorized
        };
        let improv = Arc::new(Mutex::new(Improv::new(initial)));

        let service_uuid: bluer::Uuid = SERVICE_UUID.parse().expect("static uuid");
        // Same name the SoftAP would broadcast — one recognizable identity
        // per box across every setup surface.
        let name = crate::maintenance::setup_ap::ap_ssid();

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
            online_at_serve: initial == State::Provisioned,
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
                | Command::PairConsume { .. }
        );
        if needs_session && !improv.lock().await.session_is(&peer) {
            let held = improv.lock().await.session_held_elsewhere(&peer);
            tracing::warn!(
                held_by_another = held,
                "ble_provision: refusing a configuring command — no setup session"
            );
            improv.lock().await.set_error(ImprovError::NotAuthorized).await;
            return;
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
                    match crate::api::provision::perform_join(&ssid, psk).await {
                        None => {
                            let url = crate::cli::link::primary_ip()
                                .map(|ip| format!("http://{ip}:8000"))
                                .unwrap_or_default();
                            let mut g = improv.lock().await;
                            g.send_result(build_result(0x01, &[&url])).await;
                            g.set_state(State::Provisioned).await;
                            tracing::info!(%ssid, "ble_provision: joined via Improv");
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
            Command::PairConsume { code, kind, source, label, endpoint_id } => {
                let improv = improv.clone();
                tokio::spawn(async move {
                    let body = pair_over_ble(code, kind, source, label, endpoint_id).await;
                    let mut g = improv.lock().await;
                    for chunk in chunk_for_results(&body) {
                        g.send_result(build_result(0x83, &[&chunk])).await;
                    }
                    // Empty terminator — same stream shape as ScanWifi.
                    g.send_result(build_result(0x83, &[])).await;
                });
            }
            Command::ClaimSetup { phrase } => {
                let improv = improv.clone();
                tokio::spawn(async move {
                    if crate::api::setup_phrase::verify(&pool, &phrase).await {
                        let mut g = improv.lock().await;
                        g.claim_session(&peer);
                        g.send_result(build_result(0x86, &["ok"])).await;
                        tracing::info!("ble_provision: setup session claimed");
                    } else {
                        // Deliberately says nothing about WHY: wrong words and a
                        // spent attempt budget look identical from outside, so a
                        // guesser learns nothing from the shape of the refusal.
                        tracing::warn!("ble_provision: setup phrase rejected");
                        improv.lock().await.set_error(ImprovError::NotAuthorized).await;
                    }
                });
            }
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
        code: String,
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
