# Performance checks

Three checks measure the app's speed and memory the same way on every run.
On one machine, with nothing else heavy running, a number that moves past
its budget means the code changed. Run the check that covers what you
touched before you call a change done. When a number has to go up, update
its baseline in the same commit and say why in the message.

| Check | What it measures | Where it runs |
|---|---|---|
| Bundle budget | What the chat home loads before its first paint | CI, on pull requests to `main` and `staging` that change more than docs |
| Lab run | Six scenarios: cold load, cold load over the relay, a long chat, a streamed reply, the sidebar toggle, and memory across view changes | Locally, by hand |
| Query plans | The hot chat queries are planned without a sequential scan | `cargo test -p virtues` |

## Bundle budget

`tools/perf/bundle-budget.mjs` reads the build's own manifest, follows the
static imports of the `(app)` catch-all route (the route every launch lands
on), and sums the JavaScript and CSS it loads before the first paint.
Dynamic `import()` is not followed, because that is how a library stays off
this path. The baseline is `tools/perf/bundle-budget.baseline`. The check
fails when a number goes over it by more than 512 bytes. A number that goes
well under it is only reported, so lower the baseline in the commit that
earned the saving, or a later change can spend it unnoticed. CI runs the
check, but `staging` has no required checks, so a failure there does not
block a merge on its own.

```sh
cd apps/web && pnpm build && cd ../.. && node tools/perf/bundle-budget.mjs --top 20
```

When it fails, `--top 20` lists the largest chunks on the path. The usual
cause is a static import of something heavy from a module the chat home
reaches. Load it where it is first used, with `import()`.

Measure a new baseline from a scratch clone, never from this checkout. The
shared tree holds other agents' uncommitted edits, and CI reads the commit
(see CLAUDE.md, "Never cut a ratchet baseline from this checkout"). The
script writes the baseline next to itself, so run this checkout's script
against the clone's build, then commit the baseline file here:

```sh
rm -rf /tmp/budget && git clone --local -b wave . /tmp/budget \
  && ln -s "$PWD/apps/web/node_modules" /tmp/budget/apps/web/node_modules \
  && (cd /tmp/budget/apps/web && pnpm build) \
  && WEB_DIR=/tmp/budget/apps/web node tools/perf/bundle-budget.mjs --update \
  && rm -rf /tmp/budget
```

## Lab run

`tools/perf/lab/run.mjs` does the whole run in one command:

1. It builds the web UI from a commit (HEAD by default) in a scratch clone,
   linked to this checkout's installed `node_modules`, and removes the clone
   afterwards.
2. It starts a box from the shared target's `virtues` binary on its own
   database, `virtues_perf_lab`, bound to 127.0.0.1 only, with
   `ENVIRONMENT=dev`, setup skipped, and only `PATH`, `HOME` and `TMPDIR`
   from your shell. The box serves that build from `STATIC_DIR`, the way an
   installed box does. Its model is `fake-model.mjs`, which streams one
   fixed invented reply (242 tokens, one every 15 ms on an absolute
   schedule) in the shape the box reads, so no real model is asked.
3. It seeds invented data (`seed.mjs`): 40 short chats and one chat of 300
   messages with headings, lists, code, tables, reasoning and tool results.
   The generator is seeded, so the data is the same on every run.
4. It drives headless Chromium through six scenarios, each run in a fresh
   browser context. Each scenario runs five times (`--runs`) and keeps the
   median; `view_cycle` runs at most three times.
5. It compares the result with `tools/perf/lab/budgets.json`. Whether the
   run passes or fails, it stops the box, drops the database and removes its
   temporary files, unless `--keep` is given.

One run at a time: a second run refuses to start while `results/lab.lock`
names a live process, because it would drop the first one's database.

```sh
cd tools/perf/lab && pnpm install   # once
node run.mjs                        # measure and compare
node run.mjs --web-build DIR        # reuse a build, skip the clone
node run.mjs --update               # record budgets.json from this run
node run.mjs --keep                 # leave the box and database up to look at
PERF_RUST_LOG=info node run.mjs     # the box's log goes to results/box.log either way
```

