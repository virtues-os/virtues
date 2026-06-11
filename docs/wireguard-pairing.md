# WireGuard Remote Access + QR Pairing (WS-2) — Implementation Plan

> The networking centerpiece. Gets a phone/laptop talking to the home server
> from anywhere, with no accounts and no public device DNS. This plan also
> absorbs the original WS-3 (netns harness) and WS-9 (onboarding), since they
> can't be validated independently.

---

## 1. Current state

- **Transport seam exists** (WS-1): `ServerTransport` trait in
  `crates/virtues-helpers/src/transport/` with `RealServerTransport` (plain TCP)
  and `DevLocalServerTransport` (loopback, gated by the `dev-transport` feature).
  `virtues-core/src/server/mod.rs` binds the listener behind `build_transport()`. The
  trait is the hook WS-2 fills.
- **Pairing exists but is placeholder**: `virtues-core/src/api/source_auth.rs`
  (`POST /api/pairing/initiate` + `/complete/:credential_id`). The web
  `DevicePairModal.svelte` shows a QR of `{e: endpoint, s: source_id}`. The
  credential model (`virtues-core/migrations/0004_*`) stores a self-issued bearer
  encrypted in `credentials.secrets_ciphertext` with an HMAC `secret_lookup_hash`
  for O(1) webhook auth.
- **iOS uses `deviceToken = deviceId`** (a stable UUID as the bearer) —
  `DeviceConfiguration.swift:~64`. This is the explicit no-stable-bearer
  violation the lint forbids.
- **Nothing else**: no WireGuard, no per-pair CA, no `virtues.internal`
  resolution, no mDNS. All scaffold comments only.

---

## 2. Locked decisions (from the parent plan)

1. **Kernel WireGuard** via netlink (`defguard_wireguard_rs`), with the bundled
   userspace backend (`defguard_boringtun`) as an automatic runtime fallback
   when `ip link add type wireguard` fails. *Reversed the original "userspace,
   never kernel" call* — confirmed on real hardware (Orange Pi 5 Plus, RK3588,
   Armbian/Debian 13, kernel 6.18, aarch64): module loads, netns handshake +
   IPv6 listen pass, and the crate drives the kernel interface end-to-end both
   bare-metal and in a `NET_ADMIN` container. See D1.
2. **AP-only WiFi onboarding + ethernet auto-detect.** No BLE.
3. **IPv6 readiness check at first boot**, not pre-ship.
4. **Two TLS contexts**: per-pair private CA for `virtues.internal` (device);
   standard public PKI for cloud infra (Atlas, virtues-api).
5. **On-network browser + WG-installed laptop: yes. Off-network browser:
   deferred** — the app is the only away-from-home surface at launch.

## 3. Decisions to make before coding

