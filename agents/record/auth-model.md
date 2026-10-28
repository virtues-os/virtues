# Auth Model

> Pair-only auth: no passwords, no email, no magic links. Every client that
> talks to the box is a **device**; the device list is the auth surface.

## TL;DR

1. To get in for the first time, run `virtues link` on the box and open the printed URL.
2. To add another device, open `Settings → Devices → Add device` from an already-paired session and scan/open the URL on the new device.
3. To revoke access, delete the device from the same page. The credential is invalidated and (if applicable) the WireGuard peer is evicted in the same transaction.
4. A few routes (revealing a password or a key, adding a device, minting a pair code, changing the AI key, importing an applet package) also need a fresh owner proof: a signature by the Virtues app with the device's own key. The `virtues sudo` confirmation that used to guard two of them was removed on 2026-10-27 (`api/owner_proof.rs`).

There is no login form, no password to brute-force, no email to phish, no
magic-link URL to leak. The only way into the box is to (a) prove physical
access to it (`virtues link`) or (b) hold a credential issued by something
that already has access.

## Why pair-only

Virtues stores concentrated personal data (financial, health, comms). Email/
password auth assumes a recoverable account boundary — "I forgot my password,
reset to my email." That assumption has costs: every password is now a leak
risk, every magic-link URL is a single-use bearer token in flight, the email
provider becomes a soft factor in the trust chain, and the login page itself
is a continuously-present surface to brute-force.

A single-tenant appliance has a stronger primitive available: **physical
proximity to the box**. We treat that as the root of trust. Everything else
descends from it via pair tokens. This is the same primitive Plex, Tailscale,
Apple Continuity, and most modern consumer infra use; it is rare in
self-hosted software because self-hosted templates copied the SaaS auth
model.

## The money plane: how the box pays for AI

Pair-only governs getting *into* the box. A separate token
model governs how the box *pays for AI* — across three services: the **box**
(consumer), **atlas** (the money/identity plane: Stripe, accounts), and
**virtues-api** (the metered-usage plane: the AI wallet). The design goal
is that the cloud can never tie your spend to your identity.

Three properties of the design achieve this:

1. **The two tokens never cross.** The `billing_token` (stable "I'm a paying
   customer," box↔atlas) and the `bearer` (ephemeral monthly, box↔api, funded by
   a voucher) live in separate planes. atlas never sees your bearer; virtues-api
   never sees your billing_token *or your identity*.

2. **The voucher also separates spend from identity.** The only thing
   crossing atlas→api is a voucher's *value* — amount, hash, expiry. No customer,
   no Stripe ID, no email. virtues-api literally cannot tie AI spend to a paying
   identity. This applies the "make the server layer extinct" principle to the
   money plane, and the architecture enforces it.

3. **Everything is hashed at rest, claimed atomically.** `billing_token`,
   `bearer`, the magic-link token, `device_code` — all SHA-256 in the DB, raw
   only in transit. Every single-use claim (voucher redeem, device-link,
   magic-link verify) is an atomic `UPDATE … WHERE … RETURNING`, so replays and
   races fail closed.

```
   box ──billing_token──► atlas        atlas ──voucher value──► virtues-api
   (consumer)            (money)       (amount · hash · expiry — nothing else)
   box ─────────bearer───────────────────────────────────────► virtues-api
                                                                (metered AI)
```

Note the asymmetry with the box's own auth: device access is
**pair-only** (no email, no magic links — proximity is the root of trust),
because the box holds your data. The *account* identity at atlas — a billing
relationship, not a data boundary — uses an email magic-link, because that's the
right primitive for "prove you're the person paying Stripe." The two planes
use different roots of trust by design.

## Schema (4 + 3 tables)

Four tables hold authority:

| Table | What it holds |
|---|---|
| `app_auth_user` | Singleton owner in v1 (`is_owner = true`). Multi-user `user_id` seam present but UI not yet wired. |
| `app_device` | The canonical paired-device record. Every client (browser, mobile app, sensor, CLI) is one row. Soft-revoked via `revoked_at`. |
| `app_auth_session` | Browser cookies. FKs to `app_device`. Has `last_used_at` for idle timeout (8h). |
| `credentials` | App/sensor bearers + WG peers. FKs to `app_device` via `device_id` (nullable; OAuth source credentials don't have a device). |

Three tables support the auth machinery:

| Table | What it holds |
|---|---|
| `app_pair_token` | RFC-8628-shape bootstrap tokens. SHA-256 hash persisted, raw token only exists in the response/QR. State machine: `pending → authorized → consumed | expired | denied`. |
| `app_auth_event` | Append-only audit log of pair/revoke/session/key events with IP + UA. Written on every event and served by `GET /api/audit/auth`; **no UI surfaces it** since the Activity page was removed. |

## Flows

### First device (fresh box)

```
[box]
$ sudo -u virtues virtues link
  → mints a pair_token with status = 'authorized' (CLI = physical proof)
  → prints http://localhost:8000/pair#t=<24B hex>   (or http://localhost:5173/pair#t=… in dev)

[laptop browser]
1. Open the URL — the `t=…` is in the URL fragment, so it never hits server
   logs or referer headers. JS reads it, POSTs /api/pair/consume.
2. Server creates app_device + app_auth_session, returns redirect + Set-Cookie.
3. Browser lands at /onboarding (or / if already onboarded).
```

### Add a device from a paired session

```
[Mac browser, paired]
1. Settings → Devices → Add device.
2. Frontend POSTs /api/pair/mint → token (status 'pending').
3. Modal renders a QR + the pair URL + a Confirm/Cancel.
4. User clicks Confirm. POST /api/pair/confirm/:id flips token to 'authorized'.

[iPhone]
5. Scan QR → /pair#t=… in mobile Safari.
6. POST /api/pair/consume → cookie set, browser lands at /.

[Mac browser]
7. Polling /api/pair/status/:id sees 'consumed', shows "iPhone · Safari paired", closes.
```

The Confirm step stops someone who sees the QR over your shoulder from using
it. The QR alone does nothing until the minting device explicitly authorizes it.

### Revoke a device

```
[any paired session]
DELETE /api/devices/:id
  → in one transaction:
      UPDATE app_device   SET revoked_at = now()
      UPDATE credentials  SET status = 'revoked', secret_lookup_hash = NULL
      DELETE FROM app_auth_session WHERE device_id = $1
  → after commit, evict any WG peer attached to that credential:
      wg set wg0 peer <pubkey> remove
  → append `revoked` to app_auth_event

  Guard: refuses if this would leave zero active devices (lockout
  prevention). Pair another device first, or start over from Devices.
```

### Owner proof

```
[app, on a paired device]
1. POST /api/owner-proof/nonce → a random nonce, single use, 60 s.
2. The reach plugin signs "virtues/http/owner-proof/v1\n" + method + "\n" +
   path + "\n" + nonce with the device's iroh key, in Rust.
3. The request carries X-Virtues-Owner-Proof: <nonce>.<signature>.

[box]
4. The route's extractor (`OwnerProven`) takes the nonce, checks it was
   issued to this device, and verifies the signature against the device's
   stored endpoint id. Console requests are exempt; agent devices never pass.
```

It exists because a paired Mac's loopback port reaches the box as that Mac
for any local process, and only the app holds the key
([secrets-audit-2026-10-27.md](secrets-audit-2026-10-27.md), section 2).
The web terminal and sudo-mode chat do not ask for it yet; see the Defenses
table below.

Each reveal, enrollment, pair code and AI key change also writes a row to
`app_auth_event` (`api::audit::record`), readable at `GET /api/audit/auth`.

## Idle timeout

Beyond the 30-day hard expiry on every session, the middleware enforces an
**8-hour idle ceiling** keyed off `app_auth_session.last_used_at`. Every
authenticated request bumps the timestamp. A tab left open overnight is
silently invalid the next morning; the user re-pairs. The `peek_session`
helper (used by `/auth/session` and `/auth/signout`) explicitly does NOT
bump the timestamp, so a polling check doesn't keep an idle session alive.

## Defenses

| Concern | How it's handled |
|---|---|
| Brute-force the login | No login surface exists. `/pair` w/o a token shows static copy and accepts no credentials. |
| Phished pair URL | URL only valid on the LAN (TLS pinned to the box CA) and revoked after one use within ~5–15 min. |
| Token leakage via referer | Pair token is in the URL fragment (`#t=…`), not the query (`?t=…`). Fragments are not sent by browsers to servers. |
| Token leakage via server logs | Raw tokens are never logged. Only SHA-256(token) is persisted in `app_pair_token`. |
| Shoulder-surfing the QR | Web-minted tokens start `pending` and require the minting device to explicitly confirm before they become `authorized`. |
| Network died mid-consume | The consume path uses a single transaction; partial failure leaves the token unconsumed and retryable until TTL. |
| Stolen device — read access | Unavoidable for any logged-in device. Mitigated by: 8h idle timeout, easy revoke from any other device (Settings → Devices). The auth-event log is recorded and readable at `GET /api/audit/auth`, but has no UI — so noticing an unfamiliar pairing depends on the Devices list, not on a log anyone reads. |
| Another process on a paired Mac | The owner proof on reveal, enroll, pair mint, the AI key and package import, so a direct request to those routes through the app's loopback port is refused. Everything else stays open to a local process there, an accepted gap. That gap includes `/ws/terminal`, which opens a shell on the box, and a chat request in sudo mode. From either, a local process reaches the box's own console, which the proof exempts, so it can still do the six actions and read `virtues.env` until those two need the proof as well. |
| Clickjacking the Add-Device modal | `X-Frame-Options: DENY` + CSP `frame-ancestors 'none'` on every response. |
| CSRF on state-changing requests | Double-submit cookie: `virtues.csrf-token` (NOT HttpOnly) + `X-CSRF-Token` header. Client-side `hooks.client.ts` wraps `fetch` to auto-attach. Exempt: `/api/pair/consume` (anonymous, body-token = capability), `/auth/signout` (idempotent, kills only own session), `/webhook/*`, `/internal/*`, `/oauth/callback`. |
| MIME sniffing | `X-Content-Type-Options: nosniff` on every response. |
| WG peer survives revoke | Revoke handler reads the credential's `wg_public_key` from metadata before the tx, then after commit calls `virtues_wg::manager::remove_peer()` (Linux-only; no-op on the macOS dev host where there's no kernel WG anyway). |

