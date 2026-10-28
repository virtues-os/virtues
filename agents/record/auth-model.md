# Auth Model

> Pair-only auth: no passwords, no email, no magic links. Every client that
> talks to the box is a **device** with its own iroh key, and the box's
> allowlist of device keys is the auth surface.

## TL;DR

1. Your first phone pairs during setup over Bluetooth. It sends the
   four-word phrase shown on the box's screen, and the box pairs it with no
   code to type ([build/onboarding.md](../build/onboarding.md)).
2. Any later device pairs with a six-digit code. `virtues pair` on the box
   prints one, and Settings → Devices → Add device in the app mints one. The
   new device sends the code and its iroh key to `POST /api/pair/consume`,
   and the box adds the key to its allowlist.
3. A paired laptop can also enroll a phone that cannot reach the box: it
   mints a key for the phone, enrolls it, and shows it in one QR
   (`crates/virtues-reach-client/src/handoff.rs`).
4. To revoke a device, remove it in Settings → Devices or run
   `virtues device rm <id>`. The box drops its key from the allowlist and
   refuses its next request.
5. A few routes (revealing a password or a key, adding a device, minting a
   pair code, changing the AI key, importing an applet package) also need a
   fresh owner proof: a signature by the Virtues app with the device's own
   key (`api/owner_proof.rs`).

There is no login form, account password, session cookie, bearer or CSRF
token. The ways in are physical access to the box (its screen and Bluetooth
during setup, or a shell on it for `virtues pair`) and a device that is
already paired adding another. One exception follows from the desktop app:
a process on a computer where the app is paired reaches the box as that
computer (see "The desktop app's loopback port" below).

## Why pair-only

Virtues stores concentrated personal data (financial, health, comms). Email/
password auth assumes a recoverable account boundary — "I forgot my password,
reset to my email." That assumption has costs: every password is now a leak
risk, every magic-link URL is a single-use bearer token in flight, the email
provider becomes a soft factor in the trust chain, and the login page itself
is a continuously-present surface to brute-force.

A single-tenant appliance has a stronger primitive available: **physical
proximity to the box**. We treat that as the root of trust. Everything else
descends from it via pair codes and devices that are already paired. This is
the same primitive Plex, Tailscale, Apple Continuity, and most modern consumer
infra use; it is rare in self-hosted software because self-hosted templates
copied the SaaS auth model.

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

## How a request is authenticated

`middleware/auth.rs` accepts three things, checked in order:

