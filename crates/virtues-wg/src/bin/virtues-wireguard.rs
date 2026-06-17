//! `virtues-wireguard` — the minimal, privileged WireGuard daemon.
//!
//! Runs rootful (`NET_ADMIN` + `/dev/net/tun` + host networking) as its own
//! Quadlet/systemd unit so the main app stays rootless. It does three things and
//! nothing else — no web, no HTTP client, no bearer, no internet egress:
//!
//!   1. **Reconcile** `wg0` to the durable peer set (`reconcile::rebuild_interface`).
//!   2. **Detect** the box's current public endpoint and record it in the DB
//!      (`endpoint::write_current`) for the app to publish to the rendezvous.
//!   3. Repeat on a tick (later: netlink-event-driven + `LISTEN/NOTIFY`).
//!
//! Env: `DATABASE_URL` (the box's local Postgres) + `VIRTUES_ENCRYPTION_KEY`
//! (to unseal the WG server key from `box_secrets`).

/// Backstop poll interval. With LISTEN/NOTIFY wired, pair/revoke reconcile in
/// ~1s; this tick still catches prefix rotation, a missed notification, or a
/// dropped LISTEN connection.
#[cfg(target_os = "linux")]
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(15);

#[cfg(target_os = "linux")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use virtues_wg::{manager, signal};

    let db_url = std::env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("DATABASE_URL not set"))?;
    let db = sqlx::PgPool::connect(&db_url).await?;
    eprintln!("[virtues-wireguard] started; reconciling wg0 from the peer store");

    // Open the inbound pinhole once at startup so a default-deny host is
    // reachable on the WG port. Best-effort + idempotent — see the fn docs.
    ensure_inbound_pinhole(manager::wg_listen_port());

    // Event-driven reconcile: the box's API fires `NOTIFY wg_reconcile` on
    // pair-consume / revoke (see virtues_wg::signal), so a new peer lands in
    // wg0 in ~1s instead of waiting up to a full poll interval — that window
    // was racing the desktop client's handshake budget on first pair. If the
    // listener can't be established we degrade to pure polling.
    let mut listener = match sqlx::postgres::PgListener::connect(&db_url).await {
        Ok(mut l) => match l.listen(signal::RECONCILE_CHANNEL).await {
            Ok(()) => Some(l),
            Err(e) => {
                eprintln!("[virtues-wireguard] LISTEN failed ({e:#}); polling only");
                None
            }
        },
        Err(e) => {
            eprintln!("[virtues-wireguard] listener connect failed ({e:#}); polling only");
            None
        }
    };

    loop {
        reconcile_once(&db).await;

        // Wait for a reconcile notification OR the backstop tick, whichever
        // comes first. A recv error means the LISTEN connection dropped — fall
        // back to pure polling so we never wedge. The match returns a bool so
        // the `&mut listener` borrow ends before we (maybe) reassign `listener`.
        let listen_failed = match listener.as_mut() {
            Some(l) => {
                tokio::select! {
                    res = l.recv() => res.is_err(),
                    _ = tokio::time::sleep(POLL_INTERVAL) => false,
                }
            }
            None => {
                tokio::time::sleep(POLL_INTERVAL).await;
                false
            }
        };
        if listen_failed {
            eprintln!("[virtues-wireguard] LISTEN recv error; falling back to polling only");
            listener = None;
        }
    }
}

/// One reconcile pass: make `wg0` match the durable peer set, then detect and
/// record the box's current public endpoint. Both steps are idempotent.
#[cfg(target_os = "linux")]
async fn reconcile_once(db: &sqlx::PgPool) {
    use virtues_wg::{endpoint, manager, reconcile};

    if let Err(e) = reconcile::rebuild_interface(db).await {
        eprintln!("[virtues-wireguard] reconcile failed: {e:#}");
    }

    match detect_public_ip() {
        Some(ip) => match reconcile::ensure_server_keypair(db).await {
            Ok(kp) => {
                let ep = endpoint::Endpoint {
                    ip,
                    port: manager::wg_listen_port(),
                    wg_pub: kp.public_key,
                };
                if let Err(e) = endpoint::write_current(db, &ep).await {
                    eprintln!("[virtues-wireguard] endpoint record failed: {e:#}");
                }
            }
            Err(e) => eprintln!("[virtues-wireguard] server key load failed: {e:#}"),
        },
        None => { /* no public IP yet; try again next tick */ }
    }
}

/// Detect the box's current GLOBALLY-ROUTABLE IP — the address a remote peer
/// on the internet would dial. Per the IPv6-direct doctrine, the box is a real
/// computer on the real internet; we publish only an address that's actually
/// reachable, never a LAN/NAT one.
///
/// Resolution order:
///   1. `VIRTUES_WG_PUBLIC_IP` env override — explicit, takes priority. Used
///      when the box sits behind a router with port-forwarding and the
///      operator knows the WAN address out-of-band. Trusted as-is.
///   2. The "outbound socket trick": open a UDP socket, `connect()` it to a
///      public address (no packets sent, just a kernel route lookup), read back
///      the local source address the kernel picked for the default route.
///      Prefer IPv6, fall back to IPv4 — but accept ONLY a globally-routable
///      result ([`is_globally_routable`]). A ULA / link-local / RFC1918 / CGNAT
///      source means the box isn't directly reachable on that family, so we
///      return `None` and retry rather than baking a dead endpoint into bundles.
///
/// `None` therefore means "not directly reachable yet" — the publish/pairing
/// path treats that honestly (no wildcard placeholder) and the install-time
/// network check surfaces it to the user.
#[cfg(target_os = "linux")]
fn detect_public_ip() -> Option<String> {
    if let Ok(s) = std::env::var("VIRTUES_WG_PUBLIC_IP") {
        if !s.is_empty() {
            return Some(s);
        }
    }

    if let Some(ip) = probe_global_addr("[2606:4700:4700::1111]:53", "[::]:0") {
        return Some(ip);
    }
    probe_global_addr("1.1.1.1:53", "0.0.0.0:0")
}

