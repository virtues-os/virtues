# Orange Pi reachability test

> **Purpose:** answer the one question that gates the whole remote-access
> design — *can a device on the public internet reach a box sitting behind a
> normal home router, over IPv6, without a coordinator or relay?*
>
> If yes (out of the box, or with a one-time router rule), the **most private**
> path is viable: **direct WireGuard + a blind rendezvous** — no Virtues
> coordinator, no relay, Virtues touches only the encrypted address lookup.
> If no, we need a fallback (self-hosted no-log coordinator + DERP relay).
>
> The Orange Pi is a Linux SBC = a faithful stand-in for the appliance.

---

## The 5-minute test

You're testing one thing: can an outside packet reach a listener on the Pi?

**1. Find the Pi's *global* IPv6** (a `2xxx:` address, not `fe80…`/`fd00…`):
```sh
ip -6 addr show scope global | grep inet6
```
No global address? → your ISP/router isn't delegating routable IPv6 to the Pi.
Note that result and skip to "If there's no global IPv6" below.

**2. Make sure the *Pi's own* firewall isn't the blocker** (so you're testing
the router, not the Pi):
```sh
sudo ufw disable        # if ufw is installed/active; otherwise check nftables/iptables
```

**3. Start a dead-simple listener** (TCP, easiest — see UDP note below):
```sh
python3 -m http.server 51820 --bind ::
```

**4. From your phone, on CELLULAR (Wi-Fi OFF)**, open a browser to:
```
http://[PASTE-PI-GLOBAL-IPV6]:51820
```
Keep the `[ ]` brackets — required for IPv6 URLs.

**Result:**
- **Directory listing loads** → inbound IPv6 works *out of the box*. 🎉
- **Hangs / can't connect** → the router firewall is blocking inbound (expected
  on most routers until you add a rule — see "If blocked" below).

---

## Interpreting / follow-ups

**Confirm the phone even has IPv6 on cellular:** on the phone, visit
`https://test-ipv6.com`. If the phone is IPv4-only on your carrier, it can't
reach an IPv6-only target (most US carriers are IPv6, but confirm).

**UDP caveat (important):** `http.server` is **TCP**; WireGuard is **UDP**.
The TCP test is the easy first pass for "is inbound allowed at all." Some
routers treat UDP differently, so once TCP works, confirm with UDP:
```sh
# on the Pi:
nc -u -6 -l 51820
# from a laptop on a cellular hotspot:
nc -u -6 <pi-global-ipv6> 51820   # type a line, see if it arrives on the Pi
```

**Probe for programmatic pinholing** (usually unsupported on consumer gear, but
worth a look):
```sh
sudo apt install miniupnpc
upnpc -l        # does the router expose IGD / any IPv6 firewall control?
```

**Sample more than one network** — test a friend's/family's different ISP +
router too. One network isn't representative.

---

## If blocked: add a router rule (per-router)

Inbound IPv6 is firewalled by default (correct security posture). Opening it is
per-router friction, not a universal wall — but it is **not** uniform. Routers
fall into three classes (full click-by-click steps + sources in
[`docs/ipv6-pinhole-setup.md`](docs/ipv6-pinhole-setup.md)):

| Class | Routers | What you do |
|---|---|---|
| **A — real per-port rule** | ASUS, eero, Google Nest, **UniFi**, Linksys, **Verizon Fios** ("IPv6 Pinholes"), OpenWrt, pfSense, OPNsense, Synology, *recent* TP-Link | one inbound rule: allow **UDP 51820** to the box's global IPv6 |
| **B — no per-port control** | **Netgear** Nighthawk/Orbi, **Xfinity** XB7/XB8, **AT&T** BGW210/320, **Spectrum** Sagemcom, *older* TP-Link | bridge/passthrough → put a Class A router behind it and pinhole there |
| **C — no inbound path** | T-Mobile Home, Verizon 5G Home, Starlink (pre-Bypass) | nothing helps → Tier 2/3 fallback below |

