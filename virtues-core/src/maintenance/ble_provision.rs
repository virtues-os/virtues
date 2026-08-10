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

// ─── Improv protocol: constants ─────────────────────────────────────────────

/// Primary service UUID, from the spec. The advertisement carries it.
pub const SERVICE_UUID: &str = "00467768-6228-2272-4663-277478268000";
/// 16-bit UUID `0x4677` for the advertisement's service-data field.
pub const SERVICE_DATA_UUID_16: u16 = 0x4677;

pub const CHAR_CURRENT_STATE: &str = "00467768-6228-2272-4663-277478268001";
pub const CHAR_ERROR_STATE: &str = "00467768-6228-2272-4663-277478268002";
pub const CHAR_RPC_COMMAND: &str = "00467768-6228-2272-4663-277478268003";
pub const CHAR_RPC_RESULT: &str = "00467768-6228-2272-4663-277478268004";
pub const CHAR_CAPABILITIES: &str = "00467768-6228-2272-4663-277478268005";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum State {
    /// Unused here — see the module docs on authorization.
    AuthorizationRequired = 0x01,
    Authorized = 0x02,
    Provisioning = 0x03,
    Provisioned = 0x04,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ImprovError {
    None = 0x00,
    InvalidPacket = 0x01,
    UnknownCommand = 0x02,
    UnableToConnect = 0x03,
    NotAuthorized = 0x04,
    Unknown = 0xFF,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `0x01` — the whole point: SSID + passphrase.
    WifiSettings { ssid: String, password: String },
    /// `0x02` — blink/identify. We have no LED to blink; acknowledged no-op.
    Identify,
    /// `0x03` — firmware name/version/hardware/device name.
    DeviceInfo,
    /// `0x04` — the box's own wifi scan, streamed back one network per result.
    ScanWifi,
}

// ─── Improv protocol: framing ───────────────────────────────────────────────
//
// Every RPC packet is `[command, data_len, data…, checksum]`, where the
// checksum is the low byte of the sum of everything before it. Strings inside
// data are length-prefixed. Kept as pure functions: this is the part of the
// module that must be correct on every platform and provable without a radio.

/// Low byte of the byte-sum — the spec's whole integrity story.
fn checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |a, b| a.wrapping_add(*b))
}

/// Parse one RPC command packet.
pub fn parse_rpc(packet: &[u8]) -> Result<Command, ImprovError> {
    // Minimum: command + len + checksum.
    if packet.len() < 3 {
        return Err(ImprovError::InvalidPacket);
    }
    let (body, ck) = packet.split_at(packet.len() - 1);
    if checksum(body) != ck[0] {
        return Err(ImprovError::InvalidPacket);
    }
    let data_len = body[1] as usize;
    if body.len() != 2 + data_len {
        return Err(ImprovError::InvalidPacket);
    }
    let data = &body[2..];
    match body[0] {
        0x01 => {
            let (ssid, rest) = take_string(data).ok_or(ImprovError::InvalidPacket)?;
            let (password, rest) = take_string(rest).ok_or(ImprovError::InvalidPacket)?;
            if !rest.is_empty() {
                return Err(ImprovError::InvalidPacket);
            }
            Ok(Command::WifiSettings { ssid, password })
        }
        0x02 => Ok(Command::Identify),
        0x03 => Ok(Command::DeviceInfo),
        0x04 => Ok(Command::ScanWifi),
        _ => Err(ImprovError::UnknownCommand),
    }
}

fn take_string(data: &[u8]) -> Option<(String, &[u8])> {
    let len = *data.first()? as usize;
    if data.len() < 1 + len {
        return None;
    }
    let s = String::from_utf8(data[1..1 + len].to_vec()).ok()?;
    Some((s, &data[1 + len..]))
}

/// Build an RPC result packet: the echoed command id + length-prefixed strings.
pub fn build_result(command_id: u8, strings: &[&str]) -> Vec<u8> {
    let mut data = Vec::new();
    for s in strings {
        // The length prefix is one byte; Improv strings cannot exceed 255.
        let bytes = s.as_bytes();
        let take = bytes.len().min(255);
        data.push(take as u8);
        data.extend_from_slice(&bytes[..take]);
    }
    let mut packet = Vec::with_capacity(data.len() + 3);
    packet.push(command_id);
    packet.push(data.len() as u8);
    packet.extend_from_slice(&data);
    packet.push(checksum(&packet));
    packet
}

/// The advertisement's service-data payload: state, capabilities, 4 reserved.
///
/// This must ride in the same advertisement as the service UUID (spec) — it is
/// how a client can render "ready to set up" vs "already online" in its picker
/// without connecting.
pub fn service_data(state: State) -> Vec<u8> {
    // Capabilities 0x00: no identify output on this hardware (no LED the owner
    // can see; the display belongs to a different subsystem).
    vec![state as u8, 0x00, 0x00, 0x00, 0x00, 0x00]
}

