# Virtues — Networking & Security Research Brief

> **Purpose of this doc.** This is a briefing for a deep-research agent to go
> research *online* and come back with notes, comparisons, and best-practice
> recommendations. It explains what Virtues is, who it's for, how its
> networking/security work today, where the pain points are, and the specific
> open questions we want answered. Read the "Research questions" section last —
> everything before it is context so the answers land in our reality.

---

## 1. What Virtues is

Virtues is a **self-hosted personal-data appliance**. A user runs a "box" at home
(a small Linux computer — a Jetson/NVIDIA box or a CPU mini-PC) that collects the
user's own data (health, location, finance, calendar, contacts, audio, etc.),
stores it locally, and runs AI/agents over it. The whole pitch is **sovereignty**:
your data lives on your hardware in your home, processed by your box, *not* in
anyone's cloud.

Clients (a macOS app, an iOS app, a web UI) connect to the box to view data,
configure it, and feed it sensor streams. The box is the server; the clients are
"devices" the user pairs to it.

**Hard product constraints / philosophy:**
- **No cloud compute, no cloud data.** The box does all the work and holds all the
  data. We are willing to run *thin* cloud infrastructure (a discovery/rendezvous
  broker) **only if it is zero-knowledge** — it must never see plaintext or user
  data.
- **Sovereignty over convenience** is the brand, but the product must still be
  usable by non-technical people (see personas).
- **No third-party SaaS on the critical path** as the *default/core* paradigm. We
  refuse to make e.g. Tailscale a mandatory middleman between a user and their own
  home box. (Power users may BYO Tailscale/Headscale; it just isn't shipped as the
  core.)

## 2. Target personas

This is the single most important framing for the research:

- **Primary persona: non-technical.** Does **not** know what NAT is, what IPv6 is,
  what a port is, what Tailscale/WireGuard/VPN are, what a relay is, what
  "userspace" means, or even the difference between HTTP and HTTPS. They will
  **not** configure a router, set up port forwarding, install a VPN, or "read the
  docs" to get connectivity working. For them, reachability must be **zero-config
  and invisible** — pair with a code, and it just works from anywhere.
- **Secondary persona: prosumer / technical self-hoster.** Owns their network or is
  happy to BYO networking (Tailscale, a VPS relay, port forwarding, their own
  domain). Comfortable with the appliance model. This is who self-hosting
  historically serves, but it is **not** who we're primarily building for.

