# Opening the IPv6 pinhole — per-router setup

> **What this is:** the click-by-click reference for the one manual step the
> most-private remote-access path (Tier 1: direct WireGuard + blind rendezvous)
> can require — opening an **inbound IPv6 firewall rule** on the home router so
> the phone can reach the box directly.
>
> This doc is the **source of truth for the in-app setup wizard**. The wizard
> detects the router (UPnP discovery / gateway fingerprint) and shows the
> matching section below; this file is where we keep those steps current.
>
> Companion: [`orangepi.md`](orangepi.md) (the reachability test that tells
> a given home which tier it lands in) and
> [`wireguard-pairing.md`](wireguard-pairing.md) (the WG transport this rides on).

---

## The rule, in one sentence

**Allow inbound IPv6, protocol UDP, to the box's global IPv6 address, on the
WireGuard port (default `51820/udp`).**

That's the whole thing. Everything below is just "where is that setting on *your*
router."

### Why it's safe to open (say this to nervous users)

WireGuard answers **only** cryptographically-authenticated packets and silently
drops everything else — no reply, no banner, no error. To a port scanner the open
UDP port is indistinguishable from a closed one. **Opening `51820/udp` to the
WireGuard listener exposes nothing** to anyone who doesn't already hold a paired
key. (Source: wireguard.com/protocol — WG is explicitly designed "not to respond
to any unauthenticated packets, thus hampering scanners and service discovery.")

So this is the *most secure* form of remote access: no relay sees your traffic,
no coordinator sees your address, and the open port is invisible.

---

## Before you touch the router: pin the box's address

Every router below keys the rule on the box's IPv6. ISPs rotate the **prefix**
(the front half) periodically; we want the box's **interface ID** (the back half)
to stay constant so the rule keeps matching across rotations, and so the
rendezvous only has to re-publish the prefix change, not chase a moving suffix.

On the box (VirtuesOS does this automatically; documented here for the spec):

- Assign a **static interface ID** (or a stable EUI-64), and
- **disable SLAAC privacy / temporary addresses** (`net.ipv6.conf.*.use_tempaddr=0`)
  so the box presents one predictable global address, not a rotating set.

Then the rule's stable target is `<rotating-prefix>:<our-fixed-suffix>`. When the
prefix rotates, the box re-applies the rule (where the router API allows) and
re-publishes its endpoint to the rendezvous regardless — see the rotation section
at the end.

---

## Capability classes (what to expect)

Routers fall into three buckets. The wizard branches on these.

| Class | Meaning | Routers |
|---|---|---|
| **A — has a real per-port IPv6 allow rule** | Tier 1 works; one rule and you're done | ASUS, eero, Google Nest, UniFi, Linksys, Verizon Fios, OpenWrt, pfSense, OPNsense, Synology, **recent** TP-Link |
| **B — no per-port IPv6 control** | gateway can't express the rule; put it in bridge/passthrough and let a **downstream router you control** (any Class A) do the pinhole | Netgear Nighthawk/Orbi, Xfinity XB7/XB8, AT&T BGW210/320, Spectrum Sagemcom/Askey, **older** TP-Link |
| **C — no inbound path exists at all** | CGNAT or locked carrier core; **no rule helps** — these homes fall to the Tier 2/3 fallback (coordinator/relay) | T-Mobile Home Internet, Verizon 5G Home, Starlink (until Bypass Mode + own router) |

---

# Class A — one rule and done

### ASUS (RT-AX series, ZenWiFi / AiMesh)
Best-in-class: up to 128 rules, explicit UDP.
1. Enable IPv6 first: **IPv6** menu → set a connection type (Native / Passthrough as your ISP needs).
2. **Firewall → IPv6 Firewall** tab → **Enable IPv6 Firewall = Yes**.
3. Under **Inbound Firewall Rules**, add a row:
   - **Service Name:** `WireGuard`
   - **Remote IP/CIDR:** *(blank = any source)*
   - **Local IP:** the box's full global IPv6
   - **Port Range:** `51820`
   - **Protocol:** **UDP**
4. **Apply.**
- Caveat: keyed on the full **Local IP** — re-enter if the box's address changes. Doc: asus.com/us/support/faq/1013638/

### eero (eero 6 / Pro / Max)
No eero Plus subscription required for this.
1. eero app → **Settings → Network Settings → Reservations & Port Forwarding**.
2. Scroll to **IPv6 Firewall Rules → + Add**.
3. Enter the box's **IPv6 address**, **port** `51820` (start=end), **protocol UDP** (or Both).
4. Save.
- Keyed on the full /128 — re-enter on prefix rotation. Doc: support.eero.com/hc/en-us/articles/207908443

### Google Nest Wifi / Nest Wifi Pro
Google calls inbound IPv6 "**Port opening**" (distinct from IPv4 "Port forwarding").
1. Google Home app → **Wi-Fi** tile → **Settings (gear) → Advanced Networking → Port management**.
2. **Add** → choose the **IPv6** tab.
3. **Select the box** from the discovered-device list, enter **port range** `51820–51820`, protocol **UDP** (or "TCP and UDP").
4. Save.
- Limitation: you pick a *discovered device*, can't type an arbitrary address; if the box isn't listed, fix its address stability first. Doc: support.google.com/googlenest/answer/6274503

### Ubiquiti UniFi (UDM / UDM Pro / Cloud Gateway)
Most rotation-resilient — can match a **network/prefix**, not just a /128.
1. **Settings → Security** (Policy Table / Zone-Based Firewall) → **Create Policy**.
2. **IP Version = IPv6**; **Source Zone = External (WAN)**; **Destination Zone = Internal** (or **Gateway** if WG terminates on the UDM itself).
3. **Protocol = UDP**; **Destination = the box** (host /128 *or* the internal network object); **Destination Port = 51820**; **Action = Allow**.
4. Add a companion **Allow ICMPv6** policy (NDP / PMTUD).
5. Place the rule **above** the predefined drops; apply.
- Older consoles: **Settings → Firewall & Security → Internet v6 In**. Doc: help.ui.com/hc/en-us/articles/27699646208279

### Linksys (Velop mesh, MX series)
1. Linksys app or web dashboard → **Router Settings → Security → IPv6 Port Services**.
2. Add entry: **Protocol = UDP** (or Both), **IPv6 Address** = the box's global address, **Allow (port)** = `51820`.
3. Save.
- Use the **global** address (not `fe80::`); typing a `/mask` is rejected. Full-address only — re-enter on rotation. Doc: linksys.com/support-article?articleNum=243551

### Verizon Fios (G3100, CR1000A/B)
The cleanest ISP gateway — Verizon literally ships "IPv6 Pinholes."
1. Log in (`192.168.1.1` or `myfiosgateway.com`).
2. **Advanced → Security & Firewall → IPv6 Pinholes** (CR1000A: **Advanced → Firewall → IPv6 Pinhole**).
3. **External Host:** blank/`*` (any). **Internal Host:** select the box. **Protocol:** **UDP**. **Source/Dest Port:** `51820`. **Schedule:** Always.
4. **Add to list → Apply Changes.**
- Disable SLAAC privacy addresses on the box so the discovered host stays put. Fios prefixes are relatively sticky. Doc: Verizon router guide (IPv6 Pinholes).

### OpenWrt (LuCI)
1. **Network → Firewall → Traffic Rules → Add.**
2. Protocol **UDP**; Source zone **wan**; Destination zone **lan**; Destination address = box's IPv6; Destination port **51820**; Action **accept**.
3. **Advanced Settings → Restrict to address family = IPv6.**
4. Save & Apply. Doc: openwrt.org/docs/guide-user/firewall/firewall_configuration

### pfSense
1. **Firewall → Rules → WAN → Add.**
2. Action **Pass**, Interface **WAN**, Address Family **IPv6**, Protocol **UDP**, Source **any**, Destination **Single host** = box's IPv6, Dest port **51820**.
3. Save → Apply. (No NAT needed — IPv6 is routed, not NATed.) Doc: docs.netgate.com/pfsense/en/latest/vpn/wireguard/rules.html

### OPNsense
1. **Firewall → Rules → WAN → +.**
2. Action **Pass**, Direction **in**, TCP/IP Version **IPv6**, Protocol **UDP**, Source **any**, Destination = box's IPv6, Dest port range **51820**.
3. Save → Apply. Doc: docs.opnsense.org/manual/firewall.html

### Synology SRM (RT2600ac / RT6600ax)
1. **Network Center → Security → Firewall.**
2. Select the firewall profile → **Create** a rule: Protocol **UDP**, Ports **Custom 51820**, Source IP **All**, Action **Allow**.
3. Order matters — place above the default any/any deny. Doc: kb.synology.com/en-us/SRM/help/SRM/NetworkCenter/security_firewall

### TP-Link (recent Archer / select Deco) — *verify support first*
Spotty: only newer firmware has it.
- **Archer (web):** **Advanced → IPv6**, scroll down to **IPv6 Firewall Rules** (sits just below "MAC Clone"). **Tell-tale check: if the page ends at "MAC Clone," this router doesn't support it** → treat as Class B. Fields: Service Name, device IPv6, Port `51820`, Protocol **UDP**.
- **Deco (app):** only certain models (BE65-5G, X50-4G, X20-4G, X10-4G, X80-5G, X50-5G) and **only in "Wi-Fi Router" mode**: **More → IPv6 → IPv6 Firewall**.
- Docs: tp-link.com/us/support/faq/2642/ (Deco), Archer AX6000 user guide ch.9.

---

# Class B — gateway can't do it; let the Virtues box take the edge

The gateway can't express the rule, so don't fight it — **demote it and let the
Virtues box hold the perimeter.** Primary path (no extra hardware): put the ISP
gateway into **IP Passthrough / bridge** so it hands the public IPv6 to the
**Virtues box**, which then **firewalls itself** — the router-config step
disappears entirely for these users, and it's the most on-brand posture (your
privacy box guards the front door). If the user already runs a Class A router,
the equivalent is to passthrough/bridge to *that* and pinhole there.

