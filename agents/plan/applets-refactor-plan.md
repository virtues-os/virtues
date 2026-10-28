# Applets: make scheduled applets run reliably and show their results

**Status:** Planned 2026-10-06. Nothing built. Written against `wave` after an
audit of the whole applets surface (10 areas, each finding checked against the
code by a second reader). File and line links below are from that day.

## Goal

You describe an applet once in chat and then do not need to think about it
again. It runs at the time it says, and you can always see whether it worked.

Today none of that is true. A user report (2026-10-06): a daily report applet,
shown as "On · Daily at 9pm" in the list and "Daily at 10:15am" on its own page,
never ran at either time, and the owner could not tell where its output would
have gone. The owner rebuilt it from scratch because they could not trust the
existing one.

## What is wrong

1. **Applets that say On can never run.** The scheduler only picks up a row
   whose triggers include `cron`, whose agent is non-empty, and whose cron has
   six fields ([scheduler/mod.rs:87](../../virtues-core/src/scheduler/mod.rs#L87)).
   Creation checks none of that: explicit triggers without `cron` pass
   ([applet_setup.rs:97](../../virtues-core/src/tools/applet_setup.rs#L97)), and
   any 5 or 6 field string passes
   ([applet_setup.rs:150](../../virtues-core/src/tools/applet_setup.rs#L150)).
   Every screen still shows the schedule.
2. **Changes do not stick.** A time changed on the page or by the applet tool's
   `edit` writes only the row; the next reconcile (boot, any new applet, a source
   connect) puts the manifest's value back
   ([applet_templates/mod.rs:1359](../../virtues-core/src/applet_templates/mod.rs#L1359)).
   Changing the time through `edit` also switches the applet off, and
   nothing says so
   ([applet_management.rs:138](../../virtues-core/src/tools/applet_management.rs#L138)).
3. **When it runs, you will not see it.** Output is appended to the chat it was
   made in, with no notice
   ([agent/applet_runner.rs:284](../../virtues-core/src/agent/applet_runner.rs#L284)).
   An empty reply posts nothing and still records success. Each run replays
   that whole chat as context, so it grows with every run
   ([agent/applet_runner.rs:95](../../virtues-core/src/agent/applet_runner.rs#L95)).
4. **When it does not run, nothing tells you.** A skipped, missed or never
   scheduled time leaves no record. Times show no zone, and every future time
   reads "now" ([palette.ts:59](../../apps/web/src/lib/applets/palette.ts#L59)).
5. **Faces go blank.** The face sandbox blocks every outside host
   ([faces.rs:133](../../virtues-core/src/server/faces.rs#L133)) and the guide
   never tells the model, so a dashboard that loads Chart.js or a web font from
   a CDN renders empty. Turning an applet on replaces the page's data with a
   reply that has no `has_face`, so the face vanishes until reload
   ([AppletDetailView.svelte:297](../../apps/web/src/lib/components/tabs/views/AppletDetailView.svelte#L297)).
   The face token is minted once and dies after an hour or a restart
   ([FaceFrame.svelte:23](../../apps/web/src/lib/components/applets/FaceFrame.svelte#L23)),
   and faces always render light because `data-theme` holds a theme name, never
   `dark` ([FaceFrame.svelte:24](../../apps/web/src/lib/components/applets/FaceFrame.svelte#L24)).
6. **The screens show machinery.** The list is a crontab grid where bank syncs
   and box upkeep outnumber what the owner asked for. The page leads with
   settings, edits schedules as cron text, and tells the owner about
   "the applet's manifest" and "reconciling"
   ([AppletDetailView.svelte:547](../../apps/web/src/lib/components/tabs/views/AppletDetailView.svelte#L547)).
   The chat card is a snapshot that says "Running." whether or not anything
   will run
   ([AppletProposalCard.svelte:115](../../apps/web/src/lib/components/chat/AppletProposalCard.svelte#L115)).
7. **Some of it damages data whether or not anyone uses applets.**
   - Reconcile overwrites every shipped applet's `config`
     ([applet_templates/mod.rs:1380](../../virtues-core/src/applet_templates/mod.rs#L1380)),
     which is where sync cursors live
     ([applet_runner/mod.rs:1169](../../virtues-core/src/applet_runner/mod.rs#L1169)),
     so Plaid, Gmail, calendar and X lose their place on every boot.
   - The id to folder lookup matches by prefix, so deleting `daily_2` can act
     on `daily`'s folder
     ([applet_templates/mod.rs:864](../../virtues-core/src/applet_templates/mod.rs#L864)).
   - Jobs keep the UTC offset from when they were registered
     ([scheduler/mod.rs:167](../../virtues-core/src/scheduler/mod.rs#L167)),
     so after the clocks change every daily applet runs an hour off, and twice
     in autumn. EU clocks change 2026-10-25, US 2026-11-01.
   - A face's SQL can switch back from the read-only role and read every
     `app_*` table ([faces.rs:369](../../virtues-core/src/server/faces.rs#L369)).

## Decisions made

- **The AI has full permission.** Creating or changing an applet from chat
  needs no Allow prompt and no Enable step: it is on as soon as it is made.
  This replaces the gate in [record/applets.md](../record/applets.md) ("no path
  from model output to an enabled, scheduled row without a user action"), the
  Allow prompt for applet `setup`
  ([executor.rs:344](../../virtues-core/src/tools/executor.rs#L344)), and
  `edit`'s switch-off. The guard that remains is money: every applet the
  AI makes gets a daily spending cap, shown on its card.
- **The result lives on the box; notifications point to it.** Every run's
  output is kept as the run and shown on the new chat screen (home and chat
  are one place since the home page was removed on 2026-10-06) and on the
  applet's page. A
  notification ("Your client work breakdown is ready") is a pointer through
  [notifications-plan.md](notifications-plan.md)'s router, never a copy sent
  by the applet itself. External channels get a pointer by default.
- **Chat is not a delivery target.** Chat is where you make and change an
  applet. Reports do not go back into it.
- **Plumbing leaves the Applets page.** Per-account syncs and device ingest go
  to Sources, box upkeep to System. Applets lists only what the owner asked for.

## Target, by area

| Area | Target |
|---|---|
| **Creating** | The AI makes it and it is on. The server refuses anything that would not run (no `cron` with a schedule, bad cron, schedule on a face-only applet) and normalizes what it can. The tool result states the next three run times with zone and where results appear, so the AI can say so. The guide tells the model a face cannot load outside hosts. |
| **Chat card** | A summary card that reads live state: what it does, when ("Every day at 9:00pm, New York time"), where results show up, its spending cap, and Turn off and Undo. It updates when the applet changes. |
| **New chat screen** | Home is a new chat, so this is where you already are. "From your applets", above the composer while the thread is empty: what arrived since you last looked, newest first, and anything that did not run, in a sentence ("Didn't run at 9:00pm: your server was off"). |
| **Applets list** | Only the owner's applets. Each row: name, what it last produced and when, when it runs next with zone, and a plain state (on, off, can't run). |
| **Applet page** | Its output first: the face, or the latest reports. Then one sentence for when it runs, a history of every scheduled time and what happened, and simple controls (time picker, on/off, cap). Changes go through the same path as chat. No cron, no manifest. |
| **Faces** | Load every time, follow the theme, survive saves and restarts, and show a sentence instead of a blank box when they break. |

## Order of work

### 0. Fix the bugs that damage data (must reach stable before 2026-10-25)

- Exact folder lookup instead of prefix.
- Reconcile keeps a shipped applet's sync state; cursors move to their own field.
- Re-register jobs when the zone's offset changes.
- One rule for "will this run", used by creation, the scheduler and every
  screen. Existing applets that could never run are repaired on upgrade and
  start running, under the default cap.
- Edits to chat-made applets write the manifest, so reconcile keeps them.
- Close the face role escape.

**Done when:** a test retimes an applet through each path and it survives a
reconcile; a sync cursor survives a reboot; a DST test shows one run at the
right local hour; an applet made with triggers `["manual","message"]` and a
schedule either runs on schedule or is refused with a sentence.

### 1. Creating and the chat card

- Remove the Allow prompt and the Enable step; add the default cap.
- The summary card, reading live state.
- Runs start clean (the applet's prompt and notes, not the authoring chat).
- Faces: guide and check cover outside hosts; one applet shape from every
  endpoint; token refresh; theme.

**Done when:** "make me a daily report at 9pm" produces an applet that is on,
a card that says when and where in the owner's zone, and a first run whose
output does not depend on the chat it came from.

### 2. Show results and missed runs

- A record for every scheduled time: ran, skipped and why, failed and why.
- "From your applets" on the new chat screen.
- The notification object from [notifications-plan.md](notifications-plan.md),
  built once, with applets as a first producer ("ready" and "didn't run").
  First channel decided below.

**Done when:** a scheduled applet's output appears on the new chat screen within a minute of
the run, and a run missed while the box was off shows as a sentence.

### 3. The list and the page

Rebuilt to the target above, using the shared list and tab components
(UniversalDataGrid, route-driven SubNav). Mockups first.

### 4. Move syncs and upkeep out of Applets

Syncs and ingest under their connection in Sources; upkeep in System. The
"Show built-in" toggle goes.

## Still open

| Decision | Recommendation |
|---|---|
| How slice 0 ships before 2026-10-25: hotfix from the stable tag, or next stable from `staging` | Needs the owner; either works if it lands in time |
| The default daily cap for AI-made applets | $1.00, editable on the page |
| Catch-up after the box was off | Run once if still within the period and the next run is more than 2 hours away; label it late |
| First notification channel | Phone push, since the user report was about a phone; the push work in [reminders-plan.md](reminders-plan.md) becomes the critical path |
| Hide Applets until slice 1 | No: the damage in "What is wrong" 7 happens either way, and hiding removes the off switch |

## Related

- [record/applets.md](../record/applets.md) keeps the primitive, the runtime
  allowlist and the limits; its gate section is replaced by the decision above
  when slice 1 ships.
- [applets-next-plan.md](applets-next-plan.md) (data triggers, Persona) waits on
  this.
- [record/applets-surface-audit.md](../record/applets-surface-audit.md) marks two
  fixes as shipped that are not (the row toggle and glossed fields); correct it
  in slice 3.
- [applets/AGENTS.md](../../applets/AGENTS.md) and
  `MANIFEST_SCHEMA.json` (deleted on 2026-10-27, when applets became
  `SKILL.md` folders) are rewritten
  against the code in slices 0 and 1: the run contract, the gate, faces, and
  the `message` trigger the schema omits.
