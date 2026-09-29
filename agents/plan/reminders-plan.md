# Reminders — the box's way of reaching the owner

**Status:** Phase 1 (the address) is in staging prereleases from
`v0.1.10-staging.84` (efe49f8f box, dcb8613c phone, a29d8e80 audit fixes); no
stable release carries it, and it is unverified on a device. **Nothing sends:**
`aps-environment` is still absent from `virtues_iOS.entitlements`, so
registration fails, the phone reports itself unreachable, and both screens say
so. The step is the owner's: enable Push Notifications on the App ID in the
developer portal first, *then* add the entitlement — the other order breaks
signing. Phases 2 (signer) and 3 (reminders) are not built.

An applet can be woken, can check a condition, and can write to the record.
It cannot *reach the owner*. `applets-next-plan.md` (Persona) names this precisely
when it marks Persona "v2 — not yet authorable": *no inbound wake, no channel,
no send capability — a model following the recipe today authors dead
manifests.* This plan builds the channel.

## The thesis

A connection has two halves: **a key**, which is how a thing proves it is
itself, and **an address**, which is how you reach it. The box models keys
well — `app_device.endpoint_id` is an allowlisted iroh key and
`middleware::auth` says outright that the key *is* the credential. It has no
address for anything. Every path into the box is something else dialing in.

A reminder is the first thing that needs the box to go the other way.

## What this is NOT

The owner learns one word: **reminder**. Not notification, not alert, not
channel, not push. They say a sentence in chat and it comes back later.

There is no notifications room, no category matrix, no per-source toggles. A
reminder is an applet row: it can be listed, opened, edited and deleted like
any other, and the reason it is inspectable is not a design principle — it is
that the owner wrote it.

## What is already decided, and not restated here

- **Wake and condition** — `agents/record/applets.md` (wake/gate/life). *Trigger =
  who wakes you; condition = what you check once awake.* Data triggers
  (`trigger=data:data_location` + `condition="speed < 5"`) are phase 4 there.
  This plan **depends on** that phase and adds nothing to it.
- **One-shot vs standing** — already the `until` field: absent = forever,
  `"once"` = first success, SQL bool = archive when true. Nothing new needed;
  the authoring model must simply fill it, and the UI must show what it chose.
- **Rate limiting** — `max_runs` per hour/day, already marked mandatory before
  data triggers light up composition loops.
- **Chat authoring** — `agents/record/applets.md`.
- **The iroh socket wedge** — `agents/record/reach-reliability.md` and iroh#4289, still
  open upstream as of 2026-09-22.

## Delivery

### Why APNs, and why it is not a fallback

Earlier drafting assumed most reminders could be evaluated on the phone
(`UNLocationNotificationTrigger` and calendar triggers fire with no network at
all). **That assumption is wrong for this product and the idea is dropped.**
The overwhelming majority of conditions worth reminding about need the box:
they are SQL over the record, or they need inference, or they span sources.
A phone cannot evaluate "a charge over $500 posted" or "Nick finally replied
about the lease". Keeping a second, local mechanism for the thin slice iOS can
evaluate would be two mechanisms for one concept, and the thin slice does not
earn it.

So: **the box decides, APNs carries, the phone displays.** One path.

Holding the iroh endpoint up to let the box dial the phone was also considered
and dropped. Two reasons, either sufficient. First, measured from the code:
backgrounded, the drain loop ticks at 300s, holds to `CONSTRAINED_DRAIN_SECS`
(900s) on an expensive radio, and **parks the endpoint entirely when the outbox
is empty** — so the phone is reachable roughly 3–8% of the time when it has
data to send and 0% when it does not. Those windows exist because the phone has
an errand, not because it is listening. Second, an iOS notification-service
extension is a **separate process** and cannot inherit the app's endpoint, so
holding one up would not speed the notification path by a single millisecond.

### The push carries the text

The push **carries the reminder text**; APNs allows ~4KB, which is ample. This
replaces a contentless-wake-plus-fetch design:

- No dial from the phone on the user-visible path, so no 1–5s cold start, no
  30s ceiling, and no fallback banner that says something useless.