| # | Decision | Options | Recommendation |
|---|---|---|---|
| D1 | Server WG engine | kernel via netlink (`defguard_wireguard_rs`) · userspace (`defguard_boringtun`) · `wireguard-go` subprocess | **DECIDED — kernel via `defguard_wireguard_rs` 0.9.6**, userspace as the auto-fallback (it's a transitive dep of the same crate, so free). Spike passed on the Orange Pi 5 Plus. API notes: methods are on the `WireguardInterfaceApi` trait (must import); `WGApi<Kernel>` vs `WGApi<Userspace>`; `InterfaceConfiguration { name, prvkey, addresses: Vec<IpAddrMask>, port, peers, mtu, fwmark }`; the read-back `Host` has no `public_key` field — derive it from the private key. Crate `examples/{client,server,userspace}.rs` match the 0.9.x API. |
| D2 | iOS WG engine | `WireGuardKit` (official, NetworkExtension `NEPacketTunnelProvider`) | **WireGuardKit** — it's what the official WireGuard iOS app ships. |
| D3 | Address plan | ULA IPv6 (`fd00::/8`) per-pair `/128` · IPv4 `10.x` | **ULA IPv6**, matching the netns harness's synthetic-IPv6 design; dual-stack the interface to be safe. |
| D4 | TLS trust model | true per-pair CA · single long-lived **server CA**, root shipped in each pair bundle | **Single server CA**, per-pair *trust distribution*. Equivalent threat model (no public CT logs, client pins it only for `virtues.internal`), far simpler to operate. Flag as a refinement of the plan's "per-pair CA" language. |
| D5 | **Remote rendezvous** (the hard one) | QR-pinned static endpoint · dynamic DNS · lightweight push-over-tunnel broker · **coordinator (Headscale/Netbird-style)** | **DECIDED — direct WG over IPv6 + a *blind rendezvous* for endpoint discovery. No Virtues coordinator, no relay at launch.** See §6. |
| D6 | mDNS lib (LAN auto-heal) | `mdns-sd` crate | `mdns-sd`. |

---

## 4. The handshake bundle

Pairing must provision **everything in one exchange** (QR scan → one round-trip).
The bundle the device receives:

- **WG**: server public key, server endpoint (see §6), assigned client IP
  (ULA `/128`), pre-shared key, allowed-IPs.
- **Bearer**: a fresh 32-byte random device bearer (replaces `deviceId`).
- **Resolution**: `virtues.internal` → assigned server WG IP (client-side only,
  never public DNS).
- **Trust anchor**: the server CA root cert, pinned by the client for
  `virtues.internal` only.
- **Endpoint pin**: for re-validation on reconnect.

The QR itself stays small — it can carry a one-time pairing code + LAN endpoint;
the device then pulls the full bundle over the initial LAN connection during
`pair_complete`. (QR capacity is limited; don't stuff keys+certs into it.)

---

## 5. Phases

### Phase A — Kill the stable bearer *(independent, ship first, no WG)*
- `pair_complete`: server generates a 32-byte random bearer, stores it encrypted
  (the vault's `finalize_self_issued_bearer` already encrypts a token — feed it
  the random bearer instead of the device UUID), returns it to the device once.
- iOS: store the returned bearer in Keychain; use it as `deviceToken`; delete the
  `deviceToken = deviceId` path.
- **Closes the no-stable-bearer lint** and is shippable on its own. Pre-launch
  drop-and-repair of existing devices is acceptable (we're pre-launch).

### Phase B — WG provisioning (server)
- New `virtues-core/src/wireguard/` module: manage the userspace interface (per D1), peer
  config, keygen, PSK.
- At pairing: generate per-pair WG keypair + PSK, assign a client ULA IP, add the
  peer, return WG params in the bundle. Persist peer config in
  `credentials.metadata` (device WG pubkey, assigned IP).
- One inbound port to the internet (e.g. `51820/udp`).

### Phase C — `virtues.internal` + server CA (TLS context 1)
- `rcgen`: mint a long-lived server CA + a leaf for `virtues.internal`
  (SAN: `virtues.internal` + assigned WG IP). Ship the CA root in each pair bundle.
- Client pins that root **for `virtues.internal` only** (scoped trust).

### Phase D — Transport integration
- New `WireGuardServerTransport` (or compose: WG at the userspace layer +
  `TlsServerTransport` that terminates rustls with the `virtues.internal` cert,
  binding on the WG interface IP). `dev-transport`/`DevLocal` stays untouched.

### Phase E — iOS client
- `WireGuardKit` + `NEPacketTunnelProvider` target: build the tunnel from the
  bundle. The tunnel itself provides encryption + authentication (Noise IK
  handshake = SPKI pin); no TLS on top.
- `QRScannerView`: parse the pairing code; `DeviceConfiguration` stores WG
  config + bearer + endpoint. `NetworkManager`: dial
  `http://virtues.internal:8000` through the tunnel (the WG client maps
  `virtues.internal` to the box's WG address).
- `SettingsView`: reachability status (tunnel up/down, last handshake age).
- Re-pin paths: LAN mDNS auto-heal, push-over-live-tunnel, physical "show QR"
  re-pair button.

### Phase F — netns harness (was WS-3)
- `crates/virtues-wg/tests/netns/`: Linux netns + userspace WG + synthetic ULA IPv6 +
  `virtues.internal` hosts entries. Scenarios: prefix rotation, NAT change, ISP
  swap → assert auto-heal / re-pair recovery. CI on Linux; macOS dev in a
  container.

### Phase G — Onboarding (was WS-9) *(parallel/after E)*
- AP-only WiFi setup: soft-AP `Virtues-XXXX` → app pushes home creds → device
  joins → AP down. Ethernet auto-detect skips the AP step. First-boot IPv6
  diagnostic (non-blocking warn). QR pairing UX is the centerpiece.

---

## 6. Remote rendezvous — DECIDED (D5)

A WG tunnel needs the server's *current* public endpoint. The QR pins one at
pairing, but home IPs rotate (DHCP lease, ISP change, prefix rotation). Inside
the LAN, mDNS re-discovers the box. **Away** from home, the phone needs to learn
the new endpoint.

**The decision: direct WireGuard over IPv6, with a *blind rendezvous* for
endpoint discovery. No Virtues coordinator. No relay at launch.**

The phone connects *directly* to the box's public IPv6 (one inbound
`51820/udp` pinhole). Discovery is the only Virtues-operated touchpoint, and it's
built so Virtues can't read or correlate it:

- **The rendezvous is a tiny KV**: `publish_id → encrypted_blob`. The blob is the
  box's current endpoint, encrypted with a per-box key **K** that exists only on
  the box and its paired devices — **Virtues never holds K**, so the stored value
  is opaque to us.
- **`publish_id` is an opaque, capability-style key** (random, per-box). Holding
  it is the read capability; it ties to no customer and no usage bearer.
- **Box publishes on change**: when its prefix/endpoint rotates, the box re-encrypts
  and `PUT`s the new blob, then goes quiet. No persistent connection to Virtues —
  we don't even know the box is online. (Contrast a coordinator, which needs a ~25s
  keepalive so it always knows where the box is. *Publishes-on-change vs.
  phones-home-constantly* is the whole privacy difference.)
- **Phone fetches on failure**: the phone uses its cached endpoint; only when a WG
  handshake fails does it `GET publish_id`, decrypt with K, and re-dial.
- **Writes are bearer-authed** (reuse the entitlement bearer — see
  [`entitlement.md`](entitlement.md) — no new per-box write token), so abuse is
  gated by the same "I'm a paying subscriber" anonymous pass that gates everything
  else. Reads need only the capability `publish_id`.
- **No logs, TTL'd.** The KV keeps nothing legible and expires stale rows.

This is the fourth party in [`Virtues-API.md`](../Virtues-API.md): "an opaque
pointer to an encrypted address it can't read."

### Why not a coordinator (and what else was rejected)

The alternative — a Virtues-run Headscale/Netbird-style coordinator — was
**rejected**. A hole-punch coordinator *cannot be blind*: to broker NAT traversal
it must see both peers' IP:port + timing, and the box must phone home continuously
so it's always findable. That is exactly the connection metadata a subpoena could
reach — it breaks "subpoena Virtues → nothing" at the connection layer, the one
property that separates us from Tailscale/Plex/Syncthing (all of which keep a
non-blind coordinator + relay). Also rejected: **Tor hidden services** (what
Start9/Umbrel default to) — too slow/unreliable and the "dark web" association
repels the mainstream edge of the demo; and **third-party overlays**
(Tailscale/Telegram-as-relay) — outsources the network, off-brand.

We can hold this purist line *because we own the full stack* (hardware + iOS app +
tunnel), which lets us make direct-connect silent — no DDNS, no Nginx, no SSL, no
second app — the misery that pushed the DIY crowd toward outsourcing.

### Reachability coverage (how the tail is handled — no relay needed for ~all of the demo)

IPv6-first is the enabling assumption (IPv6 crossed 50% globally in 2026 and is
far higher in the target ISPs). The phone roams freely; the *box's* address is the
only thing that moves, and the rendezvous handles that. Router-side inbound is the
one manual step, handled by the onboarding wizard — full per-router detail in
[`ipv6-pinhole-setup.md`](ipv6-pinhole-setup.md):

- **Premium mesh (eero / Nest / ASUS / UniFi) + clean ISPs (Fios, etc.):** one
  in-app-guided firewall rule. The bulk of the demo.
- **No-pinhole ISP gateways (AT&T, Spectrum, …):** *not* a router upsell — the
  **Virtues box takes the edge.** IP Passthrough (AT&T) / bridge (Spectrum) hands
  the box the public address and it firewalls *itself*. Zero extra hardware; the
  router-config step disappears for these users.
- **Genuine residue (Xfinity where bridge kills IPv6; true CGNAT — T-Mobile/5G,
  not the demo):** the *only* place a relay would earn its keep. **Parked as a
  future opt-in relay**, never the default — so the purity holds for the ~95% and
  no one is turned away in the long run. Until then it's a pre-purchase qualifier
  (the first-boot / pre-ship IPv6 reachability check — see
  [`orangepi.md`](orangepi.md)).

### What this commits us to

1. **Never silently add a coordinator later** — it would split the product and
   retroactively weaken the launch claim. A relay may only ever appear as an
   explicit, opt-in mode for the residue.
2. **Box-side address stability**: static interface ID + disabled SLAAC privacy
   addresses, so only the prefix moves; re-apply the firewall rule via router API
   where one exists (UniFi/OpenWrt/pfSense) and always re-publish to the rendezvous.
3. **The wizard must own the messy 30%** — detection, the rotation re-bind (a rule
   that passes the live-check can silently die weeks later on prefix rotation), and
   per-router guide drift (AI-generated, version-tracked) — not just the happy-path
   five taps.

Resolve nothing further here; this gates Phase E and is now settled.

---

## 7. Critical files / new deps

**core**: new `wireguard/` module · `server/mod.rs` transport wiring ·
`api/pairing.rs` + `api/source_auth.rs` (bundle) · `credentials.metadata`.
**helpers**: `transport/` — `WireGuardServerTransport` / `TlsServerTransport`.
**iOS**: `DeviceConfiguration.swift`, `NetworkManager.swift`, `QRScannerView.swift`,
new `PacketTunnelProvider` target, `SettingsView.swift`.
**web**: `DevicePairModal.svelte` (QR payload shape).
**new deps**: `boringtun` *or* `wireguard-go` mgmt, `rcgen`, `mdns-sd` (core);
`WireGuardKit` (iOS); `qrcode` (web, already present).

---

## 8. Sequencing

**A** (bearer fix — ship now, closes the lint) → **B** (WG server) → **C** (CA/TLS)
+ **D** (transport) → **E** (iOS) → **F** (harness validates) → **G** (onboarding).

A is independent and high-value. B–E are the tunnel. F is the gate. G is polish.
D5 (rendezvous) must be decided before E.
