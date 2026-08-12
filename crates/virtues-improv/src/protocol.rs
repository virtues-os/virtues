//! Improv wire format — pure, dependency-free, and shared by both ends.
//!
//! Every RPC packet is `[command, data_len, data…, checksum]`, where the
//! checksum is the low byte of the sum of everything before it. Strings inside
//! data are length-prefixed with one byte, so no string exceeds 255 bytes and
//! neither does a packet's data section.
//!
//! The box PARSES commands ([`parse_rpc`]) and BUILDS results ([`build_result`]);
//! a client does the mirror ([`build_rpc`] / [`parse_result`]). Both directions
//! live here so the two can never drift — they did drift once, across the
//! Rust/Swift boundary, and the only reason it was caught was a hardware test.

// ─── constants ──────────────────────────────────────────────────────────────

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
    /// Unused by this box — see `ble_provision`'s module docs on authorization.
    AuthorizationRequired = 0x01,
    Authorized = 0x02,
    Provisioning = 0x03,
    Provisioned = 0x04,
}

impl State {
    /// The state byte as it appears in the advertisement, or `None` for a
    /// value outside the spec (a foreign device, or a truncated ad).
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x01 => Some(State::AuthorizationRequired),
            0x02 => Some(State::Authorized),
            0x03 => Some(State::Provisioning),
            0x04 => Some(State::Provisioned),
            _ => None,
        }
    }
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

impl ImprovError {
    /// The error byte in words, for a human. `0x03` is the one users actually
    /// hit, and "usually a wrong password" is the true and useful reading.
    pub fn describe(code: u8) -> String {
        match code {
            0x01 => "The box couldn't read that request — try again.".into(),
            0x02 => "This box doesn't support that step.".into(),
            0x03 => "The box couldn't join that network — usually a wrong password.".into(),
            0x04 => "The box refused the request.".into(),
            other => format!("Setup failed on the box (error {other})."),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `0x01` — the whole point: SSID + passphrase.
    WifiSettings { ssid: String, password: String },
    /// `0x02` — blink/identify. The box has no LED to blink; acknowledged no-op.
    Identify,
    /// `0x03` — firmware name/version/hardware/device name.
    DeviceInfo,
    /// `0x04` — the box's own wifi scan, streamed back one network per result.
    ScanWifi,
    /// `0x81` — OUR extension: 802.1X join. `[ssid, identity, password]`.
    ///
    /// Vendor command space, chosen because base Improv's wifi-settings RPC
    /// (0x01) carries exactly ssid+password and enterprise networks
    /// authenticate a USER. Without this, killing the SoftAP path would have
    /// orphaned enterprise wifi entirely — BLE is the only provisioning
    /// transport left. Foreign Improv clients never send 0x81; ours only
    /// sends it to boxes whose scan marked the network "ENT".
    EnterpriseSettings { ssid: String, identity: String, password: String },
    /// `0x82` — OUR extension: account claim grant. `[grant]`.
    ///
    /// The keystone that merges the wifi and link steps into one tap: a
    /// signed-in client asks atlas for a pre-approved `device_code` and hands
    /// it to the box over this same BLE session. The box stores it as its
    /// in-flight link and redeems it OUTBOUND the moment it can reach atlas —
    /// the box stays outbound-only, atlas never gains a path in, and the
    /// client's session was only ever the vouching authority, never a shared
    /// secret. The grant is single-use, short-lived, and worthless without
    /// this box's poll; carrying it over BLE also inherits the proximity
    /// argument (you were standing at the box). The display's QR/code flow
    /// remains the fallback for no-client and fresh-account paths.
    ClaimGrant { grant: String },
    /// `0x83` — OUR extension: pair over BLE. `[code, kind, source, label,
    /// endpoint_id]` (`source`/`label`/`endpoint_id` may be empty).
    ///
    /// Exists because pairing's LAN leg dies on hostile networks: client
    /// isolation at an office blocked `POST /api/pair/consume` between phone
    /// and box on the same wifi (WeWork, live, 2026-08-11) while BLE sat
    /// there working. The box redeems the code against its OWN consume
    /// endpoint over loopback — the same transaction, rate-limit story aside
    /// (see the handler), and device row as every other pairing — and streams
    /// the consume response back as chunked results. Security is unchanged:
    /// the code still proves the person can read the box's screen; BLE is
    /// just the wire. Cleartext like the LAN leg it replaces (and like the
    /// wifi passphrase in 0x01) — same accepted setup-window risk.
    ///
    /// Only the FIRST device can use this: a successful pair claims the box
    /// and the reconciler stops the whole BLE service. Later devices pair
    /// over LAN or relay, which exist by then.
    PairConsume { code: String, kind: String, source: String, label: String, endpoint_id: String },
    /// `0x86` — OUR extension: claim the setup session. `[phrase, label]`.
    ///
    /// The gate in front of everything else. A box advertises Improv while
    /// unclaimed, so without this any client in radio range could configure it —
    /// and radio range passes through walls. The four-word phrase is printed on
    /// the box's own panel while it is empty, so possessing it proves *line of
    /// sight*, which is the bar we actually want.
    ///
    /// A successful claim opens a session bound to THIS connection; only that
    /// session may then join wifi, take an account grant, or pair. Drop the
    /// link and the session dies with it. See
    /// `docs/onboarding-paradigm.md` §1 and §5.
    ///
    /// `label` is the claiming device's own name ("Adam's Mac"), and it is not
    /// security — it is what the box's panel says while setup is running. The
    /// phrase leaves the glass the moment it is accepted, and this replaces it,
    /// so the owner gets confirmation *on the box* that what they typed landed,
    /// and a race they did not start reads as a name they do not recognise.
    /// May be empty; the panel then says only that a device is setting up.
    ClaimSetup { phrase: String, label: String },
}

impl Command {
    /// The command byte this variant travels as.
    pub fn id(&self) -> u8 {
        match self {
            Command::WifiSettings { .. } => 0x01,
            Command::Identify => 0x02,
            Command::DeviceInfo => 0x03,
            Command::ScanWifi => 0x04,
            Command::EnterpriseSettings { .. } => 0x81,
            Command::ClaimGrant { .. } => 0x82,
            Command::PairConsume { .. } => 0x83,
            Command::ClaimSetup { .. } => 0x86,
        }
    }
}

// ─── framing ────────────────────────────────────────────────────────────────

/// Low byte of the byte-sum — the spec's whole integrity story.
fn checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |a, b| a.wrapping_add(*b))
}