- **It works when the box is asleep, offline, or wedged** — which is exactly
  when a fetch design fails.

**What that means for privacy, by phase.** Phase 2 sends the text in
**plaintext**: Apple and our signer can read every reminder in transit, and the
manual must say so in those words. Encryption to a key only the device holds
arrives in v2 with the notification-service extension (see Phases); only then
does the claim become *"content is encrypted, so transit does not matter"* — a
property rather than a promise. Until v2 ships, no surface may claim the
payload is private.

Budget the payload well under 4KB: AEAD overhead plus base64 expands it in v2,
and `413 PayloadTooLarge` is a silent failure for the owner. Truncate the text
**before** encrypting, never after.

### Who signs

An APNs JWT must be signed by a key belonging to the Apple team that publishes
the app. That is ours, because we publish it. The capability is centralized by
Apple's design and cannot be delegated per-owner: a p8 cannot be scoped to a
customer or a device, so shipping one to every box would hand every owner a
global capability. Three tiers, one code path:

1. Box holds its own APNs credential (a DIY owner with their own Apple team and
   their own build) → signs locally, never touches our cloud.
2. Box has an account → our signer relays.
3. Neither → no push. Reminders wait in the record and the app finds them.

`VIRTUES_PUSH_SIGNER_URL` plus the local-p8 path is what makes tier 1 real
rather than rhetorical, and it is the difference between a default and a
dependency.

**What the signer sees, stated plainly.** Blind tickets were considered and
refused (see Refuted), so the signer is not unable to correlate — it is built to
keep nothing. Per request it sees the calling box's `api_key` (so, the account),
the device's APNs token, the request's source IP, and in phase 2 the reminder
text. It calls APNs, hands the status straight back to the box, and stores
nothing: feedback is **request/response, not callback**, so no correlation is
needed at any point. A box that wants none of this takes tier 1 and signs
locally.

### Time-sensitive

Time Sensitive is a self-serve capability, not one of the request-a-form
entitlements (that is Critical Alerts, a much harder ask). It is policed at App
Review by usage. The justification is the canonical one: a reminder the owner
asked for, in their own words.

**Per reminder, never global.** If everything is time-sensitive the owner's
notification summary stops working and they turn the app off, which is worse
than a reminder landing an hour late. The authoring model proposes it; the
owner overrides it.

## Schema (built in phase 1)

Push authorization is not a second field: the phone reports an explicit `null`
address when notifications are off, so "can the box reach this phone" has one
source of truth.

| Column | Why |
|---|---|
| `app_device.push_address` | The address half of the pair, beside `endpoint_id`, on the row whose revocation already kills reachability for free. |
| `app_device.push_address_at` | When the box last heard this address was good — bumped on every accepted report, so it means *last confirmed*, not first registration. **Required** to handle a 410 correctly — see below. |

**Never name any of these `device_token`.** That name is already taken on the
wire by the pairing bearer (`PairingCompleteResponse.device_token`), and APNs
uses the same words for a different thing. This is the collision the
`node_id`/`endpoint_id` sweep existed to prevent.

## Invariants live in the evaluator, never in a prompt

A prohibition in a system prompt is routed around; a missing slot is not. So
anything that must always hold is structural, and only interpretation is left
to the authoring model.

| Rule | Where it lives |
|---|---|
| **A reminder never fires about a row older than itself.** | Evaluator injects an `occurred_at` floor. The authored SQL never sees the choice. Connect a finance source, it backfills two years, and without this the owner gets forty notifications about 2024 — which costs their trust in every reminder afterwards, permanently. |
| **Fire once per subject, not per evaluation.** | Evaluator, keyed on reminder + subject. The model declares *what the subject is* (a transaction id, an email id, a day) because that is a real grain decision; the dedupe itself is bookkeeping. |
| **One-shot vs standing** | `until`, already in the schema. Model fills it, UI shows it, owner corrects it. |
| **Downtime collapse** | Evaluator. Box off for a day, six became true: fire the most recent, collapse the rest, say how many. Same instinct as `apns-collapse-id`. The model has no business knowing the box was off. |

