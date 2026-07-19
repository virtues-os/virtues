# Relay control plane — box provisioning, naming, auth

**Status:** design, not built (2026-06-30). Companion to `docs/networking-relay-tee.md`
(the architecture source of truth) and memory `project_networking_relay_tee`.

## Why this doc exists

The blind L4 relay, box relay-client, ACME, and TLS hot-swap all landed, but the
**control plane that makes them usable is missing**. Concretely, a code review
found four findings that are really one gap:

- **#1** The box presents the raw `VIRTUES_RELAY_TOKEN`, but the relay (with
  `VIRTUES_RELAY_SECRET` set — the production posture) expects
  `derive_token(SECRET, sni)`. No code mints or hands the box that derived token,
  so **secret mode rejects every box**.
- **#2** Nothing writes `VIRTUES_RELAY_ADDR/SNI/TOKEN` onto a box, so the relay
  path is **inert on a real install**.
- **#10** `box_reach_url()` advertises `https://<sni>` from an env var alone, with
  no check the box is actually registered — clients persist a **dead URL** over a
  working LAN origin.
- **#11** `ProvisionResponse` (Mac→phone hand-off) dropped `bundle` but gained no
  `box_url`, so a provisioned phone has **no way to learn where to reach the box**.

None of these can be fixed in isolation: they all need a thing that assigns the
box a name, mints its auth token, and tells clients when it's reachable. That
thing is the **relay control plane**.

## Product decisions locked (2026-06-30)

- **Relay is always-on critical path, not opt-in.** Off-LAN access is central
  across three patterns: browser dashboard from anywhere, phone ingestion
  off-LAN, and power-user/API. Implications: phone ingestion → relay
  **reliability** is paramount (a blip = silent data loss; folds into the iOS
  at-least-once/idempotent work); browser → serve **HTTP/2 on the box** (one TLS
  conn multiplexes a page into one relay work-conn); API → stable URLs (the
  deterministic SNI below).
- **Nabu-Casa model — depend on Virtues for v1.** No self-hostable relay, no
  BYO-domain in v1. A documented BYO-cert / direct-expose escape is "someday,"
  not launch. The sovereignty story for v1 IS open-source relay + blind + RAM-only
  + auditable — so that must be true and visible.
- **Any-browser-via-relay is the primary (only) client trust path.** The
  localhost-daemon / mesh-SPKI doctrine is **retired** (supersedes
  `project_localhost_daemon_trust`, `project_trust_model_mesh`). One path: public
  CA cert + boxhash domain; thin desktop/iOS HTTPS clients.
  **Consequence:** ACME issuance reliability is now **launch-critical** — a box
  without its ACME cert is unreachable (the self-signed bootstrap is only a
  cold-start stopgap). Review finding #12 (ACME gives up ~45s, retries every 12h)
  is therefore must-fix, not medium.

## Decisions locked

