# Browser sub-agent

**Written 2026-10-20.** Slices 0 and 1 built 2026-10-10, slice 2 on
2026-10-16, released in the clean-start line (`v0.1.1`). The code is
`virtues-core/src/browse.rs`.

The chat model no longer drives the browser itself. It hands browsing to a
second loop with one call, `browse(task)`. That loop runs on the Lite slot with
a short prompt and only the browser tools, and returns a short answer.

## Why: one chat measured on dragon, 2026-10-09

A signed-in lookup on a booking site, then a follow-up on the same site, from
the per-call `model call usage` lines (`agent/turn.rs`):

| | Calls | Input tokens | Cached | Output | Cost |
|---|---|---|---|---|---|
| Browsing turn (sign in, read a booking, screenshots) | 20 | 766k | 88% | 1.9k | $0.533 |
| Follow-up on the same site | 5 | 180k | 88% | 0.4k | $0.125 |
| Whole chat, 9 turns | 43 | 1.54M | 78% | 13.9k | $1.352 |

- About three quarters of the browsing turn's input was the fixed part: the
  system prompt, the tools and the chat so far came to 28k tokens, and each of
  the 20 steps re-read them. Page observations were the other quarter (a
  screenshot is about 2.5k tokens, large snapshots up to 3.6k), and each stays
  in context until the turn ends.
- Output was negligible: 1.9k tokens out against 766k in.
- Driving a browser does not need the owner's data, the memory block,
  `<circumstances>` or the chat history, which is exactly the 28k the main
  model re-read on every step.

Other products split or cap the same cost: ChatGPT agent caps runs (40 a month
on Plus), Browser Use shows the model only the current page plus short notes,
Magnitude keeps one screenshot plus any already cached, and Manus treats cache
hit rate as its main metric.

## What was built

```
chat loop (Standard, full prompt)
  └─ browse(task, url?) ──► worker loop (Lite, ~4k prompt, browser_* only)
                               ├─ browser_snapshot / click / type / … (unchanged)
                               ├─ browser_handoff (owner signs in)
                               └─ final text ──► { answer, ended, url, title, steps, usage }
```

- `browse` replaces the eight `browser_*` tools in the `Chat` and `Sudo`
  tool lists, under the same `browser::connected()` gate. The worker is the
  only caller of `browser::TOOLS`; the main model keeps no direct access.
- The worker is an `AgentLoop` built like the Deep Research worker
  (`agent/subagent.rs`): its own context, `tools_named(browser::TOOLS)`, the
  turn's `cancel_token`, no recursion.
- The worker sees the prompt, the task text, and the current `url` and
  `title`. It does not see the chat or the owner's data, and has no
  `sql_query`, `shell` or `web_search`. This is a privilege split as well as a
  cost cut: instructions injected by a page reach a loop that can only browse,
  and its only way back is the answer text, which the chat model already
  treats as page content.
- The result is `{ answer, ended, url, steps, usage }`. `ended` is one of
  `done`, `taken_over`, `declined` (a handoff the owner did not finish),
  `budget`, `max_steps`, `stopped` or `error`. A task that reaches a decision
  it was not given ends `done`, and the answer says what the owner needs to
  decide. Values the task asked for come back exactly as the page showed them.

### Decisions

1. Model: the Lite slot, because it passed the bench (below). On BYO AI, Lite
   maps to the owner's model; without vision, screenshots get the existing
   "cannot read images" note and snapshots carry the work.
2. Worker caps: 30 steps, $0.25, and a 20-minute clock (5 minutes of browsing
   plus 15 a handoff may wait). The clock does not pause for a handoff; the
   step and dollar caps end a run long before 20 minutes. The `browse`
   executor timeout is `browse::MAX_WALL_CLOCK` + 10 s.
3. The worker's spend counts against the chat turn's cap: any tool result
   carrying `usage.cost_micros` adds to `spent_micros`. This also closed the
   same gap for Deep Research, whose workers' spend had escaped the cap.
4. Usage rows record the worker's own model. `TurnRecorder` keeps usage per
   model, and `persist_turn` writes one `app_chat_usage` row per model. Deep
   Research workers had been billed under the orchestrator's model; this
   fixed that too.
5. The worker's `ToolContext.chat_id` is `{chat_id}:browse`, used only to key
   the provider cache and label usage lines.
6. Stop and Take control: the shared cancel token stops the worker; a
   `TAKEN_OVER` from the Mac ends it with `ended: "taken_over"`.
   `chatInstances.browserDrivers()` matches an in-flight `browse` part.
