# Networking — how you reach your box

> **Source of truth** for Virtues' remote-access model. Supersedes the old
> `wireguard-pairing.md`, `orangepi.md`, and `ipv6-pinhole-setup.md` (removed),
> which described a hole-punch coordinator + "blind rendezvous + parked relay"
> design that no longer exists.

## The doctrine

**We're not building a network. We're giving you back a real computer on the
real internet — and refusing to stand between you and it, or to let anyone
else.**

Your box gets a real, globally-routable **IPv6** address and is reached
**directly over WireGuard** (the WG handshake *is* the trust pin — SPKI, no CA).
NAT + IPv4 scarcity turned everyone into a tenant behind a carrier's wall; every
overlay — a coordinator, a relay, hole-punching — is scar tissue grown over that
wound. IPv6 heals it. **Virtues operates no coordinator and no relay.**

### The keystone: auth at the app layer, network as dumb transport

The box authenticates every request from its **own** credentials (a paired
device's bearer token / the device-list ACL), **not** from the path the request
arrived over. So the box is transport-agnostic: it answers identically over
Virtues' WireGuard, a BYO overlay, or plain LAN — all passing the same app-layer
auth. The pipe is never the trust boundary; the key is.

## How you reach the box

| Your situation | Path | Virtues in the loop? |
|---|---|---|
| Same network as the box | direct (LAN / mDNS `virtues.local`) | no |
| Box has global IPv6 (most home ISPs, all mobile) | **direct over IPv6** — the recommended path | no |
| Home IPv4 with a router you control | port-forward `udp/51820` | no |
| No reachable address / network you don't control (dorm, office, CGNAT) | **BYO overlay you run** — see [byo-networking.md](byo-networking.md) | no (your infra) |

The honest boundary: **you cannot host a sovereign computer on a network you
don't control.** The box lives where you have a real network (home, IPv6); you
reach it from anywhere. If you control no network, that's a real-world problem —
host it somewhere you control, or run your own overlay. Virtues never runs or
requires one.

## Opening the inbound pinhole

For a direct connection the box must accept inbound `udp/51820` (the WireGuard
listen port; override with `VIRTUES_WG_LISTEN_PORT`).

**Automatic.** The `virtues-wireguard` daemon opens the pinhole itself at
startup (idempotent `ip6tables`/`iptables` ACCEPT, additive-only — it never
tightens the host). Disable with `VIRTUES_WG_MANAGE_FIREWALL=0` if you manage
your own firewall.

**Manual (when your router/firewall is default-deny and you control it).** The
rule, in one sentence:

> Allow **inbound IPv6, UDP, to the box's global IPv6 address, on `udp/51820`.**

**Why it's safe to open:** WireGuard answers *only* cryptographically-authenticated
packets and silently drops everything else — no reply, no banner. To a scanner
the open port is indistinguishable from a closed one, and an IPv6 `/64` is
unscannable. Opening `udp/51820` exposes **nothing** to anyone who doesn't hold
a paired key. The address is a speed-bump, not a credential — the WG key + the
app-layer auth are the real locks.

**Prefix rotation.** ISPs rotate the IPv6 *prefix* (the front half) periodically.
Keep the box's *interface ID* (the back half) stable — assign a static interface
ID and disable SLAAC privacy/temporary addresses
(`net.ipv6.conf.*.use_tempaddr=0`) — so the box presents one predictable global
address and the router rule keeps matching across rotations.

## Checking reachability — `virtues doctor`

`virtues doctor` reports the box's network class and, when it has a global IPv6,
actively confirms inbound works:

- **`ipv6-direct`** — "Global IPv6 detected (…) — direct access works here."
- **`behind-nat`** — "No global IPv6 — forward `udp/51820` (home) or use a BYO
  overlay (dorm/office)."
- **`inbound: ✓ confirmed reachable`** — the box asked `virtues-api`'s
  `/v1/net/probe` to fire a UDP nonce back at its own address and received it.
  This is the one honest inbound test (a box can't test its own firewall from
  inside). It only ever confirms positively — never a false "blocked."

`virtues status --json` carries a paste-safe `network` section (class + a
boolean, no literal addresses) for support tickets.

## What Virtues *does* run (and what it sees)

Exactly one optional touchpoint: the **blind rendezvous** on `virtues-api`
(`/v1/rendezvous/:publish_id`). The box publishes its current endpoint there —
**encrypted under a key only the box and its paired devices hold** — so a device
can relearn the box's address after a prefix rotation. Virtues stores an opaque
`publish_id` → ciphertext; it cannot read the endpoint or tie it to a customer.
And the `/v1/net/probe` echo fires one UDP nonce back at the caller's *own*
observed address (no reflection, stores nothing). Neither is a coordinator and
neither is a relay — no traffic ever flows through Virtues.

## Security baseline

- WireGuard is **silent** — drops non-peer packets with no reply, so the WG port
  is invisible to scanners.
- An IPv6 `/64` is **unscannable** — you won't be found by mass scanning.
- The box binds `[::]:8000` (dual-stack) for its HTTP server, reachable only
  over a tunnel/overlay or behind the pinhole; everything sensitive is gated by
  app-layer auth, never by the network.

## See also

- [byo-networking.md](byo-networking.md) — recipes for Tailscale / Headscale /
  plain-WG-VPS / Cloudflare / Tor / dynamic-DNS+IPv6.
- [auth-model.md](auth-model.md) — the pairing + device-list + bearer model.
