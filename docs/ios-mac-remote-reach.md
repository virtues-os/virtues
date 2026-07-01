# iOS + Mac remote reach (#9) — remaining work, scored by owner

Making the native apps reach the box **from anywhere** via the relay, with durable
ingestion. This is the scoped remainder after the box-side Rust/verification pass.
Related: [data-durability.md](data-durability.md) (the ingestion durability project).

## ✅ Done (box-side, verified/shipped this session)

- **`box_url` plumbing** — verified: the box returns `https://<sni>` at pairing
  (consume/provision/QR) **only when relay-registered** (`box_reach_url`,
  [pair.rs](../virtues-core/src/api/pair.rs)). Clients consume it, fall back to the
  pairing origin.
- **Provision-QR payload** — verified complete: `{v, box_url, bearer,
  credential_id, device_id}` — everything the scanner needs for one-scan setup.
- **Deterministic content-id for location/HealthKit** — shipped (`stream_id_or_hash`,
  commit `33d1c07`): byte-identical retries dedupe via `ON CONFLICT` instead of
  duplicating. Box-side half of the idempotency linchpin.

## 📱 Xcode (iOS / Swift — needs a device; also the Mac app)

1. **Provision-QR scanner** — scan `{v, box_url, bearer, credential_id,
   device_id}` and configure the app in one scan (no prior box contact). Contract
   is confirmed box-side.
2. **Path-selection (LAN → relay)** — probe `virtues.local` / cached LAN IP with a
   short timeout; else use `box_url`. Wrap `BoxTransport.send` / `NetworkManager`.
   *Per-app (Swift): iOS has no Rust bridge, so it can't share the desktop helper.*
3. **Send stable per-record `id`** for location + HealthKit — generate + **persist
   in the local SQLite queue** so a retry reuses the same id. (Box now also
   content-hashes as defense-in-depth, but the app sending ids is the clean fix.)
4. **Map `413` → split batch** (not delete) — the box's status→action contract:
   `400`→delete, `409`→keep+resend; `413` must mean "too large, split." Prereq for
   the box returning 413 (Us #4).
5. **Bound upload batch by BYTES at the source** so a batch never exceeds the box
   body limit (512 MB) → no permanently-un-landable batch.
6. **HealthKit anchor after durability** — persist the HKAnchoredObjectQuery anchor
   only *after* the samples are durably enqueued (fixes upstream silent loss).
7. **Background ingestion** — APNs/PushKit-wake → dial `box_url` → upload (Track B
   of data-durability; background execution is only testable on a real device).
8. **Mac app path-selection** — same LAN→relay rule in the Mac source.

## 🛠️ Us (box-side Rust / shared / infra — no Xcode)

1. **Request-level idempotency** — a `webhook_idempotency_requests` table +
   `Idempotency-Key` dedup in [webhook.rs](../virtues-core/src/server/webhook.rs);
   pairs with iOS sending the header (Xcode side).
2. **Confirmed cursor / ack** — extend the webhook response with
   `records_ingested` + `last_record_at`; the device advances its checkpoint only
   on ack (pairs with iOS checkpoint logic).
3. **Atomic `ios_ingest`** — wrap all stream writes in one transaction
   (all-or-nothing) so a mid-batch failure can't leave partial state + retry dupes.
   [actions/ios_ingest/main.rs](../actions/ios_ingest/main.rs).
4. **`413` on oversized body** — return 413 (not 409-retry-forever) when the body
   exceeds the limit. **Ship together with Xcode #4** (iOS split handling) — 413
   without iOS support could make the app delete a too-large batch.
5. **Stale-run TTL watchdog** — reap wedged `running` action locks (today cleared
   only on box restart → one stuck run stalls all of a device's ingestion).
6. **Desktop CLI path-selection (Rust)** — the LAN→relay resolver for
   [apps/desktop](../apps/desktop) (`~50 lines`; the Rust twin of Xcode #2).
7. **Onboarding "remote access ready" signal** (#10) — surface in the setup state
   machine that remote reach is live once the box is relay-registered.

## Sequencing / coupling notes

- **Idempotency is layered:** box content-hash (done) + iOS stable ids (Xcode #3)
  + request-key (Us #1) are complementary; ship in that order of value.
- **413 is coupled:** Us #4 ⇄ Xcode #4 — ship together or not at all.
- **Cursor/ack is coupled:** Us #2 ⇄ iOS checkpoint-advance.
- **Path-selection is per-app:** Xcode #2 (Swift) + Us #6 (Rust) implement the same
  rule; no shared crate (iOS has no Rust bridge).
