//! Kernel WireGuard interface management (WS-2) — **Linux only**.
//!
//! Drives the in-kernel WireGuard device via `defguard_wireguard_rs` (netlink).
//! This is the box's side of the hub-and-spoke tunnel: one `wg0` interface, one
//! peer per paired device. Kernel WireGuard only — the box (Jetson appliance or
//! DIY mini-PC) must have the `wireguard` kernel module; there is no userspace
//! fallback. On the Jetson the module ships in the appliance image.
//!
//! Confirmed against real kernel WG on the Orange Pi 5 Plus and in the OrbStack
//! arm64 dev container. The live test below needs `NET_ADMIN` (run it in the
//! privileged container): `cargo test -p virtues -- --ignored wireguard`.
//!
//! The interface is ephemeral (rebuilt from the persisted peer set on boot — see
//! Phase 2); this module is pure engine and holds no DB state.

use anyhow::{Context, Result};
use std::net::IpAddr;
use std::time::SystemTime;

use defguard_wireguard_rs::{
    key::Key, net::IpAddrMask, peer::Peer, InterfaceConfiguration, Kernel, WGApi,
    WireguardInterfaceApi,
};

/// The single tunnel interface name.
pub const WG_IFNAME: &str = "wg0";

/// Default WG UDP listen port (WireGuard's standard). Overridable so a box that
/// already runs another WireGuard on 51820 (a user's own Tailscale/overlay)
/// can coexist — see [`wg_listen_port`].
pub const DEFAULT_WG_LISTEN_PORT: u16 = 51820;