// ─── protocol tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(command: u8, data: &[u8]) -> Vec<u8> {
        let mut p = vec![command, data.len() as u8];
        p.extend_from_slice(data);
        p.push(checksum(&p));
        p
    }

    #[test]
    fn wifi_settings_round_trip() {
        // The packet the whole feature exists to receive.
        let mut data = vec![4u8];
        data.extend_from_slice(b"home");
        data.push(8);
        data.extend_from_slice(b"hunter22");
        let cmd = parse_rpc(&frame(0x01, &data)).unwrap();
        assert_eq!(
            cmd,
            Command::WifiSettings { ssid: "home".into(), password: "hunter22".into() }
        );
    }

    #[test]
    fn open_network_has_empty_password() {
        let mut data = vec![4u8];
        data.extend_from_slice(b"cafe");
        data.push(0); // zero-length password — valid, means open network
        let cmd = parse_rpc(&frame(0x01, &data)).unwrap();
        assert_eq!(cmd, Command::WifiSettings { ssid: "cafe".into(), password: "".into() });
    }

    #[test]
    fn a_bad_checksum_is_rejected() {
        let mut p = frame(0x02, &[]);
        *p.last_mut().unwrap() ^= 0xFF;
        assert_eq!(parse_rpc(&p), Err(ImprovError::InvalidPacket));
    }

    #[test]
    fn truncated_and_oversized_data_are_rejected() {
        // data_len says 4 but only 2 bytes present (before checksum).
        let p = vec![0x01, 4, b'a', b'b', 0]; // checksum wrong too, but length first
        assert_eq!(parse_rpc(&p), Err(ImprovError::InvalidPacket));
        // A string length pointing past the data.
        let mut data = vec![200u8];
        data.extend_from_slice(b"x");
        assert_eq!(parse_rpc(&frame(0x01, &data)), Err(ImprovError::InvalidPacket));
    }

    #[test]
    fn unknown_commands_are_distinguished_from_garbage() {
        // A well-formed packet with a command we don't know must say so —
        // clients treat InvalidPacket as "retry" and UnknownCommand as "don't".
        assert_eq!(parse_rpc(&frame(0x7F, &[])), Err(ImprovError::UnknownCommand));
    }

    #[test]
    fn results_carry_their_own_checksum() {
        let p = build_result(0x01, &["http://192.168.1.187:8000"]);
        let (body, ck) = p.split_at(p.len() - 1);
        assert_eq!(checksum(body), ck[0]);
        assert_eq!(p[0], 0x01);
        // Round-trip the string back out.
        let (url, rest) = take_string(&body[2..]).unwrap();
        assert_eq!(url, "http://192.168.1.187:8000");
        assert!(rest.is_empty());
    }

    #[test]
    fn advertisement_service_data_is_six_bytes_state_first() {
        // The picker renders off byte 0 without connecting; the spec fixes the
        // layout. Six bytes, state, capabilities, four reserved zeros.
        let d = service_data(State::Provisioned);
        assert_eq!(d.len(), 6);
        assert_eq!(d[0], 0x04);
        assert_eq!(&d[1..], &[0, 0, 0, 0, 0]);
    }
}

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
    }

    impl Improv {
        fn new(initial: State) -> Self {
            Self {
                state: initial,
                error: ImprovError::None,
                last_result: Vec::new(),
                state_tx: None,
                error_tx: None,
                result_tx: None,
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
                match (claimed, serving.is_some()) {
                    (true, true) => {
                        tracing::info!("ble_provision: box is claimed, stopping Improv service");
                        serving = None; // handles drop → unregister + stop advertising
                    }
                    (false, false) => match serve().await {
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
    }

    async fn serve() -> bluer::Result<ServeHandles> {
        use bluer::gatt::local::{
            Application, Characteristic, CharacteristicNotify, CharacteristicNotifyMethod,
            CharacteristicRead, CharacteristicWrite, CharacteristicWriteMethod, Service,
        };

        let session = bluer::Session::new().await?;
        let adapter = session.default_adapter().await?;
        adapter.set_powered(true).await?;

        // Online now? Then we are already provisioned and the advertisement
        // says so — the app uses that to skip straight to discovery.
        let initial = if crate::cli::link::primary_ip().is_some() {
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
                                Box::new(move |value, _req| {
                                    let improv = improv.clone();
                                    async move {
                                        handle_rpc(improv, value).await;
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

        Ok(ServeHandles { _adv: adv_handle, _app: app_handle, _session: session })
    }

    /// Execute one RPC. Runs inside the BLE write callback; the join itself is
    /// spawned so a slow `nmcli` cannot stall the GATT event loop.
    async fn handle_rpc(improv: Arc<Mutex<Improv>>, packet: Vec<u8>) {
        let cmd = match parse_rpc(&packet) {
            Ok(c) => c,
            Err(e) => {
                improv.lock().await.set_error(e).await;
                return;
            }
        };
        // A new command clears the previous error — the client is acting again.
        improv.lock().await.set_error(ImprovError::None).await;

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
                        let auth = if n.secured { "YES" } else { "NO" };
                        let packet = build_result(0x04, &[&n.ssid, &rssi, auth]);
                        improv.lock().await.send_result(packet).await;
                    }
                    improv.lock().await.send_result(build_result(0x04, &[])).await;
                });
            }
        }
    }
}

#[cfg(target_os = "linux")]
pub use server::spawn;

/// On non-Linux hosts (dev Macs) the service does not exist; the spawn is a
/// no-op so `server::run` can call it unconditionally.
#[cfg(not(target_os = "linux"))]
pub fn spawn(_pool: sqlx::PgPool) {}
