//! The desktop Improv client — CoreBluetooth on macOS, WinRT on Windows,
//! BlueZ on Linux, all through `btleplug`.
//!
//! This is what makes a **Mac the first device**: it discovers an unclaimed
//! box in radio range, reads that box's own wifi scan, sends credentials
//! (including 802.1X, which no phone keyboard ever wanted to type), and — on
//! networks that isolate clients — carries the pairing itself. Same four verbs
//! as the iOS client, same wire, so the connect shell drives either platform
//! with one code path:
//!
//!   discover()  → which unclaimed boxes are in radio range?
//!   wifi_scan() → what networks can THAT BOX see? (RPC 0x04)
//!   provision() → put it on one, and watch it happen (0x01 / 0x81)
//!   pair()      → redeem the screen's code through the box itself (0x83)
//!
//! **One operation at a time, by construction** — the shared session is behind
//! a mutex. Setup is a single conversation with a single box; a queue would
//! only add states nobody is in.
//!
//! macOS note: the *app bundle* needs `NSBluetoothAlwaysUsageDescription` or
//! the first scan dies with an authorization error the OS never surfaces to
//! the user. That string lives in the app's Info.plist, not here.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use btleplug::api::{Central, Characteristic, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Adapter, Manager, Peripheral};
use futures::StreamExt;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::protocol::{self, Command, ImprovError, State};

/// A box heard over the air. `id` is opaque and platform-shaped — the caller
/// passes it back, never parses it.
#[derive(Debug, Clone)]
pub struct FoundBox {
    pub id: String,
    pub name: String,
    /// Byte 0 of the advertisement's service data: `0x02` needs wifi, `0x04`
    /// already online. Rendered in the picker WITHOUT connecting.
    pub improv_state: u8,
    pub rssi: i32,
}

/// One network from the box's own scan.
#[derive(Debug, Clone)]
pub struct Network {
    pub ssid: String,
    pub signal: i32,
    pub secured: bool,
    /// 802.1X — needs a username, so the UI must route it to the two-field form.
    pub enterprise: bool,
}

/// How long to wait for a box to answer a streamed reply (its wifi scan takes
/// seconds on a busy band; the pair relay does a local HTTP round trip).
const REPLY_TIMEOUT: Duration = Duration::from_secs(25);
/// A join is bounded by `nmcli`'s own timeout on the box; add slack.
const JOIN_TIMEOUT: Duration = Duration::from_secs(45);
/// Pairing (0x83) is the slowest answer the box gives, ON PURPOSE: it spends
/// up to 45s finishing the reach ticket (grant redeem + relay bind) and then
/// up to 15s on its loopback consume before it says anything. This was
/// `REPLY_TIMEOUT` (25s) until 2026-09-27, so a slow relay made the app report
/// a failed pairing that the box then completed — claiming itself and turning
/// its Bluetooth off under an owner who had just been told to try again.
/// Must exceed 45 + 15; the screen's own deadline sits above this one.
const PAIR_TIMEOUT: Duration = Duration::from_secs(70);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// WHY a call failed, as a class a screen can branch on — never parsed out of
/// words. The strings match `BoxRadioErrorCode` in the web app
/// (`apps/web/src/lib/tauri/boxRadio.ts`) and the Swift client's codes, so one
/// screen handles Mac and iPhone alike.
///
/// Until 2026-09-27 these rode as prefixes on the message (`"refused: …"`)
/// and the plugin split them back out, which is exactly the kind of contract
/// that breaks silently the first time someone rewords an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// Out of range, gone, or would not connect.
    NotFound,
    /// The server said no: wrong words, not its owner, already linked.
    Refused,
    /// The server is older than the command.
    Unsupported,
    /// The server tried the network and could not join it.
    JoinFailed,
    /// The server stopped answering.
    Timeout,
    Failed,
}

impl FailureKind {
    pub fn code(self) -> &'static str {
        match self {
            FailureKind::NotFound => "not-found",
            FailureKind::Refused => "refused",
            FailureKind::Unsupported => "unsupported",
            FailureKind::JoinFailed => "join-failed",
            FailureKind::Timeout => "timeout",
            FailureKind::Failed => "failed",
        }
    }
}