- **Box domain — dedicated `virtues.ch` (NOT a subdomain of `virtues.com`).**
  Boxes live on a separate registrable domain (Plex's `plex.direct` model) so
  user-controlled content is origin/cookie/DNS/reputation-isolated from the
  primary `virtues.com` app + auth + email domain. `.ch` (Switzerland, already
  owned) is chosen for the **Swiss privacy halo** — on-theme for a personal-data
  sovereignty appliance, and SWITCH is a stable/neutral registry (the ccTLD
  geopolitical caution that ruled out e.g. `.cr` does not apply). `virtues.world`
  is kept as a defensive/redirect domain. **Do not overclaim data residency:**
  `.ch` is a values/brand signal — data lives on the user's box, the relay is
  blind; it is not a "data stored in Switzerland" claim.
  **PSL — eventual, non-blocking.** Submit `virtues.ch` to the Public Suffix
  List (framed as cross-tenant isolation) to also isolate boxes *from each other*;
  the separate domain already isolates them from `virtues.com` immediately, so PSL
  propagation (6–18 mo) is a follow-up, not a launch dependency.

- **Naming — deterministic hash, optional vanity alias.** The canonical SNI is
  `<boxhash>.virtues.ch`, where `<boxhash>` is derived from the box's existing
  identity/pubkey (not random). Stable across reinstall/recovery, matching the
  "re-point key, balance preserved" recovery doctrine (`project_box_theft_model`,
  `project_credential_billing_model`); it's private-ish and appears in CT logs at
  issuance (acceptable, not a secret). An optional **vanity alias**
  (`you.virtues.ch`) may be layered on top — atlas mints a token for it and
  the cert carries both names — but the hash stays the canonical routing/recovery
  anchor (a vanity registry adds squatting/impersonation/dispute governance, so
  treat it as a paid nicety, not the primary name). BYO custom domain
  (`box.adamsdomain.com` via one-time `_acme-challenge` CNAME delegation) is the
  deferred sovereignty escape, not v1.

- **Minting authority — atlas/virtues-api (option A).** atlas already mints the
  box's `api_key` at link (`/claim`, see `virtues-core/src/virtues_api/renew.rs`)
  and virtues-api already holds the box relationship. It assigns the SNI, holds
  `RELAY_SECRET`, and computes `derive_token(RELAY_SECRET, sni)` for the box. The
  box **never holds the master secret** (so a compromised box cannot mint another
  tenant's token — preserves the #3 fail-closed fix). The **relay stays stateless
  and blind**: it holds one secret and derives the expected per-SNI token on the
  fly — no per-box table, fits the RAM-only / zero-knowledge property that is the
  privacy moat.

  Rejected alternatives: a dedicated `/relay/provision` endpoint (more auth
  surface for the same payload that can ride the link/claim channel — keep as a
  fallback if we later want relay provisioning fully decoupled from billing); and
  per-box secrets in a relay lookup (makes the relay **stateful** — needs api↔relay
  sync + restart re-hydration, sacrifices the blind/stateless property).

## Flow

```
LINK / RELAY-ENABLE
  box --(api_key)--> atlas: "give me my relay config"
  atlas: boxhash = H(box_identity_pubkey); sni = "<boxhash>.virtues.ch"
         token  = derive_token(RELAY_SECRET, sni)      # atlas + relay share RELAY_SECRET
         ensure wildcard DNS *.virtues.ch -> relay (one-time infra)
  atlas --> box: { relay_addr, sni, token }
  box: persist {relay_addr, sni, token} in box_secrets

RUNTIME
  box relay-client dials relay_addr, Register{ sni, token }
  relay: expected = derive_token(RELAY_SECRET, sni); ct_eq(token, expected) -> Registered
  box: flip an in-process REGISTERED flag (atomic) once Register succeeds

PAIRING
  box_reach_url() returns Some("https://<sni>") ONLY when REGISTERED is set
  ConsumeResponse.box_url AND ProvisionResponse.box_url both carry it
  client stores box_url as apiEndpoint; falls back to LAN origin when absent
```

## Pieces to build

1. **atlas: relay-config minting.** Endpoint (or a field on the existing
   claim/link response) that, given a valid `api_key`, returns
   `{ relay_addr, sni, token }`. Holds `RELAY_SECRET`; computes `boxhash` from the
   box identity key; idempotent (same box → same sni/token until secret rotates).

2. **box: fetch + persist.** On link and on first relay-enable, call atlas, store
   `{relay_addr, sni, token}` in `box_secrets`. `relay::maybe_spawn` reads from
   `box_secrets` (falling back to env for dev) instead of bare env. Add a
   `virtues` CLI/refresh trigger so enabling relay doesn't wait on a renew cycle.

3. **box: registration flag (#10).** The relay-client sets a shared
   `AtomicBool`/watch on `Registered` and clears it on disconnect.
   `box_reach_url()` returns the URL only when the flag is set, so clients never
   persist a URL the box isn't reachable at.

4. **api: `ProvisionResponse.box_url` (#11).** Mirror `ConsumeResponse` — add
   `box_url` to the provision path so the Mac→phone hand-off conveys reach. Remove
   the dead `bundle`/`qr_svg`-as-bundle expectation on the iOS side
   (see `apps/ios/RELAY_MIGRATION.md`).

5. **onboarding gate (#7, real half).** Keep `BoxStatus.ready` true for LAN-only
   boxes (the self-signed bootstrap always serves). Add a distinct **"remote
   access ready = relay registered"** signal to the setup state machine, derived
   from the registration flag, rather than overloading `ready`.

## Secret distribution & rotation

- `RELAY_SECRET` is an ops secret shared by atlas (minter) and the relay
  (verifier). Distribute out-of-band (deploy config), never to a box.
- **Rotation:** the relay accepts `derive_token` against **current and previous**
  secret during a grace window; atlas re-mints on the next box fetch. Boxes
  re-fetch + re-register within the window. Document the window vs. the box
  refresh cadence.

## Infra prerequisites (ops, parallel)

- **Wildcard DNS** `*.virtues.ch → relay` (failover-IP/GeoDNS per the P4
  plan; single region for v1). Dedicated zone, fully automated, isolated from the
  `virtues.com` zone.
- **ACME DNS-01**: the box runs `instant-acme`; the authority (atlas) writes the
  `_acme-challenge.<boxhash>` TXT (apex + wildcard share one RRset — already
  handled in `acme.rs`). Ownership proof = client-TLS-cert; email-token recovery
  re-points to a new client cert (see `networking-relay-tee.md`).

## Recovery (deterministic naming pays off)

Because `boxhash = H(box_identity)`, a reinstall that restores the same identity
key gets the **same SNI** back; atlas re-mints the same token; DNS + clients keep
working. A lost identity → email-token re-point issues a new boxhash (clients
re-pair). Consistent with `project_box_theft_model`.

## Revocation — bounded, stateless-preserving (resolves open-question #4)

**Problem.** The relay is stateless (derives the expected token from one secret —
no per-box table; that's the RAM-only/blind moat). So it cannot "revoke" a single
previously-valid token without *some* state or expiry. Instant revocation would
require pushing a denylist to the relay → per-box state → erodes the moat.
**Decision: bounded revocation via time-bucketed tokens** (no relay state).

**Mechanism.**
1. **Bucketed token:** `derive_token(secret, sni, bucket)` =
   `HMAC(secret, "<sni>:<bucket>")`, `bucket = floor(unix_secs / 86400)` (24h).
2. **Relay verify (stateless):** on `Register`, accept if the presented token
   matches the **current OR previous** bucket (constant-time). The ±1 window
   absorbs clock skew and day-boundary races. No stored tokens.
3. **Force periodic re-registration:** the relay drops a control connection once
   it exceeds a **max age** (~1 bucket, jittered to avoid a reconnect herd). The
   token is only checked at `Register`, so without this a long-lived connection
   would never be re-verified and revocation would never bite. Max-age is a
   per-connection local timer — still no shared state.
4. **Box refresh:** the box re-fetches its token from atlas on an interval
   `< bucket` (e.g. 12h) and presents the fresh token on each (re)connect. This
   means the relay-client must read the *current* token at connect time, not hold
   a single one for the process life — a small `token_source` seam on
   `RelayClientConfig` (tests keep the static `token`).
5. **atlas gate:** atlas mints the current-bucket token **only for an active,
   non-revoked account**. Revocation = atlas stops minting → the box's token
   expires within ≤2 buckets (≤48h) and the next forced re-registration is
   rejected. Subscription-lapse already flows through `resolve_active_customer`;
   owner-initiated "revoke" is the same gate plus a `revoked` flag.

**Latency:** ≤2 buckets (≤48h at 24h). Tunable down (smaller bucket → more
re-fetch traffic). Acceptable for v1 (lapse/cancel isn't second-critical).

**Stolen-box / replace-and-keep-account** needs a per-box **epoch** folded into
the SNI: `sni = H(account_id + ":" + epoch)`. Owner provisions a replacement →
atlas bumps the epoch → new SNI (new DNS name + cert, clients re-pair); the old
box keeps the old SNI, atlas stops minting for it, relay rejects within the
window. Matches `project_box_theft_model` (rotate creds, re-pair).

**Build order (coordinated — all-or-nothing, can't half-ship the bucketing):**
protocol `derive_token(.., bucket)` → relay accept-current-or-previous +
max-age eviction → atlas mint current bucket → box periodic refresh +
present-fresh-on-reconnect. Each with a test (bucket-boundary, ±1 acceptance,
max-age eviction, refresh-rotates-token).

## Path selection — the future tiers (LAN-direct, native P2P, QUIC) and why v1 stays relay-only

This section records a deep ideation (June 2026) so we don't re-litigate it. **The v1
decision is unchanged: ship the blind TCP relay + ACME. Everything below is v2+.**

### The governing reframe: capability vs. cost

**LAN-direct + relay together already cover 100% of *capability*** — reach at home,
reach from anywhere, on any device including a vanilla browser. Every "direct" path
beyond that (IPv6-direct, NAT hole-punching, QUIC) adds **zero new reach**. It only
changes two things: our **relay bandwidth bill** and **latency** for native-app users
away from home. So those paths are **cost/latency optimizations layered on a complete
system**, not missing capabilities — which is exactly why they can be deferred without
leaving a hole. The relay's role shifts from "the road" to "the always-there floor."

### "WAN-direct" is two different animals

- **WAN-direct via IPv6** — both ends have public routable v6, so they just connect.
  No hole-punching. Cheap. (This was the old WireGuard model.) Coverage is
  unpredictable — v4-only/v6-blocked networks (WeWork) kill it — so it can only ever
  be a *fast-path attempt that silently falls back to relay*, never relied on.
- **WAN-direct via IPv4 hole-punching** — both behind NAT, need a coordinator to
  choreograph simultaneous-open (DCUtR / ICE / iroh magicsock). Complex, **70% global
  success (libp2p, independently measured) to ~90% (Tailscale/iroh, first-party)**,
  and **structurally fails on symmetric NAT / CGNAT** — the exact networks that drove
  us to the relay. The stubborn 10–30% tail is permanent → **the relay never goes away.**

### Library landscape (2025–2026)

- **iroh 1.0** (n0-computer, shipped 2026-06-15; wire+API stability guarantee) is the
  strongest off-the-shelf match for **native** P2P: dial-by-public-key, magicsock-style
  relay→direct upgrade (~90% direct), a **self-hostable blind relay that is the same
  ciphertext-only shape we built** (but routes by node-id — see caveat), mDNS LAN-direct,
  and **official Swift bindings that run in-process on iOS with no Network Extension /
  VPN slot.** Apache-2.0 OR MIT.
- **rust-libp2p / DCUtR** — protocol-rich but **no production iOS story** (swift-libp2p
  is experimental); good reference for *how* hole-punching works, weak as our stack.
- **ICE/STUN/TURN** (pion/Go, str0m/Rust + coturn/turn-rs) — the RFC-grade alternative;
  a TURN server *is* a standardized blind relay. You build the signaling yourself.
- **Tailscale** — best *reference architecture* (magicsock / DISCO / DERP / Call-Me-Maybe);
  as a library only via Go (`tsnet`) or a system-wide Network Extension.

**Verdict:** if/when we build native direct paths, it's **iroh — or nothing**. It's the
only option satisfying Rust core + real in-process iOS + blind relay + actual stability.
Do **not** hand-build DCUtR/ICE.

**iroh caveat vs. our doctrine:** iroh's relay routes by ed25519 public key, so it learns
*"endpoint X talks to Y, N bytes"* (blind to content, **not** blinded-token anonymous).
That violates the `networking-relay-tee` unlinkability goal. **Lean: keep our blind L4
relay as the privacy-maximal floor; layer iroh only for native direct paths.** With ~90%
going direct, iroh's weaker relay metadata only ever touches bootstrap + the 10% tail.

### iOS reality (decides the shape)

In-process UDP hole-punching is **allowed and does NOT consume the VPN slot** — the slot
is only `NEPacketTunnelProvider` (system-wide VPN). Talking to the relay or a remote peer
over the public internet is **permission-prompt-free**; only **LAN-direct** trips the
one-time "Local Network" prompt. **Background P2P is unreliable on iOS regardless** →
relay + APNs-wake stays the background story no matter what (`project_apns_push_primitive`).

### iroh in the browser — does NOT change the browser story

iroh's WASM build is real but **relay-only**: *"All connections from browsers to somewhere
else need to flow via a relay server"* (browser sandbox can't send UDP → no hole-punch).
So for browsers iroh is architecturally **the same as our relay**, minus our unlinkability,
plus a protocol rewrite. **Browsers stay on our relay.** (Watch item: iroh's roadmap to add
WebTransport/WebRTC direct-from-browser — see below.)

### WebTransport + `serverCertificateHashes` — cert-trust, not reachability

A browser primitive (shipped in **all four engines incl. Safari/iOS 26.4, March 2026**)
that lets a browser trust a **self-signed** cert by **SHA-256 hash** passed out-of-band
(via pairing) — native certificate pinning, no CA. Buildable directly on the Rust
**`wtransport`** crate, **independent of iroh**. Constraints: ECDSA P-256, **cert validity
≤ 14 days** (no revocation; short life is the substitute), hash is over the **full DER
cert** (every rotation → new hash → must re-distribute to clients), runs over **HTTP/3
(QUIC/UDP)**, IP-literals allowed (no DNS needed), calling page must be a secure context.

**Crucial limit: it does ZERO NAT traversal.** It needs the box already reachable (LAN,
public v6, or port-forward). It solves **cert trust**, not **reachability**.

- **LAN-direct: genuine win.** Browser on the same network hits `https://<lan-ip>:<port>`,
  pins the box hash from pairing, gets a real padlock, **no DNS / no CA / no ACME / no
  trust-on-first-use prompt.** This **supersedes the split-horizon-DNS and mDNS+local-CA
  options** for LAN-direct (Open Q #5 / todo #13). The only costs: the 14-day rotation +
  hash-redistribution choreography, and an **iOS ≤ 26.3 fallback** (plaintext / relay).
- **Remote: blocked by reachability**, not by the API. CGNAT box stays unreachable → relay
  still required.

### Why we did NOT build "QUIC v3" (eliminate ACME everywhere) now

The dream: make the **relay a QUIC/WebTransport passthrough** + have browsers pin the box
hash even over the relay → drop ACME/public-CA for *every* path. It breaks on the
**TCP-fallback tail**:

1. **QUIC is UDP/443.** Most networks pass it (QUIC is mainstream) — including UDP-open /
   v6-blocked nets like WeWork, where a QUIC relay *would* work (v6-blocked ≠ UDP-blocked;
   these are unrelated failures). **But** a real tail of networks **deliberately block
   UDP/443** (enterprises forcing TCP so inspection middleboxes can see traffic).
2. **WebTransport has no automatic TCP fallback** — if UDP is blocked it just fails. So
   the deliberate-blocker tail needs a TCP path or they're locked out.
3. **Browsers have ZERO cert-pinning over TCP** — `serverCertificateHashes` is
   WebTransport-only. Over a TCP relay the browser uses ordinary HTTPS → must validate
   against a **public CA → needs ACME.**

So even granting UDP works on most networks, **you keep a TCP relay + ACME for the tail,
because there's no fallback and no TCP cert-pinning.** The "everything simpler" resolves to
*"most users get a CA-free QUIC path"* — **not** *"ACME goes away."* And QUIC-relay-now is a
large **parallel** build (UDP flow-relaying, QUIC SNI peek, HTTP-over-WebTransport
forwarder, 14-day hash treadmill, iOS tail) that **adds** surface rather than replacing
anything. **Mental model: TCP is the floor because it's the only thing always allowed;
QUIC/UDP is a fast-path you can offer where the network permits and that deletes nothing.**

> Open data question if we ever pursue QUIC-relay: pin down the real **UDP/443 blocking
> rate** (Google/Cloudflare QUIC-fallback stats, academic reachability studies) to size the
> TCP-fallback tail before committing to two transports.

### Phasing summary

| Tier | What | When |
|---|---|---|
| **v1** | blind **TCP** relay + **ACME** (apex) — reaches 100% of browsers on every network today | **now** |
| **v2a** | **WebTransport + hash** for **LAN-direct** (no DNS/CA; supersedes split-horizon/mDNS) | post-launch |
| **v2b** | **iroh** for **native** paths (desktop/iOS/CLI/box-to-box): dial-by-key, ~90% hole-punch, mDNS LAN; our relay stays the blind floor | when relay-bandwidth cost bites |
| **v3** | QUIC/WebTransport relay as a **CA-free fast-path** for the UDP-open majority; **TCP+ACME stays as the permanent tail fallback** (does NOT eliminate ACME) | evaluate after iOS-26.4 adoption is high |
| **never** | hand-built DCUtR/ICE; requiring an agent for *any* access (browsers must always work via relay, no install) | — |

## Open questions

1. Exact `boxhash` derivation (which key, hash, truncation length vs. collision/
   enumeration tradeoff).
2. Whether relay-config rides the existing claim/link response or a small
   dedicated authenticated call (leaning: extend claim/link to avoid new surface).
3. Refresh trigger UX — automatic on relay-enable toggle vs. `virtues` CLI verb.
4. ~~Per-box revocation under HMAC~~ — **RESOLVED** above (bounded, time-bucketed
   tokens + max-age re-registration; epoch-in-SNI for replace-and-keep-account).
5. LAN-direct naming (`192-168-1-50.<boxhash>.virtues.ch`) interplay with the wildcard
   cert and rebind filters (see `networking-relay-tee.md` LAN gotcha).
```
