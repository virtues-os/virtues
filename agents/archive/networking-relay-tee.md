# Networking — reach your box via a relay (ADR, SUPERSEDED)

> **STATUS — SUPERSEDED (2026-08-28). This ADR is history, not doctrine, and
> nothing in it should be quoted as a property of the shipped system.** It was
> the source of truth on 2026-06-29 and it lost that role when reach moved to
> iroh. The current design is [relay-control-plane.md](relay-control-plane.md);
> the accurate user-facing account is
> [the reach manual page](../docs/operate/reach.md).
>
> Two things to hold in mind while reading it:
>
> - **The mechanism is gone.** Single-hop SNI-routed L4 relay, per-box ACME, a
>   box-held TLS key, browser-anywhere access: none of that exists. Reach is an
>   iroh connection between two Ed25519 keys — LAN-direct, hole-punched, or
>   relayed as a last resort — and a browser cannot do it, having no key.
> - **The privacy hardening was never built.** RAM-only/diskless relay,
>   blinded-token (Privacy Pass) auth, unlinkability, an independent audit, a
>   warrant canary, open-sourcing the relay — every one of these is a *proposal
>   in this document*, and every one is unimplemented. The relay we run gates on
>   an atlas subscription callout keyed by EndpointId, which is the opposite of
>   unlinkable. Text below that reads as description ("the relay keeps no logs",
>   "RAM-only", "audited", "blinded") is aspiration written in the present
>   tense; treat it as such, and never copy it into user-facing or legal text.
>
> The failure analysis of the IPv6-direct/WireGuard model, below, is still
> correct and is why the ladder ends at a relay at all.

> **Original header (2026-06-29).** Supersedes
> the former `networking.md`, `jetson-wg.md`, and the WG/IPv6-direct parts of
> `byo-networking.md` (all three since deleted) — those described an
> IPv6-direct/WireGuard model that **fails on v4-only visited networks** (corporate /
> coworking / café / CGNAT). This document replaces that doctrine.

## Why the old model failed

