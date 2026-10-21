# Version compatibility — when the app outruns the server

How a phone that updates itself keeps working against a server that only
updates when someone types a command.

**Status (2026-10-20):** part of this shipped with the Mac's own copy of the
app: the box serves an `api_version` integer in `/health`
(`virtues-core/src/api_version.rs`), and the SPA refuses a box below
`MIN_BOX_API` (`apps/web/src/lib/boxApi.ts`) with one "Your server needs an
update" screen. See [record/local-ui.md](../record/local-ui.md). Check each
item below against that before building it; the CI ratchet is still open.

## The incident

`POST /api/chat` required `model` and validated it against the allowed list
through v0.1.5. v0.1.6 made it optional, so the server resolves the turn's model
from its slot. That change was correct, and it let us remove
`default_model_id`, because a seeded default freezes the choice forever.

Nobody noticed the consequence until an App Store reviewer was two taps away
from hitting it. An app built after that change omits `model`; a v0.1.5 server
rejects the body outright, so **chat fails to deserialize on every message**.
Chat stops working entirely: it shows "An error occurred" and a Retry that
fails again.

We verified both directions against the review server with the app's exact wire
shape: 422 on v0.1.5, a real answer on v0.1.6.

**This asymmetry is permanent.** Phones update themselves; most people have
App Store auto-updates on. Servers update when their owner runs
`sudo virtues upgrade`. So the app is always the side that runs ahead, and every
wire change causes an outage for anyone who hasn't upgraded. The error appears
in the app, even though the app is the part that is up to date.

## What already exists

Nearly all the plumbing is already built, which makes the work smaller than
expected. It is not yet connected to anything.

**Both version coordinates already travel.**

- Server → app: `GET /api/updates` returns `UpdateStatus` with `current`
  (`CARGO_PKG_VERSION` — its own doc comment calls it "the migration/compat
  coordinate everything else speaks") alongside `running_version` (release
  identity from the baked build tag), `running_channel`, and `running_ahead`.
- App → server: `app_device.device_info->'build'->>'app'` is recorded per
  device and surfaced as `app_version` in `api/devices.rs`.
- The app already has the typed client for all of it —
  `apps/web/src/lib/api/client.ts:263`, comment included: *"Kept because it is
  the compat coordinate."*

**Prerelease ordering is already solved.** `cli/upgrade.rs` parses with
`semver::Version` and compares the baked tag instead of the crate version.
That fix came from the `0.1.0-staging.N` series, which every stable server
correctly refused as a downgrade.

**The forward-compatibility rules are already written down** — in
`crates/virtues-helpers/src/contract.rs`, for the applet subprocess envelope:
optional fields use `#[serde(default)]`, adding one is backward-compatible, and
**"don't add `deny_unknown_fields`"**. Nothing in the tree uses it. That rule is
correct, but it was never applied beyond the subprocess wire to the
client↔server wire, which is the one that now spans versions.

So the missing pieces are a declared minimum version, a comparison, and a
message to show. This does not need a new subsystem. The earlier estimate in
conversation ("a design doc, not a sprint task") was wrong about the plumbing,
though the design questions below are real.

## What the sweep found beyond `model`

Comparing v0.1.5 → v0.1.6 for shapes that cross the wire:

| Change | Direction | Fatal? |
|---|---|---|
| `model: String` → `Option<String>` (two request types) | new app → old server | **Yes.** 422 on every chat. |
| `/api/wiki/chapters` added | new app → old server | 404 on that feature only. |
| `/api/metrics/activity` removed | old app → new server | 404; the commit retiring it says nothing reads it. |
| `default_model_id` / `background_model_id` removed | old app → new server | Safe — no `deny_unknown_fields`, so extra fields are ignored. |
| `interview_started`, `degraded` added to responses | old app → new server | Safe — unknown response fields ignored. |

**Removing or tightening a REQUEST field is the change that breaks chat.**
Every other change only removes a feature. That rule is small enough to
enforce.

## Open design questions

1. **Global minimum or per-capability?** A single "app needs server ≥ X" is
   coarse: 1.2.16 works against v0.1.5 for everything except chat, and blocking
   the whole app breaks more than the skew did. Per-capability is a negotiation
   matrix and does not obviously pay for itself yet.
2. **When is the check made?** At pair, at app start, or lazily before the first
   send? Cheapest correct answer is probably at pair plus on reconnect, since
   the version cannot change without a restart.
3. **What if the remedy is not in the person's hand?** Telling someone on an
   iPhone to run `sudo virtues upgrade` assumes shell access to their server.
   Often they are not near it. The message must say so, and the app should
   ideally offer to trigger the upgrade remotely (the endpoint exists) as well
   as naming the command.
4. **The reverse direction.** An old app against a new server is a permanent
   case and will not go away after a transition period.
5. **The test matrix.** N app versions × M server versions, which does not
   collapse on its own. Minimum: current app against the two most recent server
   releases.
6. **Is this the same mechanism as the plugin ACL lockstep?** That problem —
   `build.rs` COMMANDS as the single ACL source, and a paired window loading a
   server-served SPA refusing unresolved commands — is the same shape: two
   artifacts that must agree, shipped separately. One mechanism should
   probably cover both.

## Proposed first slice

This slice is small, and it turns the fatal failure into one the person can
diagnose:

1. The app declares `MIN_SERVER_VERSION` at build time — one constant, bumped
   by hand whenever a request field is removed or tightened.
2. On pair and on reconnect, read `current` from `/api/updates` and compare.
3. On mismatch, the app shows a specific message naming the actual remedy,
   and — where the app can reach it — a button that triggers the upgrade, so
   the person does not have to run a command they may not have access to.
4. A CI check that fails when a request-type field is removed or made required
   without `MIN_SERVER_VERSION` moving. This is the part that stops the next one
   happening, and it is the same ratchet shape as
   `tools/check-swallowed-queries.sh`.

Defer per-capability checks until something needs finer granularity than
"chat works / chat doesn't."

## What not to do

**Nightly auto-upgrade.** It would shrink the skew window to under a day, but
we already considered it and decided against it. The reasoning is in
`api/updates.rs`:

> A vendor who auto-ships to a fleet can do it safely because they watch that
> fleet and halt a bad rollout; virtues deliberately has no such telemetry, so
> it has no way to notice a bad release and no way to stop one. The human
> pressing the button IS the halt mechanism — every server that hasn't pressed
> it is a server a bad build never reached.

It also does not solve this problem. Auto-upgrade fixes *future* skew; it does
nothing for the fleet already on an old version, because they would need to
already be running the version that upgrades itself. And it cannot reach a
server that is off, offline, or pinned to a version on purpose.

If it is revisited, it belongs behind a trustworthy unattended rollback, and as
a separate decision from this one.