/// A classified failure, carried inside `anyhow::Error` so every signature
/// stays `Result<T>`; [`classify`] reads it back out.
#[derive(Debug)]
pub struct Failure {
    pub kind: FailureKind,
    pub message: String,
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Failure {}

fn fail(kind: FailureKind, message: impl Into<String>) -> anyhow::Error {
    Failure { kind, message: message.into() }.into()
}

/// `(code, words)` for any error this client returned. An unclassified error
/// (a BlueZ or CoreBluetooth failure deep in btleplug) is `failed`, with its
/// top-level message only — the full chain is for logs, not for a person.
pub fn classify(e: &anyhow::Error) -> (&'static str, String) {
    match e.downcast_ref::<Failure>() {
        Some(f) => (f.kind.code(), f.message.clone()),
        None => ("failed", e.to_string()),
    }
}

/// The same timeout words everywhere.
const NO_ANSWER: &str = "Your server didn't answer. Move closer to it and try again.";

struct Session {
    id: String,
    peripheral: Peripheral,
    rpc: Characteristic,
}

#[derive(Default)]
struct Inner {
    adapter: Option<Adapter>,
    /// Everything the last scan heard, so `id` can be resolved to a peripheral.
    found: HashMap<String, Peripheral>,
    session: Option<Session>,
}

pub struct ImprovClient {
    inner: Mutex<Inner>,
}

static CLIENT: OnceLock<ImprovClient> = OnceLock::new();

fn uuid(s: &str) -> Uuid {
    s.parse().expect("static uuid")
}

/// The 16-bit advertisement service-data UUID `0x4677`, in its 128-bit form
/// (the Bluetooth base UUID). btleplug reports service data keyed this way.
fn service_data_uuid() -> Uuid {
    Uuid::from_u128(
        ((protocol::SERVICE_DATA_UUID_16 as u128) << 96) | 0x0000_1000_8000_0080_5F9B_34FB,
    )
}

impl ImprovClient {
    pub fn shared() -> &'static ImprovClient {
        CLIENT.get_or_init(|| ImprovClient { inner: Mutex::new(Inner::default()) })
    }