/// Run the outbound-socket trick and return the source address ONLY if it is
/// globally routable; otherwise `None`.
#[cfg(target_os = "linux")]
fn probe_global_addr(dest: &str, bind: &str) -> Option<String> {
    let sock = std::net::UdpSocket::bind(bind).ok()?;
    sock.connect(dest).ok()?;
    let ip = sock.local_addr().ok()?.ip();
    if is_globally_routable(ip) {
        Some(ip.to_string())
    } else {
        None
    }
}

/// True only for addresses reachable from the public internet. Rejects every
/// non-global range so the box never advertises an address a remote peer can't
/// dial: loopback, unspecified, multicast, link-local, IPv6 unique-local (ULA),
/// IPv4 private (RFC1918), CGNAT (RFC6598), broadcast.
///
/// `Ipv6Addr::is_unique_local`/`is_unicast_link_local` are unstable on stable
/// Rust, so the v6 ranges are open-coded against the leading bits.
#[cfg(target_os = "linux")]
fn is_globally_routable(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(v) => {
            !v.is_loopback()
                && !v.is_unspecified()
                && !v.is_private()
                && !v.is_link_local()
                && !v.is_broadcast()
                && !v.is_multicast()
                && !is_cgnat_v4(v)
        }
        IpAddr::V6(v) => {
            !v.is_loopback()
                && !v.is_unspecified()
                && !v.is_multicast()
                && !is_link_local_v6(v)
                && !is_unique_local_v6(v)
        }
    }
}

/// 100.64.0.0/10 (RFC 6598) — carrier-grade NAT space, never internet-routable.
#[cfg(target_os = "linux")]
fn is_cgnat_v4(v: std::net::Ipv4Addr) -> bool {
    let o = v.octets();
    o[0] == 100 && (o[1] & 0xc0) == 0x40
}

/// fe80::/10 — IPv6 link-local.
#[cfg(target_os = "linux")]
fn is_link_local_v6(v: std::net::Ipv6Addr) -> bool {
    (v.segments()[0] & 0xffc0) == 0xfe80
}

/// fc00::/7 — IPv6 unique-local (ULA).
#[cfg(target_os = "linux")]
fn is_unique_local_v6(v: std::net::Ipv6Addr) -> bool {
    (v.segments()[0] & 0xfe00) == 0xfc00
}

/// Best-effort: ensure the WG listen port is accepted inbound, so a box on a
/// default-deny host is reachable. Idempotent — checks for the rule first
/// (`-C`) and only inserts (`-I INPUT`, prepended so the terminal `ACCEPT`
/// beats any later default-DROP) if absent. Runs for both `ip6tables` (the
/// doctrine's primary v6 path) and `iptables`; on iptables-nft systems these
/// are the standard nft-backed shims.
///
/// ADDITIVE ONLY: it adds one `ACCEPT` and never drops anything, so it cannot
/// tighten the host. It genuinely opens the port on the common "bare default
/// DROP policy" lockdown; on a ufw/firewalld-managed host with a conflicting
/// rule it may not, which is why `virtues doctor net` independently verifies
/// real inbound reachability and tells the user the truth.
///
/// Disable with `VIRTUES_WG_MANAGE_FIREWALL=0` if you manage your own firewall.
/// Failures are logged, never fatal (many appliance installs have no
/// restrictive firewall at all, so this is a no-op there).
#[cfg(target_os = "linux")]
fn ensure_inbound_pinhole(port: u16) {
    let disabled = std::env::var("VIRTUES_WG_MANAGE_FIREWALL")
        .map(|v| v == "0" || v.eq_ignore_ascii_case("false"))
        .unwrap_or(false);
    if disabled {
        eprintln!(
            "[virtues-wireguard] firewall management disabled \
             (VIRTUES_WG_MANAGE_FIREWALL=0); ensure inbound udp/{port} is open"
        );
        return;
    }

    let port_s = port.to_string();
    let mut opened_any = false;
    for bin in ["ip6tables", "iptables"] {
        let check = ["-C", "INPUT", "-p", "udp", "--dport", &port_s, "-j", "ACCEPT"];
        let insert = ["-I", "INPUT", "-p", "udp", "--dport", &port_s, "-j", "ACCEPT"];
        // Rule already present → nothing to do (idempotent).
        if run_fw(bin, &check) {
            opened_any = true;
            continue;
        }
        if run_fw(bin, &insert) {
            eprintln!("[virtues-wireguard] opened inbound udp/{port} via {bin}");
            opened_any = true;
        }
    }
    if !opened_any {
        eprintln!(
            "[virtues-wireguard] could not auto-open inbound udp/{port} \
             (no ip(6)tables?); if your firewall is default-deny, open it \
             manually — run `virtues doctor net` to verify reachability"
        );
    }
}

/// Spawn a firewall command silently; return whether it exited 0. Sync
/// `std::process` is fine — this runs once at startup.
#[cfg(target_os = "linux")]
fn run_fw(bin: &str, args: &[&str]) -> bool {
    use std::process::{Command, Stdio};
    Command::new(bin)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|st| st.success())
        .unwrap_or(false)
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("virtues-wireguard runs only on Linux (the appliance).");
    std::process::exit(1);
}
