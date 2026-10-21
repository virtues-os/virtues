# Applets — what is next

The applet primitive, its authoring loop and the gate are built and documented
in [applets.md](../record/applets.md). Two things that design describes are not
built yet.

The loop and the gate do not work as described today.
[applets-refactor-plan.md](applets-refactor-plan.md) fixes them first, and data
triggers build on the single change path it adds.

## Data triggers

A wake on new rows: `trigger = data:<table>`, carrying the new rows as the
payload, composed with a `condition` (`data:data_location` + `speed < 5`).
The trigger decides when the applet wakes; the condition is checked after it
wakes.

Unbuilt: the trigger set is closed at cron · manual · tool · api · webhook ·
message, validated in `scheduler/applets.rs` and `tools/applet_setup.rs`.
Today cron-poll plus a condition covers everything a data trigger would, at a
latency and efficiency cost, which is why this came last.

- Accept an object trigger (`{"data": {"table": …}}`) beside the bare
  strings, which stay as shorthand.
- `max_runs_per_hour` / `max_runs_per_day` exist and must be on by default for
  data-woken applets before anything can wake on another applet's output:
  that is the composition loop.
- Applet-finished as a wake is **not** in scope. Run chaining was dropped
  (0011) for having no writer; if composition returns, it returns with
  schema, writers and readers together.
- `reminders-plan.md` depends on this for anything ingest-shaped.

## Persona

"Reply to Mom as me": an agent with an identity, boundaries, exemplars drawn
from the owner's own messages, and a channel. **Not authorable**: there is no
inbound wake from a third party (`message` is inbound from the owner only) and
no send-as-me channel. AGENTS.md tells the authoring model to refuse and
offer draft mode: a scheduled applet that drafts replies as pages for the
owner to send.

Needs, in order: a channel that can reach someone (the reminders plan builds
the owner-facing half), an inbound wake carrying a third party's message, and a
send verb behind the "side effects that send" boundary.