    async fn adapter(inner: &mut Inner) -> Result<Adapter> {
        if let Some(a) = &inner.adapter {
            return Ok(a.clone());
        }
        let manager = Manager::new().await.context("bluetooth manager")?;
        let adapter = manager
            .adapters()
            .await
            .context("list bluetooth adapters")?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("no Bluetooth adapter on this machine"))?;
        inner.adapter = Some(adapter.clone());
        Ok(adapter)
    }

    /// Scan for Improv boxes for `seconds`, then hand back what was heard.
    /// Filtered on the service UUID, so only Virtues boxes (and other Improv
    /// devices — fine, the name disambiguates) ever appear.
    pub async fn discover(&self, seconds: f64) -> Result<Vec<FoundBox>> {
        let mut inner = self.inner.lock().await;
        let adapter = Self::adapter(&mut inner).await?;
        let service = uuid(protocol::SERVICE_UUID);

        adapter
            .start_scan(ScanFilter { services: vec![service] })
            .await
            .context("start bluetooth scan")?;
        tokio::time::sleep(Duration::from_secs_f64(seconds.clamp(1.0, 30.0))).await;
        // Best-effort: a failed stop is not a failed scan, and the results
        // below are already in hand.
        let _ = adapter.stop_scan().await;

        let sd_uuid = service_data_uuid();
        let mut out = Vec::new();
        inner.found.clear();
        for p in adapter.peripherals().await.context("read scan results")? {
            let Ok(Some(props)) = p.properties().await else { continue };
            // A scan filter is a HINT on some backends (and matches nothing at
            // all on others until the ad is re-parsed) — verify membership
            // rather than trusting it, or a laptop in a busy office lists
            // every beacon in the building as a Virtues box.
            let advertises = props.services.contains(&service) || props.service_data.contains_key(&sd_uuid);
            if !advertises {
                continue;
            }
            let id = p.id().to_string();
            let improv_state = props
                .service_data
                .get(&sd_uuid)
                .and_then(|d| d.first().copied())
                // No service data (a truncated ad, or a scan response we
                // didn't get): assume it still needs wifi. Sending a box that
                // is already online to the wifi picker is harmless; hiding a
                // box that needs setup is not.
                .unwrap_or(State::Authorized as u8);
            out.push(FoundBox {
                id: id.clone(),
                name: props.local_name.clone().unwrap_or_else(|| "Virtues box".into()),
                improv_state,
                rssi: props.rssi.unwrap_or(-127) as i32,
            });
            inner.found.insert(id, p);
        }
        // Closest first: proximity is how a human breaks the tie between two
        // unclaimed boxes, and it is the only signal we have.
        out.sort_by(|a, b| b.rssi.cmp(&a.rssi));
        Ok(out)
    }

    /// Connect (idempotently) and hand back the session's RPC characteristic.
    async fn ensure_connected(inner: &mut Inner, id: &str) -> Result<Session> {
        if let Some(s) = &inner.session {
            if s.id == id && s.peripheral.is_connected().await.unwrap_or(false) {
                return Ok(Session {
                    id: s.id.clone(),
                    peripheral: s.peripheral.clone(),
                    rpc: s.rpc.clone(),
                });
            }
        }
        if let Some(s) = inner.session.take() {
            let _ = s.peripheral.disconnect().await;
        }
        let peripheral = inner
            .found
            .get(id)
            .cloned()
            .ok_or_else(|| fail(FailureKind::NotFound, "Your server went out of Bluetooth range. Move closer to it and look again."))?;

        tokio::time::timeout(CONNECT_TIMEOUT, peripheral.connect())
            .await
            .map_err(|_| fail(FailureKind::NotFound, "This computer couldn't reach your server over Bluetooth. Move closer and try again."))?
            .map_err(|e| fail(FailureKind::NotFound, format!("This computer couldn't reach your server over Bluetooth ({e}).")))?;
        peripheral.discover_services().await.context("discover GATT services")?;

        let chars = peripheral.characteristics();
        let find = |u: &str| {
            let want = uuid(u);
            chars.iter().find(|c| c.uuid == want).cloned()
        };
        let rpc = find(protocol::CHAR_RPC_COMMAND)
            .ok_or_else(|| fail(FailureKind::NotFound, "That device isn't a Virtues server."))?;
        let result = find(protocol::CHAR_RPC_RESULT)
            .ok_or_else(|| fail(FailureKind::NotFound, "That device isn't a Virtues server."))?;

        // Subscribe once per session. State/error are best-effort: the box
        // always answers with a result packet too, and a device that refuses
        // these subscriptions can still be provisioned.
        peripheral.subscribe(&result).await.context("subscribe to RPC results")?;
        if let Some(c) = find(protocol::CHAR_CURRENT_STATE) {
            let _ = peripheral.subscribe(&c).await;
        }
        if let Some(c) = find(protocol::CHAR_ERROR_STATE) {
            let _ = peripheral.subscribe(&c).await;
        }

        let session = Session { id: id.to_string(), peripheral: peripheral.clone(), rpc: rpc.clone() };
        inner.session = Some(Session { id: id.to_string(), peripheral, rpc });
        Ok(session)
    }

    /// RPC 0x86: claim the setup session with the box's four-word phrase.
    ///
    /// Must succeed before wifi, the account grant, or pairing — a box refuses
    /// every configuring command without a session. The phrase is printed on the
    /// box's own panel while it is unclaimed, so having it proves line of sight,
    /// which radio range does not.
    ///
    /// `label` is this machine's name. It is not security — the box puts it on
    /// its panel in place of the phrase, so the owner sees on the box itself
    /// that their words landed here and not somewhere else.
    ///
    /// Returns whether the box actually HAS a gate. A box whose firmware
    /// predates `0x86` answers `UnknownCommand`, and that is not a failure to
    /// report — every released box (v0.3.0 and earlier) is in that state, and
    /// an app that treats it as one cannot set up a single shipped appliance.
    /// Proceeding there gives up nothing: the gate is enforced on the box, so a
    /// client cannot conjure protection a box does not implement.
    pub async fn claim_setup(&self, id: &str, phrase: &str, label: &str) -> Result<bool> {
        let mut inner = self.inner.lock().await;
        let session = Self::ensure_connected(&mut inner, id).await?;
        let mut notifications = session.peripheral.notifications().await.context("notifications")?;
        let error_uuid = uuid(protocol::CHAR_ERROR_STATE);

        // At most two attempts: with the label, then without it. A box built
        // before the label existed parses 0x86 strictly — `!rest.is_empty()` is
        // a malformed packet — so it rejects the extra string, and the client
        // reads that as "wrong words". Cost us a hardware session: the correct
        // phrase could not have worked either. The label is cosmetic, so
        // dropping it is always the right trade against not getting in at all.
        let attempts: &[&str] = if label.is_empty() { &[""] } else { &[label, ""] };
        let last = attempts.len() - 1;
        for (i, lbl) in attempts.iter().enumerate() {
            session
                .peripheral
                .write(
                    &session.rpc,
                    &protocol::build_rpc(&Command::ClaimSetup {
                        phrase: phrase.into(),
                        label: (*lbl).into(),
                    }),
                    WriteType::WithResponse,
                )
                .await
                .context("send setup phrase")?;

            // `Ok(None)` = retry without the label; anything else is final.
            let watch = async {
                while let Some(n) = notifications.next().await {
                    if n.uuid == error_uuid {
                        match n.value.first().copied() {
                            None | Some(0) => continue,
                            Some(c) if c == ImprovError::UnknownCommand as u8 => {
                                // Firmware older than the gate. Not an error.
                                return Ok(Some(false));
                            }
                            Some(c) if c == ImprovError::InvalidPacket as u8 && i < last => {
                                return Ok(None);
                            }
                            Some(_) => {
                                // The box will not say WHETHER the words were
                                // wrong or the attempt budget is spent, and
                                // neither will we — one message for both, so a
                                // guesser learns nothing.
                                return Err(fail(
                                    FailureKind::Refused,
                                    "Those words don't match. Check the words on your server's screen.",
                                ));
                            }
                        }
                    }
                    if protocol::parse_result(&n.value, 0x86).is_some() {
                        return Ok(Some(true));
                    }
                }
                Err(fail(FailureKind::Timeout, NO_ANSWER))
            };
            let outcome = tokio::time::timeout(REPLY_TIMEOUT, watch)
                .await
                .map_err(|_| fail(FailureKind::Timeout, NO_ANSWER))??;
            if let Some(gated) = outcome {
                return Ok(gated);
            }
        }
        Err(fail(FailureKind::Timeout, NO_ANSWER))
    }

    // pair_code (0x85) and link_code (0x84) were deleted 2026-08-24 with their
    // opcodes — the codeless 0x83 and the 0x82 grant made both round-trips
    // pointless. See protocol.rs.

    /// RPC 0x82: hand the box a pre-approved account grant.
    ///
    /// The box ACKs `"accepted"` as soon as the grant is stored — the redeem
    /// happens OUTBOUND later, when the box is online, and nothing here waits
    /// for it (a `"linked"` notify may follow on the same stream for whoever
    /// is still listening; the pair step's box-side complete-ticket wait is
    /// what actually sequences setup). Session-gated: only the phrase-proven
    /// peer may bind this box to an account.
    pub async fn claim_grant(&self, id: &str, grant: &str) -> Result<()> {
        let mut inner = self.inner.lock().await;
        let session = Self::ensure_connected(&mut inner, id).await?;
        let mut notifications =
            session.peripheral.notifications().await.context("notifications")?;
        session
            .peripheral
            .write(
                &session.rpc,
                &protocol::build_rpc(&Command::ClaimGrant { grant: grant.into() }),
                WriteType::WithResponse,
            )
            .await
            .context("send grant")?;

        let error_uuid = uuid(protocol::CHAR_ERROR_STATE);
        let watch = async {
            while let Some(n) = notifications.next().await {
                if n.uuid == error_uuid {
                    match n.value.first().copied() {
                        None | Some(0) => continue,
                        // NotAuthorized here almost always means ALREADY LINKED
                        // (re-running setup on a server that kept its account),
                        // which is not the person's failure. Same words as iOS.
                        Some(c) if c == ImprovError::NotAuthorized as u8 => {
                            return Err(fail(
                                FailureKind::Refused,
                                "This server is already linked to an account.",
                            ))
                        }
                        Some(c) => return Err(fail(FailureKind::Failed, ImprovError::describe(c))),
                    }
                }
                if let Some(strings) = protocol::parse_result(&n.value, 0x82) {
                    if strings.first().map(|s| s == "accepted").unwrap_or(false) {
                        return Ok(());
                    }
                    return Err(fail(FailureKind::Refused, "Your server didn't take the account hand-off."));
                }
            }
            Err(fail(FailureKind::Timeout, NO_ANSWER))
        };
        tokio::time::timeout(REPLY_TIMEOUT, watch)
            .await
            .map_err(|_| fail(FailureKind::Timeout, NO_ANSWER))?
    }

    /// RPC 0x04: ask the BOX what networks it can see. Streams one packet per
    /// network; an empty packet ends the list.
    pub async fn wifi_scan(&self, id: &str) -> Result<Vec<Network>> {
        let mut inner = self.inner.lock().await;
        let session = Self::ensure_connected(&mut inner, id).await?;
        let mut notifications = session.peripheral.notifications().await.context("notifications")?;

        session
            .peripheral
            .write(&session.rpc, &protocol::build_rpc(&Command::ScanWifi), WriteType::WithResponse)
            .await
            .context("send scan request")?;

        let mut networks = Vec::new();
        let mut finished = false;
        let collect = async {
            while let Some(n) = notifications.next().await {
                let Some(strings) = protocol::parse_result(&n.value, 0x04) else { continue };
                if strings.is_empty() {
                    finished = true;
                    break; // terminator
                }
                if strings.len() >= 3 {
                    networks.push(Network {
                        ssid: strings[0].clone(),
                        signal: strings[1].parse().unwrap_or(0),
                        // "ENT" is our 802.1X extension to Improv's YES/NO —
                        // those networks need a username the base protocol
                        // can't carry, so the UI routes them elsewhere.
                        secured: strings[2] == "YES" || strings[2] == "ENT",
                        enterprise: strings[2] == "ENT",
                    });
                }
            }
        };
        // A timeout with networks already collected is a truncated list, not a
        // failure — show what the server managed to report. And a finished
        // scan with NOTHING in it is an answer too ("no networks here"), not a
        // silent server: the screen says different things for the two.
        let _ = tokio::time::timeout(REPLY_TIMEOUT, collect).await;
        if networks.is_empty() && !finished {
            return Err(fail(FailureKind::Timeout, "Your server didn't answer the Wi-Fi scan. Try again."));
        }
        Ok(networks)
    }

    /// RPC 0x01 / 0x81: send credentials, then watch the join happen. Resolves
    /// with the box's own URL. This living progress is the entire reason the
    /// BLE path exists — contrast the SoftAP flow's "the socket died, go look".
    pub async fn provision(
        &self,
        id: &str,
        ssid: &str,
        password: &str,
        identity: Option<&str>,
        on_progress: impl Fn(&str) + Send,
    ) -> Result<String> {
        let mut inner = self.inner.lock().await;
        let session = Self::ensure_connected(&mut inner, id).await?;
        let mut notifications = session.peripheral.notifications().await.context("notifications")?;

        let cmd = match identity {
            Some(i) => Command::EnterpriseSettings {
                ssid: ssid.into(),
                identity: i.into(),
                password: password.into(),
            },
            None => Command::WifiSettings { ssid: ssid.into(), password: password.into() },
        };
        let echo = cmd.id();
        session
            .peripheral
            .write(&session.rpc, &protocol::build_rpc(&cmd), WriteType::WithResponse)
            .await
            .context("send wifi credentials")?;
        on_progress("sent");

        let state_uuid = uuid(protocol::CHAR_CURRENT_STATE);
        let error_uuid = uuid(protocol::CHAR_ERROR_STATE);
        let watch = async {
            while let Some(n) = notifications.next().await {
                if n.uuid == state_uuid {
                    match n.value.first().copied() {
                        Some(0x03) => on_progress("joining"),
                        // 0x04 alone is not success — the result packet with
                        // the URL follows it. But surface the milestone.
                        Some(0x04) => on_progress("joined"),
                        _ => {}
                    }
                    continue;
                }
                if n.uuid == error_uuid {
                    if let Some(code) = n.value.first().copied().filter(|c| *c != 0) {
                        let kind = if code == ImprovError::UnableToConnect as u8 {
                            FailureKind::JoinFailed
                        } else if code == ImprovError::NotAuthorized as u8 {
                            FailureKind::Refused
                        } else {
                            FailureKind::Failed
                        };
                        return Err(fail(kind, protocol::ImprovError::describe(code)));
                    }
                    continue;
                }
                if let Some(strings) = protocol::parse_result(&n.value, echo) {
                    return Ok(strings.into_iter().next().unwrap_or_default());
                }
            }
            Err(fail(FailureKind::Timeout, "Your server stopped answering while it joined. Check whether it's online."))
        };
        match tokio::time::timeout(JOIN_TIMEOUT, watch).await {
            Ok(r) => r,
            Err(_) => Err(fail(
                FailureKind::Timeout,
                "Your server didn't answer in time. It may still be joining.",
            )),
        }
    }

    /// RPC 0x83: pair THROUGH the box's Bluetooth. The box redeems `code`
    /// against its own consume endpoint and streams the response back in
    /// chunks; a reassembled body starting `error:` is a refusal.
    ///
    /// Returns the consume JSON verbatim — the caller persists it with the
    /// same code the LAN path uses.
    pub async fn pair(
        &self,
        id: &str,
        kind: &str,
        source: &str,
        label: &str,
        endpoint_id: &str,
    ) -> Result<String> {
        let mut inner = self.inner.lock().await;
        let session = Self::ensure_connected(&mut inner, id).await?;
        let mut notifications = session.peripheral.notifications().await.context("notifications")?;

        // Codeless (2026-08-24): the setup session is the proof of presence;
        // the box fetches and redeems its own standing code internally.
        let cmd = Command::PairConsume {
            kind: kind.into(),
            source: source.into(),
            label: label.into(),
            endpoint_id: endpoint_id.into(),
        };
        session
            .peripheral
            .write(&session.rpc, &protocol::build_rpc(&cmd), WriteType::WithResponse)
            .await
            .context("send pair request")?;

        let mut body = String::new();
        let collect = async {
            while let Some(n) = notifications.next().await {
                let Some(strings) = protocol::parse_result(&n.value, 0x83) else { continue };
                if strings.is_empty() {
                    return true; // terminator: the body is complete
                }
                body.push_str(&strings.concat());
            }
            false
        };
        let complete = tokio::time::timeout(PAIR_TIMEOUT, collect).await.unwrap_or(false);
        if !complete {
            return Err(fail(FailureKind::Timeout, "Pairing over Bluetooth timed out. Try again."));
        }
        if let Some(code) = body.strip_prefix("error:") {
            return Err(fail(FailureKind::Refused, describe_pair_error(code)));
        }
        Ok(body)
    }

    /// RPC 0x88: ask a CLAIMED box for a one-time owner challenge. Returns the
    /// raw nonce; the caller signs [`protocol::owner_proof_message`] of it with
    /// this device's paired key and hands the result to [`Self::owner_prove`].
    ///
    /// Split in two rather than taking a signer so the key never has to cross
    /// into this crate — it stays with the reach store that owns it.
    pub async fn owner_challenge(&self, id: &str) -> Result<Vec<u8>> {
        let mut inner = self.inner.lock().await;
        let session = Self::ensure_connected(&mut inner, id).await?;
        let mut notifications = session.peripheral.notifications().await.context("notifications")?;
        session
            .peripheral
            .write(&session.rpc, &protocol::build_rpc(&Command::OwnerChallenge), WriteType::WithResponse)
            .await
            .context("ask for a challenge")?;

        let error_uuid = uuid(protocol::CHAR_ERROR_STATE);
        let watch = async {
            while let Some(n) = notifications.next().await {
                if n.uuid == error_uuid {
                    match n.value.first().copied() {
                        None | Some(0) => continue,
                        // A box older than this command: it cannot be reopened
                        // over Bluetooth, and saying so is the useful answer.
                        Some(c) if c == ImprovError::UnknownCommand as u8 => {
                            return Err(fail(
                                FailureKind::Unsupported,
                                "Your server needs an update before you can reconnect it over Bluetooth.",
                            ));
                        }
                        Some(_) => {
                            return Err(fail(FailureKind::Refused, "This server isn't asking for its owner."))
                        }
                    }
                }
                if let Some(strings) = protocol::parse_result(&n.value, 0x88) {
                    let hex = strings.into_iter().next().unwrap_or_default();
                    return decode_hex(&hex)
                        .ok_or_else(|| fail(FailureKind::Failed, "Your server sent a challenge this app couldn't read."));
                }
            }
            Err(fail(FailureKind::Timeout, NO_ANSWER))
        };
        tokio::time::timeout(REPLY_TIMEOUT, watch)
            .await
            .map_err(|_| fail(FailureKind::Timeout, NO_ANSWER))?
    }

    /// RPC 0x89: present the signed challenge. `Ok(())` opens an owner session
    /// on this connection: wifi scan and join work from here on, nothing else.
    pub async fn owner_prove(&self, id: &str, endpoint_id: &str, signature_hex: &str) -> Result<()> {
        let mut inner = self.inner.lock().await;
        let session = Self::ensure_connected(&mut inner, id).await?;
        let mut notifications = session.peripheral.notifications().await.context("notifications")?;
        let cmd = Command::OwnerProve { endpoint_id: endpoint_id.into(), signature: signature_hex.into() };
        session
            .peripheral
            .write(&session.rpc, &protocol::build_rpc(&cmd), WriteType::WithResponse)
            .await
            .context("send ownership proof")?;

        let error_uuid = uuid(protocol::CHAR_ERROR_STATE);
        let watch = async {
            while let Some(n) = notifications.next().await {
                if n.uuid == error_uuid {
                    match n.value.first().copied() {
                        None | Some(0) => continue,
                        // One answer for every refusal — not paired, revoked,
                        // stale challenge — because the box gives only one.
                        Some(_) => {
                            return Err(fail(FailureKind::Refused, "This server doesn't recognize this computer."))
                        }
                    }
                }
                if protocol::parse_result(&n.value, 0x89).is_some() {
                    return Ok(());
                }
            }
            Err(fail(FailureKind::Timeout, NO_ANSWER))
        };
        tokio::time::timeout(REPLY_TIMEOUT, watch)
            .await
            .map_err(|_| fail(FailureKind::Timeout, NO_ANSWER))?
    }

    /// Drop the BLE connection. Safe to always call on leaving the setup flow.
    pub async fn disconnect(&self) {
        let mut inner = self.inner.lock().await;
        if let Some(s) = inner.session.take() {
            let _ = s.peripheral.disconnect().await;
        }
    }
}

