# Reach & enrollment design (iroh)

Follows the iroh transport pivot. The data path ([[project_iroh_pivot]]) is
unchanged; this covers **device onboarding + reach ticket lifecycle**.

## The model: peer-vouched enrollment (broker), not self-enroll

The relay gate authorizes **every** endpoint that connects to the relay
(`iroh-relay` `http_server.rs` → atlas `/relay/authorize` checks the connecting
`X-Iroh-NodeId`), and with discovery-off even LAN-direct/hole-punch needs the
relay to exchange addresses first. So an **unregistered** device can't reach the
box over the relay at all — enrollment is the act of *becoming* registered, and
must use a channel that doesn't already require registration.

**Chosen model — broker / peer-vouched:** an already-paired, already-registered
device (the Mac) vouches for the new device by its EndpointId; the box registers
it; the new device only ever connects to the relay **after** it's legitimately
registered. This never loosens the relay gate.

**Rejected:**
- **Relay "grace-pass"** for unregistered endpoints — fights the gate we built,
  weakens the anti-freeloading property, and the relay callout can't cleanly
  validate a bootstrap token. 
- **An open `virtues/pair/1` enrollment ALPN** — the gate blocks self-enroll over
  the relay anyway, so the broker is both simpler *and* strictly more capable; no
  new box-side protocol needed (just an authenticated endpoint).

Trust chain: box trusts the Mac (registered) → Mac vouches for the phone → box
enrolls + registers the phone → phone reaches the box globally. The **bearer** is
the only secret and must travel Mac→phone over a trusted channel.

## Shipped (Phase 1 + 2a)

- **`GET /api/devices/self/reach`** ([devices.rs]) — a device re-reads the box's
  *current* `{box_node_id, relay_url}` instead of freezing the ticket at pair
  time. iOS calls it best-effort on launch (`NetworkManager.refreshReach`).
- **`POST /api/devices/enroll-peer`** ([devices.rs]) — an already-paired device
  (its own bearer) vouches for a new device: inserts `app_device{node_id=peer}`,
  mints the credential, `after_pairing_change` (allowlist + atlas register),
  returns `{bearer, credential_id, action_ids, box_node_id, relay_url}`. Reuses
  the extracted `claim_pair_token` / `insert_device_row` / `insert_credential_row`
  / `build_bearer_pack` / `box_reach` helpers shared with consume.
- **Idempotency key on consume** — clients send a stable key + retry once on a
  lost response; the box replays the same bearer (`app_pair_consume_idem`,
  ciphertext-only, swept hourly) instead of burning the single-use token.
- **iOS background budget** — FFI `dial`/`request` take a `background` flag →
  shorter budgets (8s/12s) so a cold background wake bails instead of being
  force-killed; foreground keeps 20s/30s. Upload cadence 5 → **15 min**.

## Remaining (Phase 2b/2c)

- **2b — co-located rendezvous (QR both ways):** the new device shows a QR
  `{peer_node_id, ephemeral_pubkey, nonce}`; the paired device scans it, calls
  `enroll-peer`, then shows a bearer QR **encrypted to `ephemeral_pubkey`**; the
  new device scans + decrypts. Works anywhere you're near your *Mac* (not
  necessarily the box). `enroll-peer` already accepts an optional
  `ephemeral_pubkey` slot for the encrypted-bearer return (not yet wired).
- **2c — fully-remote rendezvous (atlas blind mailbox):** new device posts its
  enroll-request ciphertext under the account; the paired device polls + approves
  (`enroll-peer`); the bearer ciphertext returns via atlas; the new device pulls
  + decrypts. Bearer is E2E-encrypted device↔device so atlas stays blind. New
  atlas table + two account-authed endpoints.
- **Desktop reach-refresh** on `up` (low value; deferred).
- **1d deep hardening:** bind `set_self_node_id` to the QUIC-proven
  `conn.remote_id()` (needs iroh→axum extension plumbing). Today it's bearer-gated
  + unique-indexed, which is acceptable.

## Out of scope
- Open `virtues/pair/1` ALPN and relay grace-pass (see rejected, above).
- iroh transport-config (idle/keepalive) tuning — n0 flags it experimental; can
  harm hole-punching. Leave default.

## Grounded iroh 1.0 notes (kept for implementers)
Multi-ALPN via `Router::builder().accept(a,h1).accept(b,h2)`; `conn.remote_id()`
is the proven peer key (trust it, never a body field) — relevant if 1d/2b ever
bind to it; must `endpoint.close().await` since 0.97; 0-RTT is idempotent-only
(our uploads qualify → a future cold-redial speedup); relay URL must always
travel with the EndpointId (discovery-off). Sources in
[[project_reach_enrollment]].
