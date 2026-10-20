# Reach polish

**Status:** Open (2026-09-29). The recovery mechanism shipped; see
`agents/record/reach-reliability.md`. These three follow-ups remain. Delete
this plan when they ship.

- **The desktop app has no recovery trigger.** The Mac runs the same
  in-process reach plugin and its `:7117` loopback reads the warm client per
  connection, so a rebuild would reach it, but nothing ever calls
  `recover_connection` on desktop. Only `ReachMonitor.swift` (iOS) does. Wire
  sleep/wake and network-change events on macOS to the same function. The Mac's
  page now stays up when the box is gone and shows the reconnect banner
  ([record/local-ui.md](../record/local-ui.md)); this item is the native
  reconnect underneath it.
- **No recovery counter.** `stats.rs` counts a rebuild as an ordinary `dials`
  increment, so a wedge recovery cannot be told apart from a cold build.
  Count pokes that healed and rebuilds separately; that is the only way to
  measure how often #4289 bites in the field.
- **Nothing in the UI shows a reconnect.** The device screen shows live path
  state only. A "reconnected" event, when a rebuild happened, would make the
  mechanism visible and make a report of "it went offline" checkable.
