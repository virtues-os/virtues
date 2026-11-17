# Performance checks

Three checks hold the app's speed in place. Each measures the same thing the
same way every time, so a change in a number means the code changed. Run the
one that covers what you touched before you call a change done, and when a
number has to go up, update its baseline in the same commit and say why in
the message.

| Check | What it holds | Where it runs |
|---|---|---|
| Bundle budget | What the chat home loads before its first paint | CI, on every pull request |
| Lab run | Cold load, opening a long chat, and memory across view changes | Locally, by hand |
| Query plans | The hot chat queries have an index to use | `cargo test -p virtues` |

## Bundle budget

`tools/perf/bundle-budget.mjs` reads the build's own manifest, follows the
static imports of the `(app)` catch-all route (the route every launch lands
on), and sums the JavaScript and CSS it loads before the first paint.
Dynamic `import()` is left out on purpose: that is how a library stays off
this path. The baseline is `tools/perf/bundle-budget.baseline`, and it may
only go down.

```sh
cd apps/web && pnpm build && cd ../.. && node tools/perf/bundle-budget.mjs --top 20
```

When it fails, `--top 20` lists the largest chunks on the path. The usual
cause is a static import of something heavy from a module the chat home
reaches. Load it where it is first used, with `import()`.

Cut a new baseline in a scratch clone, never in this checkout. The shared
tree holds other agents' uncommitted edits, and CI reads the commit (see
CLAUDE.md, "Never cut a ratchet baseline from this checkout"):

```sh
git clone --local -b wave . /tmp/budget && cd /tmp/budget/apps/web \
  && ln -s "$OLDPWD/apps/web/node_modules" node_modules && pnpm build \
  && WEB_DIR=/tmp/budget/apps/web node "$OLDPWD/tools/perf/bundle-budget.mjs" --update
```

## Lab run

`tools/perf/lab/run.mjs` does the whole run in one command:

1. It builds the web UI from a commit (HEAD by default) in a scratch clone.
2. It starts a box from the shared target's `virtues` binary on its own
   database, `virtues_perf_lab`, with `ENVIRONMENT=dev` and no AI. The box
   serves that build from `STATIC_DIR`, the way an installed box does, so the
   bytes and their compression are a box's.
3. It seeds invented data (`seed.mjs`): 40 short chats and one chat of 300
   messages with headings, lists, code, tables, reasoning and tool results.
   The generator is seeded, so the data is identical on every run.
4. It drives headless Chromium through three scenarios, each in a fresh
   browser context, five times, and keeps the median.
5. It compares the result with `tools/perf/lab/budgets.json`, then drops the
   database.

```sh
cd tools/perf/lab && pnpm install   # once
node run.mjs                        # measure and compare
node run.mjs --web-build DIR        # reuse a build, skip the clone
node run.mjs --update               # record budgets.json from this run
node run.mjs --keep                 # leave the box and database up to look at
```

| Scenario | What it records |
|---|---|
| `cold_load` | Time from navigation to a visible composer, JavaScript transferred, API requests before ready and in total, the waterfall's depth, long tasks, and heap, DOM and listener counts after garbage collection |
| `long_chat` | Time until the 300th message is in the page, the longest task, heap and DOM after garbage collection, and Chromium's layout and style time for three moves: opening the chat, narrowing the window (every message laid out again, as when the sidebar opens beside it), and scrolling to the top |
| `sidebar_toggle` | Six opens and closes of the sidebar beside the long chat, through the header's ∴ button: main-thread layout, style and script time per toggle (median after two warm-ups) and the most frames over 20 ms in any toggle |
| `view_cycle` | Ten laps through eight views by in-app navigation, and the growth per lap in heap, DOM nodes, listeners and documents between lap 2 and lap 10 |

How each number is held to its budget:

- Counts (requests, documents) and growth per lap are exact. They do not
  depend on the machine.
- Times get 30% headroom with a 50 ms floor. They depend on the machine, so
  compare them only between runs on one machine. Layout times get the same
  30% with a 10 ms floor, because a 50 ms floor would hide a long chat's
  relayout going back to what it cost before its rows could be skipped.
- Other work on the machine (another agent's build) moves every time at
  once. To compare two builds, run them back to back, more than once, and
  compare the quiet pairs.
- Sizes (heap, transfer, DOM nodes, listeners) get 5%.
- The waterfall's depth can differ by one between runs of the same code,
  because it depends on which request finishes first, so it gets one step of
  slack.
- Requests that finished before the composer showed, and the sum of all long
  tasks, are reported and never budgeted. They move between runs of the same
  code.

Memory is read only after forcing garbage collection twice through the
DevTools protocol, so it measures what the page holds, not what is waiting
to be collected.

Measure memory in headless Chromium or a visible window, never in a hidden
browser pane. A hidden page gets no animation frames, so Svelte's exit
transitions never finish and every closed element stays in the page. That
reads as a leak that is not there.

The server half is a debug build of whatever this checkout last compiled. The
lab is for the web UI. Server timings from it compare only with each other.

## Query plans

`query_plan_tests` in `virtues-core/src/api/chats.rs` plans the chat list
(`LIST_CHATS_SQL`) and the transcript (`TRANSCRIPT_SQL`) with sequential
scans turned off and fails on any `Seq Scan`. Postgres picks a full scan then
only when no index can serve the query, so the test fails when an index is
dropped or the SQL stops matching one. The handlers run the same constants
the test plans. A new query that runs on every launch or every turn should
join this module the same way.

## What these do not cover

- A streamed reply. Nothing in the lab asks a model, so the per-token cost of
  rendering a reply is not measured.
- The relay. Every request in the lab is on loopback, where a round trip is
  nearly free. The waterfall's depth is the number to watch for the phone:
  each step is a round trip over the relay.
- The Mac and iPhone shells: launch time, the `virtues://` handler, the tray.
- GPU work: redrawing the glass's blur as the sidebar or a tile changes
  size. Headless Chromium draws in software, so `sidebar_toggle` holds only
  the main thread's half. Measure the rest in the Mac app with Safari's Web
  Inspector (Timelines, Layers).
- Server memory over a long run.