The tension: the sovereignty/no-cloud philosophy points one way; the non-technical
persona (who can't do any network setup) points the other.

## 3. Current architecture (as built today)

**Transport / encryption (the tunnel):**
- The box runs **kernel WireGuard** (`wg0`, `NET_ADMIN`) as the server. Clients run
  **userspace WireGuard** (boringtun + smoltcp netstack, fully in-process, no root,
  no system VPN slot) via a shared Rust crate (`virtues-tunnel`), used by both
  macOS and iOS.
- Trust is **SPKI pinning over the WireGuard Noise IK handshake** — the device pins
  the box's static WG public key (SHA-256 fingerprint, `sha256-<base64>`). **No CA,
  no TLS.** First-pair is TOFU (trust-on-first-use), optionally confirmed by an
  out-of-band fingerprint in the pairing QR.
- The box serves **plain HTTP** *inside* the tunnel (WG is the confidentiality
  boundary). Browsers reach the box via a **local reverse proxy on
  `http://localhost:7117`** (a W3C Secure Context, so no TLS/cert needed), which
  forwards over the tunnel and injects the device's bearer token server-side.

**Reachability (getting bytes to the box):**
- **Doctrine: IPv6-direct.** The box gets a globally-routable IPv6 address and is
  reached directly over its WG port. The box auto-opens an IPv6 firewall pinhole
  (`ip6tables`) for the WG UDP port. **We refuse to build/ship NAT traversal,
  hole-punching, relays, or overlays as the core.** A WG UDP port on an unscannable
  IPv6 /64 is effectively invisible (silent drop to non-peers).
- **Blind (zero-knowledge) rendezvous for endpoint discovery:** because a home
  ISP's IPv6 prefix can rotate, the box encrypts its current endpoint
  (`{ip, port, wg_pub, ts}`) under a per-box key and PUTs the **ciphertext** to a
  capability-indexed blob store on our cloud API (`/v1/rendezvous/<publish_id>`).
  The cloud stores opaque bytes — no customer column, no key, can't decrypt, learns
  nothing. A device with the per-box key can GET + decrypt to find the box's current
  address. **This is the only piece of cloud we run for connectivity, and it's
  zero-knowledge.**
- **LAN discovery:** mDNS (`_http._tcp` with a `service=virtues` TXT record);
  box hostname `virtues.local`.
- **BYO escape hatch:** because auth is app-layer (see below), the box accepts a
  connection over *any* transport — the user *may* add Tailscale/Headscale/a VPS/
  their own domain. We never run or require it; it's an opt-in for the prosumer.

**Auth (who can talk to the box):**
- **Keystone principle: auth lives at the app layer; the network is a dumb
  transport.** The box authenticates every request from its own credentials, NOT
  from the path it arrived over. So the same bearer works over our WG, a BYO
  overlay, or plain LAN.
- Mechanisms: per-device **bearer token** (issued at pairing, HMAC-lookup, tied to a
  device row, revocable), browser **session cookie** (Secure/HttpOnly/SameSite=Lax,
  CSRF double-submit), a **loopback console** bypass (physical access = owner), and
  service-to-box **shared-secret** internal endpoints.
- **Pairing** is a device-authorization-grant shape: `virtues pair` on the box
  prints a short code (or QR); the device POSTs it to `/api/pair/consume` and gets
  back a bundle (bearer + WG params + rendezvous capability). Per-device revoke
  cascades to credentials + sessions + the WG peer.

## 4. Known pain points & gaps (from an internal code audit)

These are *our* current realities — the research should help us decide how to fix
or rethink them.

1. **IPv6 reachability is the existential question.** The whole default path assumes
   the box has a usable, reachable public IPv6 address. For the non-technical
   persona behind CGNAT / IPv4-only / firewalled residential ISPs / mobile
   networks, this often **does not hold**, and we currently have *no* zero-config
   answer for them (the only fallback is BYO, which they can't do).
2. **Rendezvous recovery is only half-wired.** The box *publishes* endpoint changes
   to the blind rendezvous, and the client has a `fetch_endpoint` function — **but
   nothing calls it.** So when an ISP rotates the IPv6 prefix, the stored endpoint
   goes stale and the tunnel silently fails until the user re-pairs. The recovery
   mechanism exists but isn't connected.
3. **Security is transport-dependent on macOS only.** iOS is WG-only and refuses
   plaintext fallback. macOS silently falls back to bearer-over-direct-HTTP (the
   paired address) when the WG handshake times out (~6s), which can mean a bearer
   sent in cleartext on a plain-HTTP LAN link. Platforms diverge.
4. **The localhost proxy has no Origin/Host validation**, so a malicious website the
   user visits could (via CSRF for writes, DNS-rebinding for reads) reach the box
   through `localhost:7117`. (App-layer bearer is injected by the proxy, so the box
   never sees an attacker-controllable credential, but it also never checks origin.)
5. **Bearer tokens are long-lived, non-rotating, no TTL.** A leaked bearer works
   until manual device revoke.
6. **Secrets at rest:** the bearer lives in the OS keychain *and* (because the
   background agent runs as a different code identity) in a `~/.virtues/bundle.json`
   plaintext file (mode 0600).
7. **No NAT traversal at all** is a deliberate stance, but it means a meaningful
   slice of users simply can't connect, with no graceful degradation.

## 5. What we're trying to decide

- Is **IPv6-direct (no relay/overlay)** a viable *default* for our non-technical
  persona in 2026, or is it betting against physics (CGNAT, IPv4-only, prefix
  rotation, mobile)? What fraction of residential/mobile users can it actually
  serve, by region?
- If we eventually need a zero-config connectivity plane for the long tail, what's
  the right shape — **self-hosted Headscale + our own DERP-style relays**, a
  **box-dials-out reverse tunnel through our zero-knowledge relay**, or something
  else — given the "no cloud data, no third-party-SaaS-core" constraints?
- How do comparable products solve "connect to a box at home from anywhere" for
  non-technical users, and what are the real costs/tradeoffs?

---

## 6. Research questions (what we want notes on)

Please research online and come back with concrete findings, citations, and
recommendations on:

**A. IPv6 & reachability reality (most important).**
1. Current (2025–2026) **residential IPv6 deployment** rates by major region/country
   and by major ISP — who gives customers globally-routable, *inbound-reachable*
   IPv6, and who firewalls or CGNATs it by default?
2. **CGNAT / IPv4 exhaustion** prevalence on residential and especially **mobile**
   networks. How often is a "home server reachable over IPv4" simply impossible?
3. **IPv6 prefix rotation** behavior on residential ISPs — how often do prefixes
   change, and what do self-hosters do about it (dynamic DNS for IPv6, etc.)?
4. Do mobile carriers (cellular) give devices working IPv6 that can reach an
   arbitrary home IPv6 host's UDP port? Any carrier-side filtering of inbound or of
   WireGuard/UDP?
5. Real-world reliability of an **auto-opened IPv6 firewall pinhole** — do consumer
   routers/ISPs block inbound UDP even on IPv6 by default?

**B. NAT traversal & connectivity models (for the long-tail plan).**
6. Compare the architectures of **Tailscale, Headscale, Nebula, ZeroTier,
   Cloudflare Tunnel, ngrok, and Tor onion services** for "reach a device behind
   NAT" — control plane, relays (DERP/TURN), encryption, identity, who-sees-what,
   and operational cost.
7. **WireGuard NAT traversal** specifically: what works (UDP hole-punching success
   rates by NAT type, STUN, the role of a coordination server) and what doesn't
   (symmetric NAT, hostile corporate/WeWork firewalls). What's the realistic
   coverage of "direct" vs "needs relay"?
8. **Box-dials-out / reverse-tunnel** patterns (SSH reverse, Cloudflare Tunnel,
   Tailscale Funnel, frp) — pros/cons for a self-hosted appliance, and how to keep a
   relay **zero-knowledge** (relay sees only ciphertext) end-to-end.
9. **Headscale** in production: maturity, scaling limits, what it takes to self-host
   the control plane + DERP relays, licensing, and gotchas for embedding into a
   commercial product serving thousands of isolated end-customers.
10. Economics: typical **relay bandwidth costs** for the "hostile-NAT tail," and how
    products price/limit it.

**C. Security & trust model best practices.**
11. **SPKI/TOFU pinning over WireGuard vs. a CA/mTLS model** for device↔server trust
    in a no-CA, self-hosted setting — best practices, key rotation, revocation, and
    how others (e.g. Tailscale's tailnet lock, Signal's safety numbers) handle
    out-of-band verification.
12. **Localhost-daemon security**: best practices for a local HTTP service that a
    browser talks to (the `localhost:7117` pattern) — defending against DNS
    rebinding and CSRF (Origin/Host allowlisting), and how tools like Jupyter,
    Syncthing, and Docker Desktop handle local auth tokens vs. "any browser just
    works."
13. **Bearer-token lifecycle** best practices for long-lived device credentials:
    rotation, short-lived access + refresh, scoping, binding to device keys, and
    leak mitigation.
14. **Secrets at rest** on macOS for a background LaunchAgent: how to avoid a
    plaintext bundle file given keychain code-identity scoping (keychain access
    groups, signed/notarized helper identity, etc.).

**D. Accessibility / onboarding for non-technical users.**
15. How do consumer "home server / NAS / smart-home hub" products (Synology,
    Home Assistant, Umbrel, Start9/embassyOS, Helm — the discontinued email
    appliance) solve **zero-config remote access** for non-technical users? What did
    they ship, what worked, what failed, what did it cost them?
16. UX patterns for **pairing** a phone/laptop to a home device with no accounts and
    no network knowledge (QR, short codes, BLE, cloud-brokered) — best practices and
    failure modes.
17. Any privacy-preserving discovery/rendezvous designs in the wild we should learn
    from (e.g. pkarr/DHT, BitTorrent DHT, Tailscale's coordination, Apple's
    HomeKit/iCloud relay) — especially **zero-knowledge** ones.

**E. Synthesis we want at the end.**
18. Given our personas and the "no cloud data / no third-party-SaaS-core /
    zero-knowledge-only infrastructure" constraints, **what would you recommend** as
    the v1 default and the v2 connectivity plane? Name the model, the tradeoffs, the
    rough cost, and what we'd have to give up.

---

*Context note for the agent:* assume we will keep WireGuard/Noise as the encryption
+ device-identity layer regardless (it's solid and shared across platforms). The
open questions are mostly about **reachability** (how bytes get to the box for users
who can't configure anything) and the **security/UX best practices** around that.
Prioritize section A.