/// The WG UDP listen port — also the inbound pinhole port and the port baked
/// into pairing bundles. Reads `VIRTUES_WG_LISTEN_PORT` (mirroring
/// `VIRTUES_WG_PUBLIC_IP`), else the WireGuard default 51820. A bad/zero value
/// falls back to the default.
pub fn wg_listen_port() -> u16 {
    std::env::var("VIRTUES_WG_LISTEN_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .filter(|&p| p != 0)
        .unwrap_or(DEFAULT_WG_LISTEN_PORT)
}

/// A paired device, as a WG peer to install on the interface.
#[derive(Debug, Clone)]
pub struct PeerConfig {
    /// Device WG public key, base64 (the phone generates the keypair on-device
    /// and sends only the public half up at pairing).
    pub public_key: String,
    /// Per-pair pre-shared key, base64 (defense-in-depth).
    pub preshared_key: String,
    /// The device's assigned address in the box's ULA space (installed as a /128
    /// AllowedIP — only this device may use it).
    pub allowed_ip: IpAddr,
}

/// A freshly generated WG keypair, base64-encoded (WireGuard's wire repr).
#[derive(Debug, Clone)]
pub struct KeyPair {
    pub private_key: String,
    pub public_key: String,
}

/// Read-back peer state for the on-box device console (who's connected).
#[derive(Debug, Clone)]
pub struct PeerStatus {
    pub public_key: String,
    pub last_handshake: Option<SystemTime>,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

/// Generate the box's own WG keypair (minted once at first boot, private half
/// sealed at rest). x25519 via the crate.
pub fn generate_keypair() -> KeyPair {
    let private = Key::generate();
    let public = private.public_key();
    KeyPair {
        private_key: private.to_string(),
        public_key: public.to_string(),
    }
}

/// Generate a per-pair pre-shared key: 32 uniformly-random bytes, base64.
pub fn generate_psk() -> String {
    use ring::rand::{SecureRandom, SystemRandom};
    let mut b = [0u8; 32];
    SystemRandom::new()
        .fill(&mut b)
        .expect("SystemRandom should always produce bytes");
    Key::new(b).to_string()
}

fn api() -> Result<WGApi<Kernel>> {
    WGApi::<Kernel>::new(WG_IFNAME.to_string()).context("open wg api")
}

fn to_peer(p: &PeerConfig) -> Result<Peer> {
    let mut peer = Peer::new(Key::try_from(p.public_key.as_str()).context("parse peer pubkey")?);
    peer.preshared_key = Some(Key::try_from(p.preshared_key.as_str()).context("parse peer psk")?);
    let mask: IpAddrMask = format!("{}/128", p.allowed_ip)
        .parse()
        .context("parse peer allowed-ip")?;
    peer.allowed_ips.push(mask);
    Ok(peer)
}

/// Create `wg0` (idempotent) and apply the server key, listen port, address,
/// and the full peer set. Called at boot from the persisted peer list, and on
/// any full reconfiguration. The interface is the ephemeral runtime projection
/// of the durable peer store.
pub fn bring_up(server_privkey: &str, server_addr: IpAddr, peers: &[PeerConfig]) -> Result<()> {
    let mut wgapi = api()?;
    // The interface is ephemeral, so create it fresh; tolerate "already exists"
    // on a re-run (the configure step below is the source of truth either way).
    if let Err(e) = wgapi.create_interface() {
        tracing::debug!("wg create_interface (may already exist): {e}");
    }
    let server_mask: IpAddrMask = format!("{server_addr}/128")
        .parse()
        .context("parse server addr")?;
    let cfg = InterfaceConfiguration {
        name: WG_IFNAME.to_string(),
        prvkey: server_privkey.to_string(),
        addresses: vec![server_mask],
        port: wg_listen_port(),
        peers: peers.iter().map(to_peer).collect::<Result<Vec<_>>>()?,
        mtu: None,
        fwmark: None,
    };
    wgapi
        .configure_interface(&cfg)
        .context("configure wg interface")?;
    Ok(())
}

/// Add or update a single peer (a new pairing) without touching the rest.
pub fn add_peer(peer: &PeerConfig) -> Result<()> {
    let wgapi = api()?;
    wgapi
        .configure_peer(&to_peer(peer)?)
        .context("configure wg peer")?;
    Ok(())
}

/// Remove a peer by its base64 public key (revoke / re-pair).
pub fn remove_peer(public_key: &str) -> Result<()> {
    let wgapi = api()?;
    let key = Key::try_from(public_key).context("parse peer pubkey")?;
    wgapi.remove_peer(&key).context("remove wg peer")?;
    Ok(())
}

/// Tear the interface down (shutdown / reset).
pub fn tear_down() -> Result<()> {
    let wgapi = api()?;
    wgapi.remove_interface().context("remove wg interface")?;
    Ok(())
}

/// Read current peer state — feeds the on-box "Devices" view (who's connected,
/// last handshake age, transfer).
pub fn read_peers() -> Result<Vec<PeerStatus>> {
    let wgapi = api()?;
    let host = wgapi.read_interface_data().context("read wg interface")?;
    Ok(host
        .peers
        .into_values()
        .map(|p| PeerStatus {
            public_key: p.public_key.to_string(),
            last_handshake: p.last_handshake,
            rx_bytes: p.rx_bytes,
            tx_bytes: p.tx_bytes,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keypair_and_psk_are_distinct_base64() {
        let kp = generate_keypair();
        assert_ne!(kp.private_key, kp.public_key);
        // 32 bytes → 44 base64 chars (with padding).
        assert_eq!(kp.public_key.len(), 44);
        assert_eq!(kp.private_key.len(), 44);
        // The public key must parse back as a valid Key.
        assert!(Key::try_from(kp.public_key.as_str()).is_ok());
        let psk = generate_psk();
        assert_eq!(psk.len(), 44);
        assert_ne!(psk, generate_psk());
    }

    /// Live kernel-WG round trip. Requires `NET_ADMIN` — run in the privileged
    /// OrbStack dev container: `cargo test -p virtues -- --ignored wireguard`.
    #[test]
    #[ignore]
    fn live_bring_up_add_remove() {
        let server = generate_keypair();
        let device = generate_keypair();
        let peer = PeerConfig {
            public_key: device.public_key.clone(),
            preshared_key: generate_psk(),
            allowed_ip: "fd00:5654::2".parse().unwrap(),
        };
        let server_addr: IpAddr = "fd00:5654::1".parse().unwrap();

        bring_up(&server.private_key, server_addr, std::slice::from_ref(&peer)).unwrap();

        let peers = read_peers().unwrap();
        assert!(peers.iter().any(|p| p.public_key == device.public_key));

        remove_peer(&device.public_key).unwrap();
        let peers = read_peers().unwrap();
        assert!(!peers.iter().any(|p| p.public_key == device.public_key));

        // Best-effort: defguard's remove_interface also clears DNS (resolvconf),
        // which ENOENTs in a minimal container. The engine assertions above are
        // the point; teardown cleanliness isn't.
        let _ = tear_down();
    }
}