IPv6-direct healed the **server** side (the box gets a routable address, no
port-forward, no hole-punch) but did **nothing** for the **client's visited
network**, which we don't control and which changes every time the user walks
into a building. v4-only networks (WeWork, most corporate/coworking wifi, CGNAT)
have **no IPv6 route at all** → the box is unreachable, no matter how elegant the
box-side v6. "Reach it from anywhere" was never true. (Confirmed live: WeWork
returns `No route to host` on the box's v6; only outbound TCP/443 survived.)

**The trilemma — pick two:** {works on any visited network} + {works from any
home incl. CGNAT} + {nobody in the path}. We **drop "nobody in the path"** —
making it a *default with escape hatches*, not a hard rule — to keep
reach-anywhere and grandma-reachability.

## The decision: single-hop BLIND relay

The box **dials out** to a Virtues-operated **relay** (outbound TCP/443); the
relay is reachable from any network and the box's home IP is never exposed. The
relay is **also the coordinator** (boxes register by key on connect) — there is
no separate hole-punch coordinator.

- **Any browser, anywhere, no client install.** A user types a URL and reaches
  their box.
- **The relay is pure passthrough.** End-to-end TLS, **the box holds its own cert
  key** → the relay sees only **ciphertext** and structurally cannot MITM.
- **The relay is closed.** Authenticated boxes only, and it routes a client
  *only to that user's own registered box* — never to the open internet. This
  kills the open-proxy / Tor-exit abuse class structurally.

### Rejected alternatives

- **Tor onion service:** cleanest doctrine fit (`.onion` = pubkey = native SPKI,
  no company in path, ~$0; WeWork test *passed*). Dropped for latency, throughput,
  reputation, app-only, and unknowns. Keep as a possible maximalist tier.
- **Double-blind 2-hop relay (Apple Private Relay style):** structurally blind,
  but the 2nd hop **requires client-side software** to pre-encrypt the
  destination → kills "any browser, client-needs-nothing," our top priority. Also
  still falls to global passive correlation, so it doesn't buy immunity. Possible
  power-user tier only.
- **Full cloud-TEE box** (run each user's whole server as a confidential VM):
  would erase the networking problem, but **contradicts the sovereignty thesis**
  (tenant on our hardware again), **flips capex→opex** (always-on per-user VM +
  GPU; the $20 sub can't cover it), confidential-GPU-for-AI is immature, data-at-rest
  key-sealing is hard, and **exit rights die**. **The home appliance stays the
  flagship.** Allowed only as a deferred optional *"Virtues Hosted (confidential)"*
  tier, priced well above $20, never the default.

## Privacy: content is structural; metadata is handled WITHOUT a TEE

**Content privacy is already structural — no TEE, ever.** The relay is pure
passthrough and never holds the box's key, so it *physically cannot read content*
(same as Tailscale DERP: "cannot inspect traffic"). The only open question is
**metadata** (`client IP ↔ box ↔ time`), and the research verdict (2026-06-29) is
clear: **none of the privacy leaders — Mullvad, Signal, Tailscale, Apple Private
Relay — use a TEE for this.** They use a cheaper, simpler, *provable* stack. We
adopt it, in build order:

1. **Pure passthrough, box holds cert key** → structural content privacy. (have)
2. **No-state-by-design + RAM-only/diskless relay** (Mullvad stboot model) →
   nothing durable to leak/seize/subpoena. *This replaces the TEE's "nothing to
   give retroactively" claim* — and it's proven (the 2023 Swedish police raid on
   Mullvad recovered nothing). Highest credibility-per-dollar.
3. **Open-source the relay** → anyone can read that it keeps no logs.
4. **Independent technical audit** (Cure53/Assured-class, scoped to the small
   stateless relay; re-run ≤24mo) → third-party confirmation the deployed config
   matches the no-log claim. The credibility currency the VPN industry runs on;
   affordable when scoped to one relay.
5. **Blinded-token / Privacy Pass auth (NEW — the key upgrade).** The relay
   authorizes a *paying* user **without learning which user** — collapsing the
   `account ↔ pseudonym` link **cryptographically**, not by policy. This is what
   Apple Private Relay actually uses (RSA blind signatures; RFC 9576 Privacy Pass /
   Apple Private Access Tokens). It *also is* the Sybil/quota gate (N tokens per
   period = your quota). **Design tension:** blinded auth makes per-key byte
   metering harder → meter by **debiting a prepaid token quota at issuance**, or
   accept coarse aggregate metering.
6. **Warrant canary + transparency report + payment tokenization** (Stripe sees a
   token, never the relay-facing key) → cheap legal-visibility + payment-unlinking.

This is *strictly what the leaders do*: content (structural), durable metadata
(don't have it + RAM-only + audit), *who-is-this* (blinded tokens) — leaving only
the global-passive-correlation residual that **no option, including a TEE,
defeats.**

### TEE — DEMOTED to optional, deferred (P4+, SEV-SNP only)

The TEE's *only* unique add over the stack above is **machine-attestation that the
exact running code is the published no-log code** (the one gap open-source +
reproducible builds can't close alone). It's narrow and expensive, and the
residual it'd protect (parent/host sees the 5-tuple; global passive correlation)
**survives it anyway.** So: **optional, deferred, and only worth it as SEV-SNP
whole-VM** (the VM owns the NIC — the one config where it beats the non-TEE stack).
**Nitro is not worth it** (untrusted parent owns the socket → "enclave-blind, not
Virtues-blind"). Details if ever pursued below.

---
*(Original TEE notes retained for the optional P4+ tier:)*

The relay must touch transient **routing metadata** (`client IP ↔ box ↔ time`).
Content is already safe (E2E TLS, box-held key).

**⚠️ Be precise about what the TEE actually buys.** It proves the relay
*application* does not **read or retain** the metadata. It does **not** hide IP
headers — those are on the wire and visible to the host / cloud provider / any
on-path observer (this *is* the global-passive residual we already concede). So
the honest claim is **"attested not to retain," never "structurally cannot
peek."** Two consequences:
- On **Nitro Enclaves**, the untrusted **parent** owns the public socket → it is
  an in-house on-path observer of client IPs. Harden it (RAM-only/measured), but
  it's in the residual.
- **SEV-SNP whole-VM (P4) removes the parent problem** — the VM *is* the TEE and
  owns the NIC — so it's not just cheaper egress, it's a **cleaner blindness
  model**. (Still doesn't hide IP headers from the cloud/network edge.)

- **TEE** (AWS **Nitro Enclaves** for v1) → memory is sealed; even root / the
  operator / a compelled employee cannot read it.
- **Remote attestation + reproducible OSS builds** → proves the *running* code ==
  the public, audited, no-log code. (OSS alone only proves what *could* run.)
- **RAM-only / swap-off / signed-boot** → nothing persists; nothing to seize.
- **Warrant canary + transparency report** → prospective compulsion becomes
  visible (tampering changes the attestation measurement).

Verification is **publicly auditable** (researchers, Signal-enclave model), not
per-browser. The app path can additionally verify attestation per-connection.

**Implementation notes (research, 2026-06-29):**
- **Nitro Enclaves have no NIC — only a vsock to the parent.** Solved by Brave's
  open-source **Nitriding** daemon (TAP interface + inbound TLS termination
  *inside* the enclave + attestation endpoint; runs in prod at ~10k tx/min) /
  `gvisor-tap-vsock`. **Use Nitriding** — don't hand-roll.
- **⚠️ Only the ENCLAVE is blind, not the parent.** Inbound is proxied through the
  untrusted parent EC2 over vsock, so the **parent transiently sees connection
  5-tuples** (client IP ↔ box ↔ time) even though it can't see content (E2E
  ciphertext). Mitigation: keep the **parent itself RAM-only/measured/minimal**,
  and **copy must say "the enclave is blind," never "Virtues is blind."** Real
  nuance — don't overclaim.
- **Reproducible builds from P1, not P3** — retrofitting determinism (toolchain
  pinning, `SOURCE_DATE_EPOCH`, no embedded timestamps) is the hard part; the
  attestation plumbing (publish source → expected PCR0/MRENCLAVE → compare live
  doc) is comparatively easy. Tie boot measurement into the attested measurement.
- **SEV-SNP (P4) attestation is a service you operate** (VCEK fetch+cache from
  AMD KDS, your own verifier validating chain + TCB + launch measurement; re-issue
  reference values + canary on every microcode/TCB bump). Budget it as "build &
  run an attestation service," not "rent a box." Use `virtee/sev`/snpguest or
  Edgeless Contrast — don't hand-roll crypto.
- **Side-channel stance confirmed:** "trust-reduction + public verifiability +
  nothing-at-rest," never "side-channel-proof." Note **`tee.fail`** (DDR5
  bus-interposer, hits SGX/TDX/SEV-SNP) needs **physical access** = exactly the
  bare-metal/Hetzner threat model → accept it in the residual.

**Honest residual (state plainly in copy):** no low-latency, any-browser system
beats a **global passive adversary** doing two-end traffic correlation (true of
Tor and double-blind too; only mixnets resist, at fatal latency cost). We sell
*"minimize what exists + provable-no-peek,"* not *"uncrackable."*

## LAN: no tunnel

WireGuard is **cut entirely** (removes the cross-platform userspace-WG
maintenance burden). On the home network the **box binds an HTTPS listener on its
LAN interface** (never the public internet) and the device connects **directly**
over LAN — no relay hop, no egress. The cert problem on LAN reuses the relay's
cert machinery via a **plex.direct-style name** (`192-168-1-50.<box>.virtues.com`
resolving to the LAN IP, valid cert). mDNS handles local discovery.

The keystone is preserved: a power user may BYO their own overlay (Tailscale/WG)
at the network layer and the box accepts it via app-layer auth — at ~zero Virtues
code. We just don't ship or maintain our own WG.

## Naming: two independent dials

"Type a URL in any browser" *requires* a name in public DNS + a public-CA cert —
there is no raw-browser path without a naming authority. So naming is **tiered**:

| Tier | Naming (phonebook) | Data path | Home IP public? | Operator-MITM? |
|---|---|---|---|---|
| **Default** `*.virtues.com` | Virtues runs DNS | relay | no | CT-detectable only |
| **BYO domain** `example.com` | **you** run DNS | relay | no | **no** (you hold DNS + cert) |
| **App + pinning** | none (opaque ID) | relay | no | **no** (pinned at pairing) |
| Direct-expose (BYO domain, no relay) | you | none | **yes** | no, but IP exposed + DDoS |

- Virtues running DNS for the default tier is the **plex.direct model** — users
  get a working cert-backed URL without owning a domain. DNS sees only `name→IP`,
  never traffic/content. Its *only* power is cert-mintability/redirect (the
  MITM-enabler), which is **CT-detectable** and closed entirely by BYO-domain or
  app-pinning.
- **A network attacker (evil wifi) cannot MITM** a raw-browser user — they can't
  obtain a CA-trusted cert for the name. The only actors who could are the
  operator (detectable) or the CA system (rare, affects all HTTPS, detectable).
- **Home IP is never public.** The relay *transiently observes* IPs (unavoidable
  for any TCP endpoint) but TEE + RAM-only mean it is **not retained or readable**.
  The only durable link held (in walled billing) is `account ↔ pseudonym`,
  necessary to bill — not IPs, not activity.

## Abuse / quota / Sybil — all blindness-safe

- **Closed relay** (boxes-only, routes only to the user's own box) → no
  open-proxy abuse.
- **Per-key byte + connection metering = volume, not content/graph** → does *not*
  break blindness (a counter, not a log). Fair-use quota → throttle or bill
  overage.
- **Payment-gates relay use = Sybil resistance** — reuse the existing OAuth-proxy
  / virtues-api / wallet credential spine. Abuser pays or is throttled.
- **Relay-side metering is authoritative.** Never trust the box's self-report
  (user-controlled, can lie). **Box self-logging = user visibility only**; a
  persistent box-vs-relay gap is itself an abuse signal.
- The relay needs its **own DDoS protection** (provider/anycast) — it is now
  critical-path.
- **Wall:** relay reports **aggregate GB-per-key** to atlas (billing); relay stays
  blind/RAM-only. Neither side holds the full surveillance picture.
- **On-box hardening is mandatory regardless** (forced auth / no default creds,
  fail2ban, per-IP rate-limit, minimal surface) — the realistic answer to
  scanning/brute-force/ransomware (the NAS-ransomware precedent), *orthogonal* to
  DDoS.
- **STEAL from TCPShield: anti-bypass forwarding secret.** The box must
  **cryptographically verify a connection came through the relay** (a signed
  forwarding token, à la TCPShield's `only-allow-proxy-connections` / ProxyProtocol
  v2) and **reject any direct public connection** (LAN excepted). Closes the
  origin-IP-bypass hole: even if an attacker discovers the home IP, they can't
  connect to or DDoS the box directly — they can only reach it via the relay, which
  absorbs the flood. This is the missing piece that makes "home IP hidden" actually
  *safe* rather than just *obscure*.
- **Direct-expose tier stays expert-only.** Research verdict: a residential link
  **cannot** survive volumetric DDoS (you can't scrub a flood after it saturates
  the uplink — no on-prem "box in front of the modem" helps; scrubbing is
  physically upstream). Directly-exposed home servers are also DDoS *reflectors*
  (Plex PMSSDP) and ransomware targets. The relay is the **only** architecture
  giving IP-hiding + flood-absorption + E2E at once — every passthrough scrubber
  (Cloudflare Spectrum, Gcore, Nabu Casa, Funnel) is the same shape. Offer
  direct-expose only to power users who BYO their own upstream scrubber.

## Build, cost & HA

- **Build, don't buy.** Nothing resellable exists (Tailscale Funnel is SaaS-only/no
  OEM; Cloudflare's cheap tiers terminate TLS = break E2E; no white-label B2C
  E2E-relay market). **But split the relay into two concerns — only one is novel:**
  - **Data plane = SNI routing + TLS passthrough.** A *solved commodity* —
    **HAProxy `ssl_preread`** / **Envoy `tls_inspector`** / nginx `stream` do this
    at scale. Never build this; never bet the core on a tunnel project for it.
  - **Control plane = the reverse tunnel** (box dials out, authenticates, registers
    pseudonym, holds connection open; relay maps `SNI → that box's live conn`).
    This is the *only* custom part, and it's small (~hundreds of lines): per-box key
    auth, anti-bypass token, blinded tokens, metering — exactly the "dated" modules
    frp does poorly.
  - **Rust-native (we're a Rust shop — prefer this over any Go fork).** The
    passthrough data plane is small enough to OWN: **tokio + `rustls` `Acceptor`**
    (peek the ClientHello SNI *without* terminating) + TCP splice + the dial-out
    registry ≈ a few hundred lines. For the production foundation build on
    **Cloudflare `pingora`** (Apache-2.0 Rust proxy framework, 40M req/s, 1T req/day
    — the "Rust Envoy," a *library* you own the logic on; ISRG/Let's-Encrypt's
    **River** is a pingora-based proxy). `rathole` (Rust) is a port-forwarder not a
    multi-tenant SNI router; `sozu` (Rust) is an option. A Go fork (**gost** >
    `frp`, which is **solo/v2-stalled, dated auth/cert**) only for a throwaway demo
    faster than writing Rust.
  - **Net:** P1 = a small **Rust** relay (tokio+rustls SNI-peek + splice + dial-out
    registry) we own; scale on **pingora**. The core rests on Cloudflare-grade Rust,
    not a solo project — and it's in our language. (This is what frp's stalled v2
    aspires to be — an Envoy-like core — except pingora already is it.)
- **Host on cheap bare-metal from the start (Hetzner/OVH), not AWS.** With the TEE
  deferred, the AWS-Nitro rationale is gone. Egress dominates cost and it's
  **AWS-specific**: ~$3.60/user/mo on AWS ≈ **~$0.05 on Hetzner overage, ~$0 on OVH
  unmetered** — a **50–90× gap**. A ciphertext-passthrough relay is pure low-CPU
  bandwidth = the ideal bare-metal workload. Real costs = fixed servers/IPs + DDoS
  mitigation + abuse handling, **not** per-GB egress.
  - **Lean OVH for the relay specifically: included DDoS protection.** TCPShield
    (Minecraft DDoS proxy, 16 Tbps L4) rides OVH's DDoS scrubbing rather than
    building its own — the cheap path to flood absorption. OVH's bundled protection
    is a real edge over Hetzner *for the relay node* (the DDoS-facing component).
- **Relay carries only remote traffic — LAN never touches it.**
- **HA: multi-region relay.** No WG fallback needed — a total relay outage still
  leaves **LAN access working**.
- **Reference implementation to study: Home Assistant's Nabu Casa Cloud** — the
  exact model (box owns the cert, relay can't decrypt, dial-out, no IP exposure),
  in production for non-technical users *today*. Proof the architecture works.
- **Prior-art validation: Olares** ($45M-funded sovereign-cloud OS) **independently
  converged on our exact design** — dial-out reverse proxy, **managed multi-region
  relay (default) + self-hosted FRP (escape)**, box-holds-its-own-cert TLS
  passthrough, plex.direct-style `*.olares.com` default domain, Tailscale/Headscale
  for the app path. Strong confirmation we're not missing a better path. **But:**
  (a) custom *source-available license forbids commercial reuse* → nothing to fork;
  (b) it's the same building blocks we vetted (FRP/Tailscale/CF/Headscale),
  K8s-entangled; (c) **they have NO hardened relay-privacy posture** — no no-logs
  claim, no RAM-only, no blinded tokens, no audit, no canary; their blindness is
  *incidental to FRP passthrough*, not a marketed/audited guarantee. **→ Our
  privacy hardening (RAM-only + blinded-token + audit + anti-bypass) is the genuine
  differentiation.** Worth borrowing: their **swappable-reverse-proxy UX** (managed
  default / BYO-FRP / per-node geo-selection) — good framing for our managed-vs-BYO
  relay + the multi-region routing in open-question #7.
- **Landscape verdict (full survey, 2026-06-29): the niche is UNOCCUPIED.** No one
  ships a *managed + blind + blinded-token + RAM-only* relay for non-technical CGNAT
  users. The field either wraps Tor/Tailscale/ZeroTier/Nebula (Umbrel, CasaOS/ZimaOS,
  Cosmos), has *no relay at all* (YunoHost, Cloudron, Sandstorm, CapRover —
  port-forward + DNS), or uses **Cloudflare Tunnel which decrypts plaintext** (runtipi,
  Coolify — the anti-pattern we market against). **Our moat — blinded-token auth +
  RAM-only blind relay — is claimed by literally no one.** Build as planned; the data
  plane is a commodity to **fork/study, not reinvent:**
  - **Sandstorm `sandcats`** (Apache-2.0) — direct prior art for our cert/naming
    (open-q #1): free per-box wildcard subdomain + cert via **DNS-01 delegation where
    the authority only writes the TXT and the box runs ACME (key never leaves the
    box)** — *exactly* our model, independently validated.
  - **Nabu Casa `SniTun`** (GPL-3.0) — *is* our data plane: box dials out, relay reads
    only the ClientHello SNI, forwards ciphertext over an AES mux, TLS ends on the box.
    Study (GPL ⇒ don't link); its auth is account-linked Fernet (not blinded) →
    confirms **blinded-token is our novel add**, not the passthrough.
  - **Start9 `start-tunnel`** (MIT) — same blind L4 DNAT passthrough, cleanly licensed
    + readable; gap is per-VPS, not multi-tenant-managed.
  - **Watch: Pangolin** (modern WG dial-out + P2P→relay fallback) — but L7/identity-
    first and **ACMEs centrally + pushes the key to the connector** (operator touches
    key = weaker than box-holds-key). **Market proof: Pluggie** (closed) already sells
    a blind relay to non-technical HA users — but no blinded-auth, no RAM-only, so our
    story is strictly stronger.

## Jurisdiction (parked)

A separate Virtues entity / nonprofit running *only* the relay in CH/IS could
raise the legal bar. But **architecture beats jurisdiction** (TEE + RAM-only =
nothing to give retroactively), and global passive correlation defeats both.
Addable later without re-architecting. Don't rabbit-hole now.

## Roadmap

- **P0 — Lock & document (this doc).** Resolve open questions below; audit
  WG-removal blast radius.
- **P1 — Reach MVP.** **Fork `frp`** as an SNI-passthrough relay on a **cheap
  bare-metal/VPS host (not AWS)**; box dials out, registers by key; browser →
  pseudonym → relay → box, E2E TLS, box holds cert; auth via existing account
  credential. **Milestone: reach your box from WeWork in a browser.**
- **P2 — Metering + quotas + visibility.** Per-key counters → atlas; quota
  enforcement on the wallet; box-side self-log into the System page.
- **P3 — Harden privacy (no TEE).** RAM-only/diskless relay; open-source it;
  independent audit (Cure53/Assured-class); **blinded-token (Privacy Pass) auth**;
  warrant canary + payment tokenization.
- **P4 — Scale (+ optional TEE).** Multi-region. *Optional:* SEV-SNP whole-VM for a
  machine-attested max-assurance tier — only if demanded.
- **P5 — Doctrine cleanup.** Delete WG paths; rewrite onboarding/BYO copy.

## Open questions (design tasks, not blockers except #1)

1. **Cert mechanics — RESOLVED (2026-06-29).** Per-box cert, **box generates &
   holds its own key**; issued via **DNS-01** (adopt LE's new **DNS-PERSIST-01**
   standing-record flow; scoped/delegated challenge writes so a compromised box
   can't rewrite the zone — Virtues only ever touches DNS, never the key). Use a
   **per-box wildcard `*.<boxhash>.virtues.com`** so one cert also covers the LAN
   `<dashed-ip>.<boxhash>.virtues.com` name without re-issue (key still only
   authenticates that box). A global `*.virtues.com` wildcard is disqualified (one
   shared key = MITM).
   **⚠️ CA is a launch-gating decision, NOT Let's Encrypt:** LE caps **50 new
   certs / registered-domain (`virtues.com`) / 7 days** — every `<boxhash>` shares
   that bucket → **~50 new boxes/week, hard ceiling.** This is exactly why **Plex
   uses DigiCert, not LE.** → **Secure a commercial high-volume ACME CA
   (DigiCert/Sectigo-class) before onboarding scales past ~50 boxes/wk;** LE (with
   a rate-limit-override application) + Google Trust Services as multi-CA
   bridge/fallback for P1. Real cost line item.
   **Hardening:** CAA on `virtues.com` with `accounturi=` account-binding (RFC
   8657) + `issuewild`, **plus live CT-log monitoring** wired as a real alert —
   this is what makes the "operator-MITM is detectable" claim operational, not
   theoretical.
   **LAN caveat:** public-name→RFC1918 resolution is a DNS-rebinding shape; some
   routers/resolvers strip it → keep mDNS `.local` / relay as fallback for those
   LANs.
2. **Heavy-media egress — mostly RESOLVED by host choice.** The ~$3.60/user
   figure is **AWS-specific**; on Hetzner/OVH bare-metal it's ~$0.05–$0/user, so
   relaying bulk media is affordable and a direct-upgrade path becomes an
   *optimization, not a necessity*. Keep an overage SKU as a backstop for extreme
   users.
3. **Quota numbers + overage-vs-throttle policy**, wired to the economic model.
4. **iOS-as-client over the relay — RESOLVED.** Three modes, all over the relay, WG
   never needed: **(a) background sync/push** = location-services cron wake (5/15 min)
   → brief HTTPS upload → sleep; **(b) foreground interactive (chatbox, pulls,
   queries)** = full network access while app is open → relay → box, request/response
   or **WebSocket/SSE streaming** (works because the relay is L4 passthrough =
   protocol-agnostic); **(c) box-initiated notify (optional, future)** = APNs wake →
   app foregrounds → pulls via relay. APNs only for (c); never for push or chat.
5. **Any-browser MITM stance:** ship "CT-detectable trust in our DNS" as the
   browser-path default, or actively nudge privacy users to BYO-domain / app?
6. **WG-removal blast radius:** `up`, `net_check`, `RemoteAccessExplainer`,
   onboarding/BYO copy, `virtues-tunnel` / `virtues-wg` deletion.

**Bigger architectural pieces newly surfaced (think about these next):**

7. **Distributed registry + multi-region routing (the real "coordinator").** The
   relay is a *fleet*. Need a consistent map of "box K's live tunnel is held by node
   N in region R" + a way to route a client (who hit some anycast node) to the node
   actually holding box K's connection (cross-node forward, or DNS-steer the client
   to the box's region). This is the meatiest under-designed part — the genuine
   coordinator we hand-waved.
8. **Connection liveness / reconnect / failover.** Persistent outbound tunnels break
   (network blips, box reboots, relay node deploys). Need keepalives, fast reconnect,
   box re-registration on failover, graceful node draining. Reliability hinges on it.
9. **Pairing + cert-provisioning + onboarding/recovery flow in the new model.** How a
   fresh box gets its cert (DNS-01), registers with the relay, and a device pairs;
   and box-replacement (new box → new cert, re-register, re-pair). Reconcile with
   existing onboarding/recovery docs.
10. **Relay observability without breaking blindness.** Monitor fleet health, per-box
    connection status, capacity — *without* logging the metadata we promised not to
    keep. Aggregate/ephemeral signals only.

## WG-removal map (audit, 2026-06-29)

**DELETE entirely:** `crates/virtues-wg/` (kernel WG engine + `virtues-wireguard`
daemon + ip6tables pinhole), `crates/virtues-tunnel/` (iOS userspace WG +
xcframework), `virtues-core/src/wireguard/` (bundle assembly/SPKI),
`apps/desktop/src/tunnel.rs` (utun/gotatun VPN), the `virtues-wireguard.service`
unit; drop both crates from workspace `Cargo.toml` members + `virtues-core`/
`apps/desktop` deps.

**CHANGE (pivot to relay):** `api/pair.rs` (`PairConsumeResponse.bundle` → relay
cert/endpoint, not WG bundle), `net_check.rs` (keep classification as
*informational only*; delete `verify_inbound()`/`/v1/net/probe` + port-51820
logic), `box_status.rs` (drop `wg_*`/`spki_fingerprint` identity + the `NatNoIpv6`
"not available" verdict → remote access always available), `devices.rs`/
`credentials.rs` (drop `NOTIFY wg_reconcile` peer eviction), `deploy.rs` bringup
(drop `ensure_server_keypair`), installer (drop wireguard pkgs + kernel-module
load), `RemoteAccessExplainer.svelte` (kill "network unsuitable → BYO"; reframe
overlays as optional LAN perf), docs (`networking.md`, `jetson-wg.md`,
`byo-networking.md`, `TUNNEL_INTEGRATION.md`, `deployment.md` privilege-split).

**DEPRECATE (keep for back-comat):** `virtues-protocol` `bundle.rs`/`spki.rs`;
`box_secrets.wg_server_keypair`; migrations `0013_raw_wg_device_kind` /
`credentials.metadata.wg` — null-safe, remove in a later major.

**⚠️ Critical load-bearing risks to handle during removal:**
1. **`wg_server_keypair` gates the "ready" state machine** — replace with a
   TLS-cert-presence / `identity_ready` flag or every setup-progress check
   misfires.
2. **SPKI pinning anchor goes away** — ensure all clients can fall back to CA
   trust (or relay attestation pinning) before deleting `spki.rs`.
3. `store_peer` / peer persistence — confirm only `pairing.rs` calls it.
4. `NOTIFY wg_reconcile` removal is safe (no kernel peers to drop) — just delete.
5. Migrations are forward-only — keep WG columns/constraints ~2-3 releases.

## See also

- [auth-model.md](auth-model.md) — pairing + device-list + bearer (the keystone).
- Memory: `project_networking_relay_tee` (supersedes `project_networking_doctrine`).