This is the decided answer (not a "buy a $99 router" upsell — see
[`wireguard-pairing.md`](wireguard-pairing.md) §6). It dissolves most of Class B
without a purchase; the genuine residue (Xfinity where bridge kills IPv6) falls to
the parked opt-in relay.

### Netgear (Nighthawk RAX, Orbi)
No configurable IPv6 pinhole — only a coarse **IPv6 Filtering: Secured / Open**
toggle (Orbi) or nothing (many Nighthawk). "Open" disables IPv6 firewalling
entirely (not recommended). Port forwarding is IPv4-only.
→ **Recommended:** run a Class A router behind it, or fall back to WireGuard over
IPv4 + DDNS. Doc: kb.netgear.com/24006

### Xfinity / Comcast (XB7, XB8)
Only a blunt **IPv6 firewall level** (Typical / Custom / Disabled) at
`http://10.0.0.1` → **Gateway → Firewall** — no per-host/per-port v6 allow rule.
**Bridge mode disables IPv6 entirely** on the XB8, so you can't bridge *and* keep
native IPv6 from the gateway.
→ **Practical options:** lower the IPv6 firewall level (blunt), or use a separate
modem + your own router for a real pinhole. Note this is the awkward one.

### AT&T Fiber (BGW210, BGW320)
No native per-port IPv6 pinhole; "Packet Filter" is IPv4-oriented. No true bridge
mode either.
→ **Recommended:** **Home Network → IP Allocation → IP Passthrough** (Allocation
Mode = Passthrough → your downstream router's MAC). AT&T delegates a /60; the
downstream router takes a /64 and **enforces its own IPv6 firewall** — open
`51820/udp` there. Double-NAT only affects IPv4; IPv6 routes via the delegated
prefix.

### Spectrum / Charter (Sagemcom, Askey RAC2V1A)
Exposes IPv4 port forwarding and a coarse IPv6 security toggle — no per-port v6
allow screen.
→ **Recommended:** **bridge mode** + your own router doing the pinhole. Spectrum
IPv6 is routable (not CGNAT). Doc: spectrum.net/support/internet/ipv6

---

# Class C — no inbound path; these need the fallback

No firewall rule exists that helps. These homes route remote access through the
**Tier 2/3 fallback** (self-hosted no-log coordinator / DERP relay — see the tier
ladder in [`orangepi.md`](orangepi.md)).

- **T-Mobile Home Internet:** CGNAT, no port forwarding, inbound carrier-filtered.
- **Verizon 5G Home (consumer):** CGNAT, same.
- **Starlink router (Gen2/Gen3):** hands out a /56 but blocks all inbound IPv6
  with no firewall UI. *Escape hatch:* **Bypass Mode** (Starlink app → Settings →
  Bypass Mode) + your own router requesting DHCPv6-PD → then it becomes Class A/B.
  Gen1 has no IPv6 at all.

**CGNAT self-check (put this in the wizard):** read the gateway's WAN IPv4 — if
it's inside `100.64.0.0/10`, the home is behind CGNAT and no pinhole is possible.

---

## The prefix-rotation caveat (applies to every Class A rule)

A rule pinned to the **full /128** breaks when the ISP rotates the prefix. Layered
mitigations, best-first:

1. **Stable interface ID on the box** (above) → only the prefix changes, the suffix
   is constant. Routers that match on **network/prefix** (UniFi) then never break.
2. **Box re-applies the rule** on prefix change where the router exposes an API
   (OpenWrt/UCI, UniFi API, pfSense) — opportunistic, not universal.
3. **Rendezvous re-publish** handles the *phone* side regardless of the rule: when
   the prefix rotates, the box re-publishes its new endpoint to the blind
   rendezvous, and the phone re-fetches on its next handshake failure. So even
   when the rule needs a manual touch, address *discovery* is automatic.

---

## Why we don't rely on auto-opening the pinhole (PCP / UPnP IGDv2)

There are two standards a LAN host could use to open its own IPv6 pinhole — **PCP
(RFC 6887)** and **UPnP IGDv2 `WANIPv6FirewallControl`** — but real-world consumer
support is essentially limited to **AVM FritzBox, OpenWrt, pfSense, OPNsense**, and
it's flaky even there (FritzBox's IGDv2/PCP has been buggy; OpenWrt's miniupnpd
`AddPinhole` returns "606 Action not authorized" on some builds; pfSense/OPNsense
support it but it's off by default). The vast majority of consumer gear ignores
both.

**Decision:** the wizard *may* attempt PCP/UPnP opportunistically (zero-click win
where it works), but **manual configuration is the supported, documented path**.
We never promise auto-open.

---

## What this means for the product

- **Tier 1 is reachable for most of the sovereign audience.** ASUS / UniFi / eero
  / Nest / Fios / OpenWrt / pfSense / Synology — the routers privacy-minded users
  actually own — all support a clean per-port UDP rule. That's the "we touch
  nothing" path.
- **The friction is real but bounded:** a guided, router-specific wizard (detect →
  show the exact screens above → verify with a reachability re-test) turns this
  from "networking project" into a 2-minute tap-through. AI-generated,
  version-tracked per-router guides keep it current as firmware UIs drift.
- **Class B is "let the box take the edge"** — a one-time passthrough/bridge so
  the Virtues box (or an existing Class A router) holds the perimeter and firewalls
  itself. No purchase, no per-router rule for these users. Worth a dedicated wizard
  branch for the big ISP gateways (AT&T/Spectrum).
- **Class C is the genuine residue** — Xfinity-where-bridge-kills-v6 + true
  CGNAT/5G (not the demo). No coordinator is being built (it would break
  "subpoena → nothing"); these homes are a pre-purchase qualifier today and the
  **parked opt-in relay** later. The Orange Pi test + the in-app IPv6 diagnostic
  sort homes into these buckets automatically.
