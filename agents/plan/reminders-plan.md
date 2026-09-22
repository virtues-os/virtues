# Reminders — the box's way of reaching the owner

**Status: planned.** Nothing here is built. Written 2026-09-22.

An applet can be woken, can check a condition, and can write to the record.
It cannot *reach the owner*. `applets-overhaul-plan.md` names this precisely
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

- **Wake and condition** — `applets-overhaul-plan.md` §Vocabulary. *Trigger =
  who wakes you; condition = what you check once awake.* Data triggers
  (`trigger=data:data_location` + `condition="speed < 5"`) are phase 4 there.
  This plan **depends on** that phase and adds nothing to it.
- **One-shot vs standing** — already the `until` field: absent = forever,
  `"once"` = first success, SQL bool = archive when true. Nothing new needed;
  the authoring model must simply fill it, and the UI must show what it chose.
- **Rate limiting** — `max_runs` per hour/day, already marked mandatory before
  data triggers light up composition loops.
- **Chat authoring** — `applet-authoring-plan.md`.
- **The iroh socket wedge** — `reach-reliability-plan.md` and iroh#4289, still
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

### The payload is encrypted, and that is the whole privacy story

The push **carries the reminder text, encrypted to a key only that device
holds**. APNs allows ~4KB, which is ample. The extension decrypts locally in
microseconds. Web Push has done this since RFC 8291.

This is strictly better than the contentless-wake-plus-fetch design it
replaces:

- No dial from the extension, so no 1–5s cold-start on the user-visible path,
  no 30s ceiling, and no need for a fallback banner that says something
  useless.
- **It works when the box is asleep, offline, or wedged** — which is exactly
  when a fetch design fails.
- The claim in the manual becomes *"content is encrypted, so transit does not
  matter"* rather than *"we promise the wake carries nothing."* The first is a
  property; the second is a promise.

Key material: the device already presents an ed25519 key at pair time. Either
convert it for ECDH or mint an X25519 key alongside it — decided at build.

Budget the payload well under 4KB: AEAD overhead plus base64 expands it, and
`413 PayloadTooLarge` is a silent failure for the owner. Truncate the text
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

**The signer should be unable to correlate**, not merely promise not to. The
box redeems its `api_key` at atlas for anonymous, unlinkable push tickets
(blind signature / Privacy Pass shape) and presents a ticket plus a device
token. The signer verifies the ticket without learning which box issued it. The
relay is already blind by construction; building this to a weaker standard than
the relay is the inconsistency someone would rightly write about.

Feedback survives the blinding because it is **request/response, not callback**:
the box calls the signer, the signer calls APNs, the signer hands the status
straight back and stores nothing. No correlation needed at any point.

### Time-sensitive

Time Sensitive is a self-serve capability, not one of the request-a-form
entitlements (that is Critical Alerts, a much harder ask). It is policed at App
Review by usage. The justification is the canonical one: a reminder the owner
asked for, in their own words.

**Per reminder, never global.** If everything is time-sensitive the owner's
notification summary stops working and they turn the app off, which is worse
than a reminder landing an hour late. The authoring model proposes it; the
owner overrides it.

## Schema

| Change | Why |
|---|---|
| `app_device.push_address` | The address half of the pair, beside `endpoint_id`, on the row whose revocation already kills reachability for free. |
| `app_device.push_address_at` | Registration time. **Required** to handle a 410 correctly — see below. |
| Push authorization in `device_info.permissions` | The self-report already carries collector permissions for exactly this reason: a revoked permission is otherwise indistinguishable from "nothing happened". |

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
notifications, APNs returns 200 and nothing displays.** Only the device's own
authorization self-report catches it. Surface it in Devices in plain words —
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

1. **Address.** `push_address` + `push_address_at`, registration at pair time
   and on every launch, authorization in the permissions self-report, the
   Devices surface for unreachable. No sending yet. Ships useful on its own:
   the box can finally say whether it could reach a device.
2. **Signer.** Relay through `virtues-api`, `VIRTUES_PUSH_SIGNER_URL`, the
   local-p8 path, full response classification. Blind tickets here or in 5.
3. **Encrypted payload + extension.** Key at pair time, encrypt on the box,
   decrypt in the NSE.
4. **Reminders.** The applet archetype, the evaluator invariants, the authoring
   loop, the Devices/applet surfaces. **Gated on data triggers** (phase 4 of
   `applets-overhaul-plan.md`) for anything ingest-shaped; cron + condition
   covers the rest at a latency cost, exactly as that plan says.
5. **Blind tickets**, if not taken in 2.

Phase 1 is worth doing before anything else is decided: it is additive, it has
no cloud dependency, and it converts an invisible failure into a visible one.

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
- **Contentless wake plus a fetch from the extension.** Superseded by the
  encrypted payload: it put a cold iroh dial on the user-visible path and
  failed precisely when the box was unreachable.

## Open

- Blind tickets in phase 2 or phase 5 — roughly a week either way.
- Whether to pursue the time-sensitive capability now or after phase 4.
- Whether the drain cadence should tighten while a reminder depends on
  phone-sourced data. Needs the battery cost of a shortened
  `CONSTRAINED_DRAIN_SECS` measured on a real device first.
- What `apns-expiration` should be. It is the whole of "how long we keep
  trying" and nobody has picked a number.
- Whether a fired reminder lands in chat as something the owner can answer
  ("did it" / "remind me tomorrow"), or simply ends. That is the difference
  between a reminder app and an assistant that remembered something.