/// Parse one RPC command packet. The BOX side of the wire.
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
        0x81 => {
            let (ssid, rest) = take_string(data).ok_or(ImprovError::InvalidPacket)?;
            let (identity, rest) = take_string(rest).ok_or(ImprovError::InvalidPacket)?;
            let (password, rest) = take_string(rest).ok_or(ImprovError::InvalidPacket)?;
            if !rest.is_empty() {
                return Err(ImprovError::InvalidPacket);
            }
            Ok(Command::EnterpriseSettings { ssid, identity, password })
        }
        0x82 => {
            let (grant, rest) = take_string(data).ok_or(ImprovError::InvalidPacket)?;
            if grant.is_empty() || !rest.is_empty() {
                return Err(ImprovError::InvalidPacket);
            }
            Ok(Command::ClaimGrant { grant })
        }
        0x83 => {
            let (code, rest) = take_string(data).ok_or(ImprovError::InvalidPacket)?;
            let (kind, rest) = take_string(rest).ok_or(ImprovError::InvalidPacket)?;
            let (source, rest) = take_string(rest).ok_or(ImprovError::InvalidPacket)?;
            let (label, rest) = take_string(rest).ok_or(ImprovError::InvalidPacket)?;
            let (endpoint_id, rest) = take_string(rest).ok_or(ImprovError::InvalidPacket)?;
            if code.is_empty() || kind.is_empty() || !rest.is_empty() {
                return Err(ImprovError::InvalidPacket);
            }
            Ok(Command::PairConsume { code, kind, source, label, endpoint_id })
        }
        0x86 => {
            let (phrase, rest) = take_string(data).ok_or(ImprovError::InvalidPacket)?;
            // The label is optional on the wire: it is cosmetic, and a client
            // that omits it should still be able to claim rather than be told
            // its packet is malformed.
            let (label, rest) = take_string(rest).unwrap_or((String::new(), rest));
            if phrase.is_empty() || !rest.is_empty() {
                return Err(ImprovError::InvalidPacket);
            }
            Ok(Command::ClaimSetup { phrase, label })
        }
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

