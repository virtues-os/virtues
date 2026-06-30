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
  (`adam.virtues.ch`) may be layered on top — atlas mints a token for it and
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

## Open questions

1. Exact `boxhash` derivation (which key, hash, truncation length vs. collision/
   enumeration tradeoff).
2. Whether relay-config rides the existing claim/link response or a small
   dedicated authenticated call (leaning: extend claim/link to avoid new surface).
3. Refresh trigger UX — automatic on relay-enable toggle vs. `virtues` CLI verb.
4. Per-box revocation under HMAC (revoking the api_key/subscription must also cut
   relay access — relay has no per-box state, so revocation likely flows through
   rotating the box's token or an atlas-side allowlist epoch; needs design).
5. LAN-direct naming (`192-168-1-50.<boxhash>.virtues.ch`) interplay with the wildcard
   cert and rebind filters (see `networking-relay-tee.md` LAN gotcha).
```