1. **A proven iroh key.** The box serves its API over iroh. The QUIC
   handshake proves the peer's key, and `serve()` forwards only keys on the
   allowlist, which `relay::reconcile` keeps equal to the `endpoint_id` of
   every `app_device` row with no `revoked_at`. The extractor maps the key to
   its device row and checks `revoked_at IS NULL` again on every request. An
   agent device (an MCP bridge on the owner's computer) reaches `/api/mcp`
   and no other route.
2. **The loopback console.** A process on the box that connects straight to
   `127.0.0.1` or `::1` with no forwarding header is the owner, as the
   device `local-console`. It can already read `virtues.env`, so this adds
   nothing. A request that carries `X-Forwarded-For` or `Forwarded` is
   refused here, because a reverse proxy also connects from loopback.
3. **The dev fallback.** With `ENVIRONMENT=dev`, a loopback request that
   carries a forwarding header is the owner too. An installed box does not
   set it unless its operator does. The review demo box runs this way behind
   Caddy, so whatever Caddy forwards to it acts as the owner
   ([build/review-access.md](../build/review-access.md)).

The plain HTTP listener on port 8000 never carries a proven key, so a
request there from another machine reaches only the routes that need no
device, such as `/api/pair/consume`.

Nothing else authenticates as a device. A pair code opens one pairing, a
signed OAuth state opens one callback, and a face token opens one face load;
none of them opens the API.

`middleware/security.rs` refuses, with 421, a request in those three classes
whose `Host` names something the box does not serve. That keeps a
DNS-rebinding page off them. It also sets `X-Frame-Options: DENY`, `nosniff`
and a restrictive CSP on every response, and `Cache-Control: no-store` on the
reveal and pair routes.

### The desktop app's loopback port

On a computer where the desktop app is paired, the app's reach plugin listens
on `127.0.0.1:7117` (`apps/web/plugins/reach/src/lib.rs`) and copies each
connection onto the app's own iroh connection
(`crates/virtues-reach-client/src/proxy.rs`, `serve_on_provider`). The box
sees the app's key, so any process or browser page on that computer that
opens the port is that device for every route the owner proof does not
cover. This is the gap the owner accepted on 2026-10-27
([plan/secrets-plan.md](../plan/secrets-plan.md), "Who can read what").

Because there is no CSRF token, a page on another site open in a browser on
that computer can also send a request to the port. CORS keeps it from reading
the reply; nothing on the box stops a write that needs no preflight.

## Tables

| Table | What it holds |
|---|---|
| `app_auth_user` | The owner. One row; `app_device.user_id` points at it. |
| `app_device` | One row per paired device: `kind` (`mobile_app`, `desktop_app`, `sensor`, `cli`, `agent`), its iroh `endpoint_id`, a label, `last_seen_at` and `revoked_at`. The `local-console` row stands for the box itself. |
| `app_pair_token` | Pair codes. Only SHA-256 of a code is stored, except a standing code's display copy, which is sealed with the box key so the box's own screen can show it. `kind` is `oneoff`, `standing` or `review`. `status` is `authorized`, `consumed` or `denied`. The schema also allows `pending` and `expired`, which nothing writes, and the sweeper deletes a code once it has expired. |
| `app_setup_phrase` | The four-word phrase that opens setup over Bluetooth and later serves as the recovery key. |
| `app_auth_event` | Append-only log of pairings, enrollments, revokes, pair codes, AI key changes and each password or key revealed, written through `api::audit::record` and read at `GET /api/audit/auth`. A pairing records the caller's IP and user agent; the other rows leave both empty. No page shows the log. |

A device has no row in `credentials`. That table holds what the box presents
to other services, never something a caller presents to the box.

## Flows

### First device (fresh box)

```
[phone, Bluetooth]
1. The setup device finds the box and sends ClaimSetup with the four-word
   phrase shown on the box's screen. That opens a session bound to this
   Bluetooth connection.
2. PairConsume (0x83): the box redeems its own standing code against
   /api/pair/consume over loopback, with the phone's iroh key, and streams
   the answer back. Nothing is typed.
3. The box is claimed. Every standing code expires, and the Bluetooth
   service stops.
```

Without Bluetooth, `virtues pair` on the box prints the standing code during
setup, and the app sends it to `/api/pair/consume`.

### Add a device

```
[box shell]                         [app, paired device]
virtues pair                        Settings → Devices → Add device
 → a one-time code, 30 minutes       → POST /api/pair/mint, with the owner
                                       proof: a one-time code, authorized at
                                       once, 5 minutes, shown with a QR
                                     → closing the modal sends
                                       POST /api/pair/deny/:id

[new device]
POST /api/pair/consume {token, kind, label, device_node_id}
 → app_device row with the device's endpoint_id, code marked consumed
 → relay::reconcile adds the key to the allowlist
 → a `paired` row in app_auth_event

[app]
GET /api/pair/status/:id until the code is consumed or denied, or the
route answers 404 once the code has expired and been swept.
```

`/api/pair/consume` is the one pair route that needs no device. It allows
10 attempts per 30 minutes per socket peer (an IPv6 peer is counted by its
/64), and loopback is exempt.

The handoff for a phone that cannot reach the box: the paired laptop mints a
key for the phone, sends `POST /api/devices/enroll-peer` with the owner
proof, and shows one QR holding that key and the box's answer. The QR
carries a private key, so whoever photographs it gets a device.

### Revoke a device

```
[app]  DELETE /api/devices/:id
  → one transaction: lock the device, refuse if it is the last active
    device (409, "Pair another device first."), set revoked_at and clear
    its push address
  → relay::reconcile drops the key from the allowlist
  → a `revoked` row in app_auth_event

[box shell]  virtues device rm <id>
  → set revoked_at and clear the push address, with no last-device check
  → no audit row
  → the server drops the key from the allowlist at its next reconcile,
    within 15 minutes
```

The CLI can revoke the last device (`revoke_device_cli` in
`api/devices.rs`). Its own call to `relay::after_pairing_change` runs in the
CLI process, which holds no allowlist, so the running server's allowlist
changes only on the server's 15-minute reconcile tick. Until then the
`revoked_at` check refuses every request from that device.

The device has no other credential to revoke, so clearing its key is the
whole of it. After a revoke from the app, its next connection fails at the
handshake, and a request on a connection that is already open fails the
`revoked_at` check.

`POST /api/pair/reopen-onboarding` (Devices → Start over) revokes every
device at once and keeps the data, network and account link. It needs a
paired device and no owner proof.

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

It exists because a paired computer's loopback port reaches the box as that
computer for any local process, and only the app holds the key
([secrets-audit-2026-10-27.md](secrets-audit-2026-10-27.md), section 2).
That holds on the Mac, whose app keeps the key in the Keychain, and on iOS.
On Windows and Linux it is in `box.json`, which any program running as the
same user can read, until the dated TODO in
`apps/web/plugins/reach/src/store.rs` lands. Android keeps it in the app's
private storage. On a Mac whose Keychain refused the move, the seed also
stays in `box.json`. The web terminal and a chat turn of the admin applet do
not ask for the proof yet; see the Defenses table below.

Each reveal, enrollment, pair code and AI key change also writes a row to
`app_auth_event` (`api::audit::record`), readable at `GET /api/audit/auth`.

## Defenses

| Concern | How it's handled |
|---|---|
| Brute-force a login | No login surface exists. |
| Guessing a pair code | Six digits, 10 attempts per 30 minutes per socket peer. A claimed box issues only one-time codes, which die on use, except a review box, whose `VIRTUES_REVIEW_PAIR_CODE` code is multi-use. The standing code rotates every 15 minutes, lasts 20, and every standing code expires when the box is claimed. |
| A pair code read off a screen | An app code lasts 5 minutes and a `virtues pair` code 30, and each works once. Closing the Add device modal denies the code. |
| Token leakage via server logs | Raw codes are never logged. Only SHA-256(code) is stored, and a standing code's display copy is sealed with the box key. The pair routes are `no-store`. |
| The handoff QR | It carries a private key. The laptop never writes it to disk, withdraws the QR once the phone appears, and the new device shows in Devices at once. |
| Network died mid-consume | The consume path uses a single transaction; partial failure leaves the code unconsumed and retryable until it expires. |
| Stolen device | It has full access until revoked from another device or with `virtues device rm`. The auth-event log has no page, so noticing an unfamiliar device depends on the Devices list. |
| DNS rebinding onto a trusted transport | The `Host` check refuses names the box does not serve (`middleware/security.rs`). |
| Another process on a paired computer | The owner proof on reveal, enroll, pair mint, the AI key and package import, so a direct request to those routes through the app's loopback port is refused. Everything else stays open to a local process there, an accepted gap. That gap includes `/ws/terminal`, which opens a shell on the box, and a chat turn of the admin applet (agentMode `admin`, or `sudo` from an older app). From either, a local process reaches the box's own console, which the proof exempts, so it can still do the six actions and read `virtues.env` until those two need the proof as well. |
| A web page in a browser on a paired computer | It can send a request to `127.0.0.1:7117` that needs no CORS preflight, and nothing stops the write. CORS keeps it from reading the reply. |
| Clickjacking | `X-Frame-Options: DENY` and CSP `frame-ancestors 'none'` on every response except applet faces, which the app frames. |
| MIME sniffing | `X-Content-Type-Options: nosniff` on every response. Message attachments that are not raster images, audio or video download as `application/octet-stream` with a sandbox CSP. |

## What pair-only DOES NOT defend against

- A trusted device that's running malicious code (compromised browser
  extension, malware on the laptop). Same as any auth model — credential
  access = full access at the credential's scope. On a computer with the
  desktop app, any process there counts, through the loopback port.
- A coerced owner (the box owner forced to run `virtues pair`). Out of
  scope for a digital control plane.
- Disclosure of data the user already saw. Read access is unavoidable once
  authenticated; that's true of any system.

## v1.1+ roadmap

- **Per-device scopes** (a sensor that should only write health data; a
  dev browser that shouldn't be able to wipe). Stored on `app_device` and
  enforced at handler entry. Agent devices are the one scope today.
- **Multi-user UI** — the `user_id` column already exists; only the UI to
  add/manage additional users is gated.

## Shipped in v1 (already done)

- **Server-side QR rendering** — Add-Device modal renders SVG QR codes
  in-process via the `qrcode` crate. The pair URL never leaves the box's
  process boundary on its way to the user's browser. (Previous implementations
  routed through `api.qrserver.com`; that dependency was removed.)
- **iOS pairs like every device.** It sends a code and its iroh key to
  `/api/pair/consume` with `kind = "mobile_app"` and keeps the key in the
  Keychain. The box answers with its reach details and the applet ids for
  the phone's collectors. There is no bearer.