/// Lowercase or uppercase hex to bytes; `None` on anything else. Local because
/// this crate's client half should not grow a dependency for eight lines.
fn decode_hex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

/// The box's consume-endpoint error codes, in words. Same strings the iOS
/// client shows — the failure a user sees must not depend on their platform.
fn describe_pair_error(code: &str) -> String {
    match code {
        // No code is typed on this path any more (0x83 is codeless), so none of
        // these may send the person to look for one. Same words as iOS.
        "invalid_or_expired_token" => {
            "Your server's setup code expired before pairing finished. Start setup again.".into()
        }
        "too_many_attempts" => {
            "Too many pairing attempts on your server. Wait a few minutes and try again.".into()
        }
        other => format!("Your server couldn't complete pairing ({other})."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_data_uuid_is_the_16_bit_uuid_in_base_form() {
        // btleplug keys service data by the full 128-bit UUID; getting this
        // expansion wrong makes every box look like it has no state byte, and
        // the picker silently loses the needs-wifi/online distinction.
        assert_eq!(
            service_data_uuid().to_string(),
            "00004677-0000-1000-8000-00805f9b34fb"
        );
    }

    #[test]
    fn a_classified_failure_keeps_its_code_through_anyhow() {
        // The whole point of FailureKind: a screen branches on the code, and
        // `refused` is what tells the owner path to try the next server.
        let e = fail(FailureKind::Refused, "no");
        assert_eq!(classify(&e), ("refused", "no".to_string()));
        let wrapped = e.context("while opening");
        assert_eq!(classify(&wrapped).0, "refused");
        assert_eq!(classify(&anyhow!("btleplug exploded")).0, "failed");
    }

    #[test]
    fn decode_hex_reads_a_nonce_and_refuses_junk() {
        assert_eq!(decode_hex("00ff10"), Some(vec![0, 255, 16]));
        assert_eq!(decode_hex("ABcd"), Some(vec![0xab, 0xcd]));
        assert_eq!(decode_hex("abc"), None);
        assert_eq!(decode_hex("zz"), None);
    }

    #[test]
    fn pair_errors_read_the_same_as_the_ios_clients() {
        assert!(describe_pair_error("invalid_or_expired_token").contains("expired"));
        assert!(describe_pair_error("too_many_attempts").contains("Too many pairing attempts"));
        assert!(describe_pair_error("weird").contains("weird"));
    }
}
