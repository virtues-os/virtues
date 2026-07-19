# Auth Model

> Pair-only auth: no passwords, no email, no magic links. Every client that
> talks to the box is a **device**; the device list is the auth surface.

## TL;DR

1. To get in for the first time, run `virtues link` on the box and open the printed URL.
2. To add another device, open `Settings → Devices → Add device` from an already-paired session and scan/open the URL on the new device.
3. To revoke access, delete the device from the same page. The credential is invalidated and (if applicable) the WireGuard peer is evicted in the same transaction.
4. Sensitive actions (export-all, BYO-key swap, wipe, last-device revoke) require an additional `virtues sudo` confirmation from the box CLI.

There is no login form, no password to brute-force, no email to phish, no
magic-link URL to leak. The only way into the box is to (a) prove physical
access to it (`virtues link`) or (b) hold a credential issued by something
that already has access.

## Why pair-only

Virtues stores concentrated personal data (financial, health, comms). Email/
password auth assumes a recoverable account boundary — "I forgot my password,
reset to my email." That assumption costs you: every password is now a leak
risk, every magic-link URL is a single-use bearer token in flight, the email
provider becomes a soft factor in the trust chain, and the login page itself
is a continuously-present surface to brute-force.

A single-tenant appliance has a stronger primitive available: **physical
proximity to the box**. We treat that as the root of trust. Everything else
descends from it via pair tokens. This is the same primitive Plex, Tailscale,
Apple Continuity, and most modern consumer infra use; it's just rare in
self-hosted because the legacy SaaS auth model leaked into self-hosted
templates.

## The money plane: how the box pays for AI

Pair-only governs getting *into* the box. A separate, equally-deliberate token
model governs how the box *pays for AI* — across three services: the **box**
(consumer), **atlas** (the money/identity plane: Stripe, accounts), and
**virtues-api** (the metered-usage plane: the AI wallet). The whole design
exists to keep one promise: the cloud can never tie your spend to your identity.

Three properties make it elegant:

1. **The two tokens never cross.** The `billing_token` (stable "I'm a paying
   customer," box↔atlas) and the `bearer` (ephemeral monthly, box↔api, funded by
   a voucher) live in separate planes. atlas never sees your bearer; virtues-api
   never sees your billing_token *or your identity*.

2. **The voucher is a privacy seam, not just a credential.** The only thing
   crossing atlas→api is a voucher's *value* — amount, hash, expiry. No customer,
   no Stripe ID, no email. virtues-api literally cannot tie AI spend to a paying
   identity. That's the "make the server layer extinct" doctrine realized in the
   money plane — a real architectural property, not marketing.

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

Note the deliberate asymmetry with the box's own auth: device access is
**pair-only** (no email, no magic links — proximity is the root of trust),
because the box holds your data. The *account* identity at atlas — a billing
relationship, not a data boundary — uses an email magic-link, because that's the
right primitive for "prove you're the person paying Stripe." Two planes, two
roots of trust, chosen on purpose.

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
| `app_sudo_request` | Pending confirmations for the four gated actions. 5-min TTL. State: `pending → approved → consumed | expired | denied`. Approved by `virtues sudo` (v1) or push-confirm (v1.1). `consumed` is the terminal "approval was actually used" state — distinguished from `expired` so the audit log can tell them apart. |
| `app_auth_event` | Append-only audit log of pair/revoke/session/sudo events with IP + UA. Surfaced at `/virtues/activity`. |

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

The Confirm step exists specifically to defeat shoulder-surf-the-QR. The QR
alone is inert until the minting device explicitly authorizes it.

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
  prevention). User must `virtues sudo` to confirm the last-device revoke.
```

### Sudo (4 gated actions)

```
[browser]
1. Click "Export all data" (or BYO-key swap / wipe / last-device revoke).
2. Frontend POSTs /api/sudo/request {action: "export_data"}.
   Server inserts app_sudo_request row (status 'pending', 5-min TTL).
3. Modal: "Run `virtues sudo` on the box to confirm."
4. Frontend polls /api/sudo/status/:id.

[box]
5. $ sudo -u virtues virtues sudo
   → prints pending request (action, requesting device label, IP, expiry)
   → prompts y/N. On 'y': status → 'approved', approved_by = 'cli'.

[browser]
6. Polling sees 'approved'. Frontend reissues the gated action with the
   request id in the body or header. Server `verify_and_consume`s it
   (single-use) and proceeds. Request row flips to 'expired' (consumed).
```

v1 uses CLI as the proof mechanism. v1.1 will add a push-confirm channel to
the iOS app; the rest of the state machine doesn't change.

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
| Stolen device — read access | Inherent to "logged in." Mitigated by: 8h idle timeout, activity log surfaced at `/virtues/activity`, easy revoke from any other device. |
| Stolen device — irreversible actions | Sudo gate on `export_data`, `change_byo_key`, `wipe_box`, `revoke_last_device`. A thief without physical access to the box can't approve. |
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

- **Push-confirm sudo** via the iOS app (replaces CLI as the default proof).
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
- **Sudo `consumed` terminal state** — `verify_and_consume` flips an approved
  sudo request to `consumed` (not `expired`) so the audit log distinguishes
  "approval was used to perform the action" from "approval timed out."
- **iOS unified pair-only auth** — the legacy `/api/pairing/initiate` and
  `/api/pairing/complete/:id` endpoints were removed. iOS now pairs via
  `/api/pair/consume` with `kind = "mobile_app"`, gets a server-issued
  bearer (stored in Keychain via the new `KeychainStore`), and the
  endpoint also returns the action-id fan-out + (when a `wg_public_key`
  was supplied) the WireGuard provisioning bundle.