Corrections from the first-pass guesses: **Netgear has no configurable IPv6
pinhole** (only a blunt Secured/Open toggle), and **Xfinity** only exposes a
coarse firewall *level*, not a per-port rule — both are Class B, not A. **UniFi**
is the most rotation-resilient (can match a network/prefix, not just a /128).

Rule = allow inbound to the **Pi's global IPv6** on the **WG UDP port** (e.g.
51820/udp). Retest after adding.

---

## If there's no global IPv6

- **CGNAT + no IPv6** (e.g. T-Mobile Home, Verizon 5G, some WISPs): no inbound
  path exists, no rule helps. This is the **Class C** population that needs the
  fallback (Tier 2/3 below), not direct inbound. Self-check: if the gateway's WAN
  IPv4 is in `100.64.0.0/10`, it's CGNAT.
- **Double-NAT** (e.g. own router behind an ISP gateway): bridge/passthrough the
  ISP box so your router gets the delegated prefix, then pinhole on your router.

---

## Caveats to remember

- **Rotating prefix:** a rule pinned to the *full* IPv6 may break when the ISP
  rotates the prefix. Prefer rules keyed on the interface-ID/suffix + port if
  the router allows, or have the box re-apply the rule + re-publish on change.
  (A persistent DHCPv6 DUID maximizes prefix stickiness — see
  `docs/wireguard-pairing.md`.)
- **Bridge mode** on ISP gateways (e.g. Xfinity XB8) can break IPv6 — test
  carefully before relying on it.
- **TCP test ≠ UDP reality** — always confirm the actual WG (UDP) port.

---

## DECIDED: Tier 1 only — direct + blind rendezvous, no coordinator

The architecture is settled (full record:
[`docs/wireguard-pairing.md`](docs/wireguard-pairing.md)
§6). Virtues uses **direct WireGuard over IPv6 + a blind rendezvous** for endpoint
discovery, and **does not** run a coordinator or relay at launch. This test now
tells us *how big the small residue is*, not which mechanism to pick.

| Tier | When | Who's in the path | Status |
|---|---|---|---|
| **1. Direct + blind rendezvous** | inbound IPv6 reachable (Class A, or Class B once bridged / box-as-edge) | nobody — Virtues holds only an encrypted address blob | **THE path** |
| **2. Coordinator-brokered punch** | inbound blocked but NAT is punchable | Virtues sees IP:port + timing (a hole-punch coordinator *cannot* be blind) | **rejected** — breaks "subpoena → nothing" |
| **3. Relay (DERP/TURN)** | not punchable (Xfinity-v6, true CGNAT) | Virtues passes encrypted bytes continuously | **parked** as a future opt-in for the residue only |

Why Tier 1 holds for ~all of the demo: the phone roams free, only the *box's*
address moves, and the rendezvous handles that — **the box publishes-on-change and
is otherwise dark (we don't even know it's online)**, vs. a coordinator that needs
a ~25s keepalive so it's always findable. That contrast is the whole privacy
argument, and it's why the coordinator was rejected.

## What this test decides now

Not *whether* to launch and not *which mechanism* — both settled. It sizes the
**residue** (homes with no inbound IPv6 path even after bridge/box-as-edge):

- **Reachable (Class A, or Class B via passthrough/bridge with the box as edge)** →
  the guided in-app wizard (detect router → exact steps from
  [`ipv6-pinhole-setup.md`](docs/ipv6-pinhole-setup.md) → live
  re-check) covers them. Expected to be the large majority of the demo.
- **Genuinely stuck (Xfinity-where-bridge-kills-v6, true CGNAT — not the demo)** →
  pre-purchase qualifier today; the parked opt-in relay later. The "20% fail" worry
  is really "what fraction land here," and for this demo it's small.

Record the result per ISP/router here as you test:

| ISP | Router | Global IPv6? | Inbound out-of-box? | After manual rule? | Notes |
|---|---|---|---|---|---|
| _e.g. Comcast_ | _e.g. ASUS RT-…_ | | | | |