## What pair-only DOES NOT defend against

- A trusted device that's running malicious code (compromised browser
  extension, malware on the laptop). Same as any auth model — credential
  access = full access at the credential's scope.
- A coerced approval (the box owner forced to run `virtues link`). Out of
  scope for a digital control plane.
- Disclosure of data the user already saw. Read access is unavoidable once
  authenticated; that's true of any system.

## v1.1+ roadmap

<!-- iOS migration shipped in v1; see "Shipped in v1 (already done)" below. -->
- **Per-device scopes** (a sensor that should only write health data; a
  dev browser that shouldn't be able to wipe). Stored on `app_device` and
  enforced at handler entry.
- **Multi-user UI** — the `user_id` column already exists; only the UI to
  add/manage additional users is gated.

## Shipped in v1 (already done)

- **Server-side QR rendering** — Add-Device modal renders SVG QR codes
  in-process via the `qrcode` crate. The pair URL never leaves the box's
  process boundary on its way to the user's browser. (Previous implementations
  routed through `api.qrserver.com`; that dependency was removed.)
- **iOS unified pair-only auth** — the legacy `/api/pairing/initiate` and
  `/api/pairing/complete/:id` endpoints were removed. iOS now pairs via
  `/api/pair/consume` with `kind = "mobile_app"`, gets a server-issued
  bearer (stored in Keychain via the new `KeychainStore`), and the
  endpoint also returns the action-id fan-out + (when a `wg_public_key`
  was supplied) the WireGuard provisioning bundle.