## Failure is silent unless built otherwise

The worst case is not latency. Typical delivery is about a second; a summary
can hold it to the next digest (time-sensitive fixes that); an offline device
is bounded by the `apns-expiration` we choose.

**The worst case is a stale address, which fails silently and forever.** APNs
tokens rotate on restore-from-backup, reinstall, and sometimes OS updates.
Classify every APNs response by whose fault it is, and let only device-class
errors touch a device row:

- **410 Unregistered** — device. But the response carries a **timestamp** for
  when the token died: if our `push_address_at` is newer, a stale send raced a
  fresh registration and we must **not** delete it. An event alone is not the
  guard; the time floor is.
- **400 BadDeviceToken** — *configuration*, usually a sandbox/production
  mismatch. Treat it as "device gone" and every debug build unregisters a
  tester's phone.
- **403 Expired/InvalidProviderToken** — our JWT. Must never touch a device
  row, or one bad key silently unregisters the fleet.
- **413** — payload. See the budget note above.
- **429 / 503** — retry, back off, touch nothing.

And the genuinely silent case is not an error at all: **if the owner disabled
notifications, APNs returns 200 and nothing displays.** Only the phone knows,
so the phone says so, as an explicit `null` address on every foreground.
Surface it in Devices in plain words —
*"This iPhone has not been reachable since Tuesday"* — because the detection is
worthless if nothing says so. Re-register on every launch, so most of this
heals itself the next time the app is opened.

## Two payload shapes, one key, two concepts