| Scenario | What it records |
|---|---|
| `cold_load` | Time from navigation to a visible composer, JavaScript transferred, API requests before ready and in total, the waterfall's depth, long tasks, and memory after garbage collection |
| `relay_load` | A cold load as the Mac and iPhone apps see it. The UI loads at once, and the page sends every box request through a proxy that holds it 100 ms (about a round trip over the relay), using the same `__VIRTUES_BACKEND_ORIGIN__` the shells inject. A held request keeps its connection, so the browser's six connections to the proxy queue as they do to the apps' loopback. Records when the composer is usable and when the sidebar shows the newest chat |
| `long_chat` | The chat draws its latest 60 messages, so this measures that window. Time until the 300th message is in the page, the longest task, memory after garbage collection, and Chromium's layout and style time for three moves: opening the chat, narrowing the window (every drawn message laid out again, as when the sidebar opens beside it), and scrolling to the top |
| `stream_reply` | A message sent in the long chat and the fake model's reply streamed back. Records the time to the first token on screen, how long after the model sent its last token the page showed it, the main thread's script and layout time over the turn, and frames over 20 ms. The first and last tokens are seen with a MutationObserver, not by reading the page's text every frame. Each run starts and ends by deleting any turn after the 300 seeded messages |
| `sidebar_toggle` | Six opens and closes of the sidebar beside the long chat, through the header's ∴ button: main-thread layout, style and script time per toggle (median after two warm-ups) and the most frames over 20 ms in any toggle |
| `view_cycle` | Ten laps through eight views by in-app navigation, and the growth per lap in heap, DOM nodes, listeners and documents between lap 2 and lap 10 |

How each number is held to its budget:

- Counts (requests, documents) and growth per lap are exact.
- Times get 30% headroom with a 50 ms floor. Layout, style and script times
  (`*_layout_ms`, `toggle_style_ms`, `toggle_script_ms`) get 30% with a 10 ms
  floor, because a 50 ms floor would hide a long chat's relayout going back
  to what it cost before its rows could be skipped.
- Sizes (memory, transfer, DOM nodes, listeners) get 5%.
- The waterfall's depth and the toggle's slow frames get one step of slack:
  both depend on timing between runs of the same code.
- Three numbers are reported and never budgeted, because they move between
  runs of the same code: requests that finished before the composer showed,
  the sum of all long tasks, and slow frames during the stream.
- A budgeted number the run did not produce, or a -1 from a wait that timed
  out, fails the run.

Times depend on the machine and on what else it is doing: another agent's
build moves every time at once. Compare times only between runs on one
machine, and to compare two builds, run them back to back more than once and
compare the quiet pairs.

Memory is read only after forcing garbage collection twice through the
DevTools protocol, so it measures what the page holds, not what is waiting
to be collected. `heap_mb` is the JavaScript heap alone. `memory_mb` adds the
DOM's own memory and ArrayBuffer storage, as Chromium counts them. Those
counters did not include the WebAssembly memory of Shiki's former regex
engine (22 MB), so the lab reads WebAssembly memory from the modules
themselves as `wasm_mb`. It is kept apart from `memory_mb`, because
Chromium's ArrayBuffer count may include some modules' memory. `dom_nodes`
counts text and comment nodes as well as elements; most of a chat's are
Svelte's empty placeholders, which take no layout.

Measure memory in headless Chromium or a visible window, never in a hidden
browser pane. A hidden page gets no animation frames, so Svelte's exit
transitions never finish and every closed element stays in the page. That
reads as a leak that is not there.

The server half is a debug build of whatever this checkout last compiled,
whatever `--ref` says. The lab is for the web UI. Server timings from it
compare only with each other.

## Query plans

`query_plan_tests` in `virtues-core/src/api/chats.rs` plans the chat list
(`LIST_CHATS_SQL`) and the transcript (`TRANSCRIPT_SQL`) with sequential
scans turned off and fails on any `Seq Scan`. Postgres then picks a
sequential scan only when no index can serve a table's part of the query, so
the test fails when an index is dropped or the SQL stops matching one. It
does not catch a full scan of an index: the chat list reads every row of
`app_chats` by its primary key, which passes. The handlers run the same
constants the test plans. A new query that runs on every launch or every turn
should join this module the same way.

## What these do not cover

- A real model's stream: its pace varies, it sends reasoning and tool calls,
  and it pauses. `stream_reply` sends plain text at an even pace.
- The relay's real behavior. `relay_load` adds a fixed 100 ms to each
  request; a real relay varies, and loses packets.
- The Mac and iPhone shells: launch time, the `virtues://` handler, the tray.
- WebKit. The lab runs Chromium, which anchors scrolling; WebKit does not,
  and the long chat's scroll handling has a path for that which the lab does
  not run.
- GPU work: redrawing the glass's blur as the sidebar or a tile changes
  size. Headless Chromium draws in software, so `sidebar_toggle` holds only
  the main thread's half. Measure the rest in the Mac app with Safari's Web
  Inspector (Timelines, Layers).
- Server memory over a long run.