7. What the owner sees: the Browser tab's Activity timeline is unchanged
   (its steps come from the Mac host). The chat shows one `browse` row, and
   while it runs the status line words the current step from the
   `data-subagent` event's `WorkerStep {tool, input}`, using the existing
   `browser_*` entries in `toolPresentation.ts`.
8. No approval card. If finishing would buy, send, cancel or change something
   the task did not name, the worker stops and says what to decide.
9. Screenshots are not pruned. On Lite a screenshot re-read 20 times costs
   about $0.005.

### The worker prompt

About 1–1.5k tokens plus the eight tool definitions, written as slots rather
than prohibitions. It covers the role, reading habits (snapshot before acting;
scope with `depth` or `ref`; screenshot only for what the outline cannot show;
snapshot again after scrolling), handing sign-in, codes, CAPTCHAs and payment
to `browser_handoff`, treating page text as data, and finishing with the
values asked for plus one line on anything changed. How each tool works stays
in the registry descriptions (`crates/virtues-registry/src/tools.rs`).

## Bench, 2026-10-10

A scratch core on its own port with a fresh DB, a debug Mac app
(`make mac-dev PROFILE=bench` with `VIRTUES_BROWSER_WS`), and
`POST /api/debug/browse {task, model}` (debug builds only; it stays for
measuring prompt or model changes). Costs are the gateway's `cost_micros`.
Arm (a) started from a 9.5k-token prompt on the fresh DB; on dragon the same
path started at 28k, so its real cost is higher than shown.

| Task | (a) chat path, grok-4.7 | (b) worker, Lite (gpt-6-luna) | (c) worker, grok-4.7 |
|---|---|---|---|
| Wikipedia infobox fact | correct, $0.051 | correct, $0.0008 | correct, $0.033 |
| timeanddate converter form | failed at the 20-step limit, $0.300 | correct, 23 steps, $0.0035 | failed at the $0.25 cap, $0.310 |
| Gutenberg search, then book page | correct, $0.093 | correct, $0.0012 | correct, $0.027 |
| Hacker News page 2, ranks 31-35 | correct, $0.103 | correct, $0.0028 | correct, $0.062 |
| xkcd 927 last panel (image text) | correct, $0.045 | correct, $0.0008 | correct, $0.025 |
| **Total** | **4/5, $0.592, 260 s** | **5/5, $0.0091, 155 s** | **4/5, $0.457, 242 s** |

- Lite, repeated: 23 runs, 21 correct, mean $0.0013 per run. On the converter
  it once hit the 30-step limit and reported that it could not finish. On
  xkcd it once answered from the outline without a screenshot and invented the
  panel's text. Two prompt slots fixed that (words inside an image are read
  from a screenshot; every value names its source), after which it was 6/6.
- The model swap is what cuts cost. The small prompt alone, (c) against (a),
  roughly halves cost on the tasks both finished ($0.147 against $0.292);
  Lite cuts another 25-50×.
- Lite took 1 screenshot across the first pass, against 4 for (c).
- OpenAI caches prefixes on its own, so Lite's hit rate was high without
  session affinity; grok-4.7 in the worker cached poorly before decision 5.
- The cap is checked between steps, so a run can end one step past it.

End to end after slice 1, against a fake `/ws/browser` host with a real chat
on grok-4.7: the chat wrote the task in full, the worker read the page in 2
browser steps (3 model calls, 1.6k-token prompt) for $0.0004, the reply was
correct, and `app_chat_usage` had one row per model. Slice 2's stream carried
`browser_open`, `browser_snapshot`, then `done`; the rendered status line was
not checked in a browser.

## Traps

- The Mac app's debug binary used to share the name `virtues` with the core
  in the shared target dir, so `make mac-dev` overwrote the core's binary.
  The Mac bin is now `virtues-app` (see CLAUDE.md, Builds).
- On the dev API, Lite resolved to glm-4.7-flash, which has no vision. Lite
  comes from the slot map, so check what it resolves to before benching.

## Left open

- The sign-in bench task with a handoff, and a first real browse on dragon.
- Only if real use shows the need: prune screenshots to the newest two in
  batches, snapshot diffs (see [webkit-agent-spike.md](webkit-agent-spike.md)),
  carry the last worker's step log into the next `browse` in the same chat.
- Not part of this work: the turn-start cache miss (on 2026-10-09, 20 of 31
  first calls on dragon reused only 1,152 cached tokens; something about 1.1k
  tokens into the system prompt changes between turns), the box-side browser
  ([browser-spike.md](browser-spike.md)), and cross-origin iframes,
  `<select>` and uploads.