/// Length-prefix `strings` into a data section. Shared by both packet builders
/// — the prefix is one byte, so a string longer than 255 is truncated rather
/// than allowed to corrupt the frame's length arithmetic.
fn pack_strings(strings: &[&str]) -> Vec<u8> {
    let mut data = Vec::new();
    for s in strings {
        let bytes = s.as_bytes();
        let take = bytes.len().min(255);
        data.push(take as u8);
        data.extend_from_slice(&bytes[..take]);
    }
    data
}

fn frame(command_id: u8, data: Vec<u8>) -> Vec<u8> {
    let mut packet = Vec::with_capacity(data.len() + 3);
    packet.push(command_id);
    packet.push(data.len() as u8);
    packet.extend_from_slice(&data);
    packet.push(checksum(&packet));
    packet
}

/// Build an RPC result packet: the echoed command id + length-prefixed strings.
/// The BOX side.
pub fn build_result(command_id: u8, strings: &[&str]) -> Vec<u8> {
    frame(command_id, pack_strings(strings))
}

/// Build an RPC command packet. The CLIENT side — the mirror of [`parse_rpc`],
/// and deliberately expressed in terms of [`Command`] so a client cannot
/// invent an argument order the box does not parse.
pub fn build_rpc(cmd: &Command) -> Vec<u8> {
    let data = match cmd {
        Command::WifiSettings { ssid, password } => pack_strings(&[ssid, password]),
        Command::Identify | Command::DeviceInfo | Command::ScanWifi => Vec::new(),
        Command::EnterpriseSettings { ssid, identity, password } => {
            pack_strings(&[ssid, identity, password])
        }
        Command::ClaimGrant { grant } => pack_strings(&[grant]),
        Command::PairConsume { code, kind, source, label, endpoint_id } => {
            pack_strings(&[code, kind, source, label, endpoint_id])
        }
        // An EMPTY label sends one string, not two. A box built before the
        // label existed parses 0x86 strictly — trailing bytes are a malformed
        // packet — so the one-string form is the shape every version accepts,
        // and it is what the client falls back to.
        Command::ClaimSetup { phrase, label } if label.is_empty() => pack_strings(&[phrase]),
        Command::ClaimSetup { phrase, label } => pack_strings(&[phrase, label]),
    };
    frame(cmd.id(), data)
}

