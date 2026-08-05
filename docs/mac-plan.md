# The Mac, end to end

Onboarding, permissions, and every update path the Mac has — the app, the
collector, and the web UI it shells to.

Written after a six-day silent outage (2026-07-30 → 2026-08-05) that all three
surfaces failed to report, and in some cases actively misreported.

---

## 1. The target

**The user never learns that the Mac app has a version.** Everything below is
what it costs to make that true.

- **Installing.** One download, drag, open. It finds the box, you confirm a
  code, and it asks for the two system permissions in the order it needs them.
  macOS can't prompt for either, so each row is a button that opens the exact
  pane and turns green by itself when you flip the toggle.
- **Running.** No window. A menu-bar dot, and a click tells you which stream is
  affected in plain terms — "Messages aren't being read" — never the name of a
  macOS permission.
- **Updating.** Nothing you can see. The app updates when idle, the collector
  updates with it, the interface updates when your box does. Settings shows one
  number: the same release your box shows, because it's the same product.
- **Breaking.** The box says so within minutes, naming the stream and the fix:
  *"Messages stopped on Jul 30 when the app updated; macOS dropped its Full Disk
  Access grant. One click to restore."*

That last one is load-bearing. This document exists because the product's answer
for six days was a banner naming the wrong permission.

---

## 2. Invariants

1. One version the user sees, and it is the product's.
2. The channel is chosen once, on the box. Devices follow; they never have their
   own opinion.
3. An update is never a question. Silent, applied when idle.
4. No component asserts a fact it didn't observe. "Unknown" is a real state and
   must reach the screen as one.
5. Every claim carries when it was observed.
6. A version only goes up, enforced mechanically.
7. Failure is described by consequence first, cause second.

---

## 3. Where we are

Measured 2026-08-05 on a live pairing.

| Surface | State |
|---|---|
| Onboarding choreography | Good. Polls truth, self-advances, deep-links both panes. |
| Permission truth | Correct in Swift, **discarded in Rust** before it reaches the UI. |
| Permission freshness | **Freezes once granted.** Republished only while something is broken. |
| Collector auto-update | Works. Piggybacks the app, restarts the LaunchAgent. |
| Web UI delivery | Genuinely OTA from the box. |
| Queue durability | Proven — 891 records held through total routing failure. |
| App update | **Inverted.** Tree builds 1.0.15; production publishes 1.0.20 from July. |
| Channel | Per-device file, unrelated to the box's. |
| Edge channel | Publishes `0.3.0` against a stable line at `1.0.20`. Undeliverable. |
| Local rebuild | Unsigned by default. Silently drops both TCC grants. |

Three version lines for one product: `virtues-core` 0.3.0, the app 1.0.x, the
collector a git SHA. The tray shows `Virtues v1.0.15` — a number matching
nothing else the user can observe.

**The undeclared coupling.** The app bundles only `pair.html` and shells to
`localhost:7117`, served in-process from the box. So the box serves the
JavaScript that calls the app's Tauri commands: `bridge.ts` ships with the box
and `invoke()`s a surface compiled into a separately-versioned binary. No
negotiation, no feature detection, no error path — a box newer than the app
calls a command that doesn't exist and fails at runtime, inside whatever feature
needed it. Latent only because that surface has been stable. No version scheme
fixes it.

---

## 4. The design

### Two identities, one invisible

"What release am I on" and "is there something newer" are not the same number.

- **Release identity** — what the user sees. The product version, owned by the
  box, shown everywhere. A Mac paired to a 0.3.0 box says 0.3.0.
- **Build identity** — what the updater compares. An opaque counter that only
  increases. Never displayed, so it can't be made incoherent.

This dissolves the mess rather than reconciling it: the 1.0.x history becomes a
build counter starting at 21. `virtues-core` keeps its lineage untouched, which
matters — real boxes compare those versions during `virtues upgrade`.

`tauri.conf.json.version` becomes the counter (`+1` per release). The displayed
string comes from the paired box, cached for offline, falling back to "not
paired" rather than to a build number.

### The channel lives on the box

Delete the per-device channel file; ask the box. One box, one channel, every
device consistent. An unpaired app has no channel and doesn't update — correct,
since it has nothing to do. Both channels publish from the same counter, so the
edge-versioning bug disappears by construction.

### Updates are silent

Keep the instinct — never force a relaunch — and go further: never ask.

Check on launch and every 6h, download and stage silently, apply on quit. For an
app running for weeks, apply during an idle window and relaunch to the same
state. Surface only after two failures, as an amber tray line with a real
reason. The collector keeps piggybacking; `reconcile_helpers` is sound.

### Permissions are three-state

`granted | denied | unknown`, with an observation time, from Swift to the screen
without loss. The truth already exists — `StatusCommand` emits
`permissionsReportedByDaemon` and `permissionsCheckedAt`, and `CollectorStatus`
drops both. Plumb them through, then:

- **unknown** renders as "haven't heard from the collector since <time>", never
  as a denial. This alone would have prevented the outage.
- Copy leads with consequence, cause second.

And the record has to actually refresh. `recordFromDaemon()` lived inside the
`!hasFullDiskAccess` branch, so it ran only while something was broken: granting
the permission made the branch unreachable and froze the record at its last
value. A permission revoked after startup was never noticed, and `isStale`
flipped true fifteen minutes into healthy operation — which is what taught every
reader to ignore the flag. Fixed by republishing on every tick, ahead of the
pause check: pausing collection is a statement about data, not permissions.

### The re-grant case

macOS keys a TCC grant to the binary's signature. Replace the binary with one
signed differently and the old entry survives, looking granted, doing nothing —
exactly how iMessage died on Jul 30.

Local builds default to a Developer ID identity from the keychain; unsigned
becomes opt-in and loud. The Full Disk Access row already carries the right
hint ("Not listed? Click + and add `~/.virtues/bin/virtues-collector`");
Accessibility needs the same line, and both should say remove-and-re-add when we
observe *denied* while an entry exists.

### Declare the OTA contract

The app exposes a command-surface version. `bridge.ts` checks it at load and
degrades a feature explicitly rather than throwing inside it.

---

## 5. Sequence

| Phase | Work | Cost |
|---|---|---|
| 0 | Bump `tauri.conf.json` past 1.0.20 (the built app is offered a downgrade to July) and republish health on every tick | ~3 lines |
| 1 | CI gate: refuse any release whose counter isn't greater than the published `latest.json` | ~25 lines |
| 2 | Staleness through Rust + `bridge.ts`; three-state rendering | the one that pays for itself |
| 3 | Display the box's version; demote the config version to a counter; retire the device channel file | |
| 4 | Stage-and-apply-on-quit; remove the restart ask | |
| 5 | Command-surface version and graceful degradation | |

Phase 1 matters because the messy history exists where nothing enforced
monotonicity — care is not a mechanism. Phase 2 is the difference between a
six-day silent outage and a five-minute one.

---

## 6. Deliberately not doing

- **Renumbering `virtues-core`.** Boxes compare those versions during upgrade
  and migration lineage keys off them. The two-identity split exists so we never
  have to.
- **A changelog in the app.** The box is the product surface; the app is a
  window.
- **Forced relaunch**, even for security fixes. Apply-on-quit gets there without
  seizing the machine from someone mid-sentence.
- **Per-device channel override.** It's how you get a fleet that disagrees with
  itself, and a support conversation that starts with four unknowns.