The same APNs key also gives the box an **out-of-band wake** for a device whose
iroh socket has wedged (iroh#4289, still open) — the one channel that is not
iroh, for the case where iroh is the thing that is broken. A `content-available`
push, budgeted and rare, which is fine because it is needed rarely.

**Keep it a separate concept in the schema and the vocabulary even though it
rides the same key.** A wake that says "your phone thinks it is offline" is
plumbing; a reminder is the owner's object. Sharing a pipe is fine; sharing a
name is how `credentials` ended up holding two opposite trust directions.

## Phases

1. **Address** — built (see Status). What later phases inherit from it:
   - `push_address` + `push_address_at` (written together; `_at` means *last
     confirmed*, which is what an APNs 410's timestamp is compared against),
     re-registered on every foreground, explicit `null` when notifications are
     off, cleared on every revoke path.
   - The token reaches the box through Rust (`virtues_report_push_address` in
     the reach plugin, request builder `reach_client::push`), not JS, because JS
     is not guaranteed to exist when iOS delivers a token.
   - Tauri forwards no remote-notification callbacks to plugins, so
     `PushRegistrar.swift` attaches them to tao's app-delegate class at runtime
     (`class_addMethod`). **The first device test is whether that hook fires.**
   - Phase 2 also needs a `UNUserNotificationCenterDelegate` with
     `willPresent`: iOS shows nothing for a notification that arrives while the
     app is foregrounded unless the app asks.
2. **Signer.** Relay through `virtues-api`, `VIRTUES_PUSH_SIGNER_URL`, the
   local-p8 path, full response classification. **Plaintext payload** — the
   push carries the text and iOS displays it, no extension involved.
3. **Reminders.** The applet archetype, the evaluator invariants, the authoring
   loop, the Devices/applet surfaces. **Gated on data triggers** (see
   `applets-next-plan.md`) for anything ingest-shaped; cron + condition
   covers the rest at a latency cost, exactly as that plan says.

**v2, deferred on purpose: the encrypted payload + notification-service
extension.** Everything above is unchanged by it — same column, same signer,
same registration, same error handling. Encryption changes only what goes in
the `alert` field. The Swift is about thirty lines; the cost is the plumbing
around it (a second Xcode target inside a Tauri-generated project, an App Group
and shared keychain access group for the key, a key minted at pair time, and a sane fallback body for when the extension times out and iOS
shows the payload unmodified). Do it when the feature has earned it.

**The crypto itself is about an hour, and is not the reason to defer.**
`virtues_helpers::crypto::seal_aes_256_gcm` already exists, is CI-linted, and
seals as `nonce(12) || ciphertext || tag(16)` — byte-for-byte what CryptoKit's
`AES.GCM.SealedBox(combined:)` expects, so the device side is two lines and
needs no HPKE, no age, no curve conversion and no new dependency. The only
missing piece is a shared key, and it needs no agreement protocol: mint 32
random bytes at pair time and hand them over the iroh channel the
raw-public-key handshake has already authenticated, keychain on the device and
`TokenEncryptor` at rest on the box, exactly as `credentials` does it. **Do not
derive it from the device's iroh key** — that is one key doing authentication
and encryption, the same conflation this plan's own thesis is about. The
tradeoff to accept knowingly is no forward secrecy: a stolen device key
decrypts past pushes, which is tolerable because whoever holds it also holds
that device's iroh credential and therefore the whole record. Overhead is 28
bytes plus base64, so a 200-character reminder is about 300 bytes against a
4KB budget. So the v1/v2 line is binary and it is drawn at the extension, not
at the encryption: build the NSE and encryption comes free with it.

## Refuted

- **Local-only notifications** (`UNLocationNotificationTrigger`, calendar
  triggers) as a tier. Covers a thin slice; a second mechanism for one concept
  is not worth it. Dropped 2026-09-22.
- **Holding the iroh endpoint up so the box can dial the phone.** Undoes
  endpoint parking, and the extension is a separate process that could not use
  it anyway.
- **A heartbeat drain** ("anything for me?") on an empty outbox. Spends a dial
  every five minutes forever to rebuild, worse, a connection Apple already
  maintains for every app on the device.
- **Contentless wake plus a fetch from the extension.** Superseded by carrying
  the text in the push: it put a cold iroh dial on the user-visible path and
  failed precisely when the box was unreachable.
- **Mac-first notifications.** Free to build and genuinely zero-infrastructure,
  but a desktop banner is a weak product surface and would not have told us
  whether the feature is any good. Dropped 2026-09-22.
- **A channel list — Telegram, Slack, Discord, Signal, email — with the owner's
  own credential.** Tempting, and more sovereign than APNs in one real sense
  (the box talks straight to the service and we are nowhere in the path). But
  it answers "who delivers this" with a settings screen, and the first thing to
  ship should be the box's own voice on the owner's own phone. Keep it in the
  back pocket: once a destination field exists, each sink is about a day.
- **Linq / iMessage as the channel.** Bidirectional and genuinely more than a
  notification system — a reply loop, no app required, reaches every Apple
  device the owner owns. Refused on identity, not capability: **every reminder
  would transit a vendor's servers in the clear**, which inverts the one claim
  the product is built on, kills the DIY tier (no self-hoster gets an account),
  and brings an A2P compliance stack — opt-out keywords, line reputation,
  volume ramps — to the job of telling someone their own reminder. Possible
  v2 as an **inbound** door (the owner texting their box is the owner's act,
  not the box publishing their record) or as an explicit per-reminder opt-out
  of the house.
- **Web Push / VAPID.** The only architecture needing no Apple team key, no
  signer and no entitlement — the box would sign with its own key. Refused
  because it does not work in a WKWebView, so it would require owners to
  install a home-screen PWA *instead of* the App Store app. A product fork, not
  a feature.
- **Blind tickets** (Privacy Pass at the signer). A week of work for a property
  that egress IP undermines anyway: it removes the durable database join but
  the signer still sees where the request came from. Say plainly what the
  signer sees instead. Revisit only if it becomes a real objection.

## Open

- Whether to pursue the time-sensitive capability now or after phase 3.
- Whether the drain cadence should tighten while a reminder depends on
  phone-sourced data. Needs the battery cost of a shortened
  `CONSTRAINED_DRAIN_SECS` measured on a real device first.
- What `apns-expiration` should be. It is the whole of "how long we keep
  trying" and nobody has picked a number.
- Whether a fired reminder lands in chat as something the owner can answer
  ("did it" / "remind me tomorrow"), or simply ends. That is the difference
  between a reminder app and an assistant that remembered something.