/// Parse a result packet for `command`. `None` when malformed, checksum-bad, or
/// carrying another command's result — a client filters a shared notification
/// stream with this, so "not mine" must be as unremarkable as "not valid".
/// `Some(vec![])` is the empty terminator packet that ends a streamed reply.
pub fn parse_result(packet: &[u8], command: u8) -> Option<Vec<String>> {
    if packet.len() < 3 || packet[0] != command {
        return None;
    }
    let (body, ck) = packet.split_at(packet.len() - 1);
    if checksum(body) != ck[0] {
        return None;
    }
    let data_len = body[1] as usize;
    if body.len() != 2 + data_len {
        return None;
    }
    let mut rest = &body[2..];
    let mut strings = Vec::new();
    while !rest.is_empty() {
        let (s, tail) = take_string(rest)?;
        strings.push(s);
        rest = tail;
    }
    Some(strings)
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

/// Chunk a long reply for the Improv frame's 1-byte length budget: the WHOLE
/// result packet's data is ≤255 bytes, so a JSON body streams as one ~200-byte
/// string per packet, terminated by an empty result — the same shape
/// [`Command::ScanWifi`] already streams. The client concatenates chunks; a
/// reassembled body starting `error:` is a failure code, not JSON.
pub fn chunk_for_results(body: &str) -> Vec<String> {
    // 200 leaves headroom for the frame (command, length, string prefix,
    // checksum) under the 255 cap. Cuts back off multi-byte seams: a device
    // label like "Adam's iPhone" with a curly quote reaches this JSON, and a
    // chunk boundary through it would corrupt the reassembled body.
    let mut out = Vec::new();
    let mut rest = body;
    while !rest.is_empty() {
        let mut cut = rest.len().min(200);
        while !rest.is_char_boundary(cut) {
            cut -= 1;
        }
        out.push(rest[..cut].to_string());
        rest = &rest[cut..];
    }
    out
}

// ─── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(command: u8, data: &[u8]) -> Vec<u8> {
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
        let cmd = parse_rpc(&raw(0x01, &data)).unwrap();
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
        let cmd = parse_rpc(&raw(0x01, &data)).unwrap();
        assert_eq!(cmd, Command::WifiSettings { ssid: "cafe".into(), password: "".into() });
    }

    #[test]
    fn a_bad_checksum_is_refused() {
        let mut p = raw(0x04, &[]);
        let last = p.len() - 1;
        p[last] ^= 0xFF;
        assert_eq!(parse_rpc(&p), Err(ImprovError::InvalidPacket));
    }

    #[test]
    fn a_string_running_past_the_packet_is_refused() {
        // A string length pointing past the data.
        let mut data = vec![200u8];
        data.extend_from_slice(b"x");
        assert_eq!(parse_rpc(&raw(0x01, &data)), Err(ImprovError::InvalidPacket));
    }

    #[test]
    fn enterprise_settings_round_trip() {
        // The 0x81 vendor extension: ssid + identity + password. Without it,
        // retiring SoftAP orphans every credential-per-user network.
        let mut data = vec![4u8];
        data.extend_from_slice(b"work");
        data.push(4);
        data.extend_from_slice(b"adam");
        data.push(6);
        data.extend_from_slice(b"hunter");
        let cmd = parse_rpc(&raw(0x81, &data)).unwrap();
        assert_eq!(
            cmd,
            Command::EnterpriseSettings {
                ssid: "work".into(),
                identity: "adam".into(),
                password: "hunter".into()
            }
        );
    }

    #[test]
    fn claim_grant_round_trip() {
        // The 0x82 vendor extension: the client-supplied claim grant that
        // merges the wifi and link steps into one tap.
        let mut data = vec![12u8];
        data.extend_from_slice(b"dc_a1b2c3d4e5");
        // 12 bytes declared, 13 provided — framing must catch it before it
        // ever becomes a device_code.
        assert_eq!(parse_rpc(&raw(0x82, &data)), Err(ImprovError::InvalidPacket));
        data[0] = 13;
        let cmd = parse_rpc(&raw(0x82, &data)).unwrap();
        assert_eq!(cmd, Command::ClaimGrant { grant: "dc_a1b2c3d4e5".into() });
    }

    #[test]
    fn claim_grant_refuses_an_empty_grant() {
        // An empty grant would store an empty in-flight device_code and poll
        // atlas with it forever.
        let data = vec![0u8];
        assert_eq!(parse_rpc(&raw(0x82, &data)), Err(ImprovError::InvalidPacket));
    }

    #[test]
    fn pair_consume_round_trip() {
        let mut data = vec![6u8];
        data.extend_from_slice(b"123456");
        data.push(10);
        data.extend_from_slice(b"mobile_app");
        data.push(3);
        data.extend_from_slice(b"ios");
        data.push(0); // label: auto-generate
        data.push(4);
        data.extend_from_slice(b"beef");
        let cmd = parse_rpc(&raw(0x83, &data)).unwrap();
        assert_eq!(
            cmd,
            Command::PairConsume {
                code: "123456".into(),
                kind: "mobile_app".into(),
                source: "ios".into(),
                label: "".into(),
                endpoint_id: "beef".into(),
            }
        );
    }

    #[test]
    fn pair_consume_refuses_empty_code_or_kind() {
        let mut data = vec![0u8, 10];
        data.extend_from_slice(b"mobile_app");
        data.extend_from_slice(&[0, 0, 0]);
        assert_eq!(parse_rpc(&raw(0x83, &data)), Err(ImprovError::InvalidPacket));
    }

    #[test]
    fn claim_setup_refuses_an_empty_phrase() {
        // An empty phrase must never reach the verifier — it would spend an
        // attempt from the box's global budget for nothing.
        assert_eq!(parse_rpc(&raw(0x86, &[0u8])), Err(ImprovError::InvalidPacket));
    }

    #[test]
    fn an_empty_label_builds_the_one_string_form() {
        // THE regression this cost us a hardware session for. A box built
        // before the label existed rejects trailing bytes as a malformed
        // packet, so an empty label must send ONE string — that is the shape
        // the client retries with, and if this ever sends two again, every
        // older box becomes unsetuppable while reporting "wrong phrase".
        let wire = build_rpc(&Command::ClaimSetup { phrase: "word".into(), label: String::new() });
        assert_eq!(wire[1] as usize, 5, "data section should be just [len, w,o,r,d]");
        assert_eq!(
            parse_rpc(&wire),
            Ok(Command::ClaimSetup { phrase: "word".into(), label: String::new() })
        );
    }

    #[test]
    fn claim_setup_accepts_a_phrase_with_no_label() {
        // The label only feeds the panel's "setting up with…" line. A client
        // that sends none must still get in.
        let mut data = vec![4u8];
        data.extend_from_slice(b"word");
        assert_eq!(
            parse_rpc(&raw(0x86, &data)),
            Ok(Command::ClaimSetup { phrase: "word".into(), label: String::new() })
        );
    }

    #[test]
    fn unknown_commands_are_distinguished_from_garbage() {
        // A well-formed packet with a command we don't know must say so —
        // clients treat InvalidPacket as "retry" and UnknownCommand as "don't".
        assert_eq!(parse_rpc(&raw(0x7F, &[])), Err(ImprovError::UnknownCommand));
    }

    #[test]
    fn results_carry_their_own_checksum() {
        let p = build_result(0x01, &["http://192.168.1.187:8000"]);
        let strings = parse_result(&p, 0x01).unwrap();
        assert_eq!(strings, vec!["http://192.168.1.187:8000".to_string()]);
    }

    #[test]
    fn every_command_the_client_builds_is_one_the_box_parses() {
        // THE test this crate exists for. Both directions of every command,
        // through both implementations — the drift that a single shared
        // module makes impossible, pinned so a future split re-fails here
        // instead of on someone's desk with a box that won't join.
        for cmd in [
            Command::WifiSettings { ssid: "home".into(), password: "hunter22".into() },
            Command::Identify,
            Command::DeviceInfo,
            Command::ScanWifi,
            Command::EnterpriseSettings {
                ssid: "work".into(),
                identity: "adam".into(),
                password: "pw".into(),
            },
            Command::ClaimGrant { grant: "dc_x".into() },
            Command::PairConsume {
                code: "123456".into(),
                kind: "desktop_app".into(),
                source: "mac".into(),
                label: "Adam's Mac".into(),
                endpoint_id: "beef".into(),
            },
            Command::ClaimSetup {
                phrase: "mango-burly-skull-dough".into(),
                label: "Adam's Mac".into(),
            },
        ] {
            let wire = build_rpc(&cmd);
            assert_eq!(parse_rpc(&wire), Ok(cmd.clone()), "round trip failed for {cmd:?}");
        }
    }

    #[test]
    fn parse_result_ignores_another_commands_packet() {
        // A client filters one shared notification stream; "not mine" has to
        // be ordinary, not an error.
        let p = build_result(0x04, &["home", "-40", "YES"]);
        assert!(parse_result(&p, 0x83).is_none());
        assert!(parse_result(&p, 0x04).is_some());
    }

    #[test]
    fn parse_result_reads_the_empty_terminator() {
        // The packet that ends a stream — distinct from "malformed".
        let p = build_result(0x04, &[]);
        assert_eq!(parse_result(&p, 0x04), Some(Vec::new()));
    }

    #[test]
    fn advertisement_service_data_is_six_bytes_state_first() {
        // The picker renders off byte 0 without connecting; the spec fixes the
        // layout. Six bytes, state, capabilities, four reserved zeros.
        let d = service_data(State::Provisioned);
        assert_eq!(d.len(), 6);
        assert_eq!(d[0], 0x04);
        assert_eq!(&d[1..], &[0, 0, 0, 0, 0]);
        assert_eq!(State::from_byte(d[0]), Some(State::Provisioned));
    }

    #[test]
    fn result_chunks_fit_the_frame_and_reassemble() {
        // A realistic consume response overflows one Improv frame (1-byte
        // length ⇒ ≤255 data bytes); it must stream and rejoin losslessly —
        // including a multi-byte char sitting on the cut.
        let body = format!(
            "{{\"device_id\":\"dev_x\",\"label\":\"Adam’s Mac\",\"box_node_id\":\"{}\",\"pad\":\"{}\"}}",
            "ab".repeat(32),
            "x".repeat(300),
        );
        let chunks = chunk_for_results(&body);
        assert!(chunks.len() > 1);
        for c in &chunks {
            assert!(c.len() <= 200, "chunk too big: {}", c.len());
            let p = build_result(0x83, &[c]);
            assert!(p.len() <= 255 + 3);
            assert!(parse_result(&p, 0x83).is_some());
        }
        assert_eq!(chunks.concat(), body);
    }
}
