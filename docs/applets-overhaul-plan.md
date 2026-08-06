# Applets — the overhaul plan

> Status: **design locked 2026-07-19, not built.** Supersedes the "actions" framing in [`architecture.md`](./architecture.md) at the concept/UX layer; the execution engine (manifest + reconcile + runner) stays. Decision history and research notes are in the appendices.

## What we are building

**A user-space systemd with an AI author.** One primitive (`app_actions`, one row = one applet), where a sync, a daemon, a dashboard, a reminder, and an AI job are the same row with different fields set. The AI turns chat intent into a small set of flat, per-field-validated values — a prompt, a cron, a SQL condition, an argv, an HTML face — which trusted, already-shipped machinery interprets. It is **not** a coding agent; real code is the power tier (Rust, off-box, git-linked).

The honest technical comps, and what each contributes:

| Comp | What we take |
|---|---|
| **systemd units** | The closest mirror: a unit file IS flat fields — `[Timer]`=cron, `ConditionPathExists=`=our SQL condition, `MemoryMax`/`CPUQuota`/`RuntimeMaxSec`=limits, `Restart=`+`StartLimitBurst`=crash-loop policy, `systemd-analyze verify`=`--check`, `systemctl status`=detail page, `journalctl`=run log. Applets are unit files humans (and models) can write. |
| **K8s CronJob** | The vocabulary we were missing: `concurrencyPolicy` (overlap), `startingDeadlineSeconds` (missed-run), `backoffLimit`. |
| **anacron / systemd `Persistent=`** | Catch-up semantics for a box that was asleep at 6am. |
| **Android WorkManager** | Constraints-as-conditions, backoff policy — the mobile proof that flat declarative job specs cover consumer automation. |
| **IFTTT / Shortcuts / HA** | ~90% of personal-app requests are NL → declarative config, not code. |
| **Claude Artifacts** | Model-authored HTML in a sandboxed iframe is the proven face pattern. |

Chat is the front door: "remind me to X on date Y" / "dashboard of heart rate vs workouts" / "calorie app — I send a photo" / "examen before 6am from my narrative identity" → applet. The ~5% power-author is the same primitive with the hood open.

## The model

| Concept | What it is |
|---|---|
| **Applet** | A thing that runs for you. Folder at `applets/<name>/` (manifest + optional prompt/face/schema/code). |
| **Fields** | Any subset of: `agent` (prompt) · `command` (argv, power tier) · `cron_schedule` · `condition`/`until` (SQL) · `triggers` · face (`index.html`). Combinations compose; nothing to pick. `runtime` is derived (command+supervise=service; face-only=view; else function). |
| **Owner** | `system` (reconcile-managed) \| `user` \| `ai`. The only sectioning; list default-hides `system`. |
| **Lifecycle** | One nullable field: `until` — absent = forever · `"once"` = archive after first success · a SQL bool = archive when true. The enum is a derived display label. |
| **Limits** | Per-applet caps (below). Protective defaults, always user-editable — never locks. |
| **Definition** | On disk, git-able. **State** in Postgres, never on disk. The sorting rule: **changes when it *runs* → Postgres (memory, rows, runs); changes when it's *edited* → folder (prompt, `schedule` seed, `schema.sql` DDL).** Manifest and row are the same serde struct in two homes — TOML is the portable/git-able serialization (the systemd-unit-file / k8s-YAML split), reconcile is the one-way apply, and they share no field they could disagree on. Memory-files-on-disk stay rejected: files-as-memory is for frameworks without a DB; run-state in the folder would churn the git lane per run and split the DB-is-the-backup story. |

**Archetypes are recipes, not types** (cookbook/AGENTS.md, never schema): Reflect = schedule+agent · Rule = trigger+condition+(agent|command) · Sync = schedule+command+credential · Tracker = `schema.sql`+face+agent-writes-entries · View = face+queries · Persona = agent (identity+boundaries+exemplars-from-own-messages)+channel.

**Two cross-cutting conventions:**

- **`description` is the intent-source, not decoration.** Lite-slot compaction distills the authoring chat into one sentence ("each morning before 6, write my examen from my values and yesterday's data"); the preview-gate tap blesses *the sentence*; the fields are compiled from it. Contract: editing the sentence recompiles the fields; manually editing a field updates/flags the sentence (no silent drift). This is also the answer to authoring-chat context inheritance — whatever mattered was distilled into the sentence at creation; nothing else carries over (`chat_id` survives only as the thread route).
- **Ensure-semantics is the blessed recipe for data-product applets** (examen, syncs, summaries): phrase the job as a state — "make sure today's X exists" — check-first, create-if-missing. Idempotency then dissolves the scheduler edge-cases: missed slot → next wake sees it's missing (catch-up), racing runs → second no-ops (overlap), failure → still missing, try again (retry). Moment-anchored effects ("remind me at the right time") stay event-shaped.

### Three-axis semantics

| Axis | Question | Values |
|---|---|---|
| **Wake** | what causes a run attempt | cron · manual · tool · api/webhook · data-event (later: new `data_*` rows, applet-finished via `parent_run_id`) · *always-up* (services aren't woken). Closed set. **Trigger = who wakes you (push, carries the new rows); condition = what you check once awake (poll).** They compose: `trigger=data:data_location` + `condition="speed < 5"`. Cron-poll + condition covers everything data triggers do, at a latency/efficiency cost — which is why data is "later." Webhook = authenticated box API route over iroh/relay; no public-exposure question. |
| **Gate** | does the attempt proceed | none · deterministic SQL `condition` — **local state only, never network I/O**. A gate that fetches is a run (latency, failures, cost). "If endpoint says X then continue" decomposes: the run fetches-and-decides (→ `skipped`), or a sync applet lands the endpoint's state in a table the gate reads. Fuzzy judgment stays OUT of the schema — "when appropriate" lives in the agent prompt. |
| **Life** | when is it done | one nullable `until` field: absent = forever · `"once"` = first success · SQL bool = archive when true (same `SELECT (expr)` evaluator as `condition`, checked after each success → `archived_at`). Second completion channel: the run can declare itself complete (agent judges "delivered, done"). |

Worked example — "remind me once today, when appropriate, to pray for X": wake=cron poll, gate=none, prompt=judge the moment/deliver/skip, life=`once`. The fuzziest consumer ask, zero new machinery.

### Schema migration (small)

```
app_actions: keep agent/command/config as-is
  DROP runtime, dir                -- both derived (fields set; folder path)
  owner: + 'ai'
  ADD until TEXT (NULL=forever | 'once' | SQL bool), archived_at
  config = THE one JSONB: limits.*, chat_id (thread route), supervise, model (slot), fetch allowlist…
           -- optional, defaulted, never queried relationally; typed serde struct on read.
           -- Columns only for what the scheduler indexes. ({view:{name}} dies with iframe faces.)
  triggers: accept objects later ({"data":{"table":…}}), bare strings stay as sugar
  manifest key: default_cron → `schedule` (phase 1) — seeds the live SQL value per field-ownership
  fan-out: system syncs keep per_credential templates; user/ai applets are CONCRETE (one row,
           single optional credential_id); multi-cred access = granted credentialed TOOLS, not rows
  memory: KEPT (persistent UNstructured data — scratchpad/cursors/prose), distinct from
          applet_<slug> tables (persistent STRUCTURED data); bounded per thread 6
app_action_runs: unchanged (parent_run_id/transform_stage already wait for chaining)
```

### Limits (the systemd checklist)

| Limit | Enforced where |
|---|---|
| `max_llm_cost` per-run/day | Gateway/wallet — helpers *propagate* (`inference()` auto-tags `action_id`), wallet *enforces* (hard-stop, surface in needs-attention). Never helper-enforced (opt-in = bypassable). |
| `max_ram`, `cpu_weight`, `timeout` | `systemd-run` jail (`MemoryMax`, `CPUWeight`, `RuntimeMaxSec`) — background work shouldn't starve the box. |
| `max_storage` | `pg_total_relation_size` over the applet's schema + folder size. |
| `max_runs` per hour/day | Scheduler — mandatory before data triggers light up composition loops. |
| `retry` | Flat object: `maxAttempts`, backoff `factor`, min/max, jitter (Trigger.dev vocabulary). |
| overlap — **not a knob, a doctrine** | Every applet is a singleton. A wake attempt during a live run records `skipped (already running)`. The one legit exception is a UI action, not config: "Run now" during a stuck run offers cancel-and-restart. (`allow` is never right for personal automations — racing syncs = duplicates.) |
| `catch_up` | bool, **defaulted from schedule shape**: daily-or-less-frequent → true (examen, weekly review must happen); hourly-or-more → false (the next tick covers it). Guardrails: catch up **at most once** (anacron semantics, never replay missed slots) and **same-period only**. "Too late in the day" needs no field — the SQL gate composes: `condition = "extract(hour from now()) < 12"`. |
| `crash_loop` | services: auto-disable + needs-attention after N restarts in window (`StartLimitBurst`). |
| runs retention | Cap run-history rows per applet; prune (the runs table must not grow unboundedly). |

Defaults per recipe; the preview gate shows estimated recurring cost ("~$0.12/day") — cost is a capability. Scheduled applets default to the cheaper slot (slot doctrine, no model literals).

## Faces = sandboxed-iframe HTML

One primitive: `face = index.html` in the folder (or the service's own URL — the port proxy already fronts it), rendered in a sandboxed iframe (`sandbox` + CSP = browser-grade jail for free). The box injects **`virtues.css`** (theme via CSS vars — light/dark for free) and **`virtues.js`** (scoped read-only `query(sql)` bridge with the applet's token). HTML/JS is the most in-distribution artifact a model produces (Claude Artifacts / Grafana / HA converged here). **Svelte is the app; iframe-HTML is the applets** — builtins' native views stay in the bundle; the boundary is trust. List thumbnails = the same iframe rendered small (gallery for free). Costs accepted: slightly less native feel; props-into-audited-components demoted to a maybe-later polish lane.

## Applet-owned tables (Trackers for free)

Each applet may own a **Postgres schema** (`applet_<slug>`), declared as an optional idempotent `schema.sql` in its folder, applied by reconcile. Schemas, not prefixes: `DROP SCHEMA … CASCADE` cleanup (grace period), per-role `GRANT` hardening later, `pg_total_relation_size` quota, and PG's transactional DDL lets `--check` dry-run in `BEGIN…ROLLBACK`. Killer feature: **joins against `data_*`** (calories × workouts — the box IS the data). Rejected: SQLite-in-folder (state-on-disk breaks the doctrine; second engine; data island that can't join).

## Authoring — a lite harness, ours (~80% assembled)

Claude Code's lesson: files + exec + a typechecker that talks back. The applet equivalents:

| Claude Code | Applets | Status |
|---|---|---|
| Write/Edit | file tools scoped to `applets/<name>/` | build |
| compiler/LSP | `reconcile --check` — per-field validators (cron parser exists; SQL via `EXPLAIN`; DDL via `BEGIN…ROLLBACK`; binary/HTML file-exists) | add `--check` |
| run tests | `applet run <name>` | exists (Run now) |
| read failure | run row / logs | exists |
| few-shot corpus | the ~20 builtin folders | free |
| CLAUDE.md | `applets/AGENTS.md` + authoring skills | build |

Loop: write-TOML → check → run → read error → fix. **The highest-leverage investment is error-message quality — the LSP of this system.** Every error names the fix (the reader is a model in a retry loop): `unknown table "data_helth_sleep" — did you mean "data_health_sleep"?`. "Lite" = no repo-wide navigation, no multi-file refactoring, no LSP farm; the full harness (Claude Code in terminal) is the power tier.

- **`setup_action` becomes sugar**: write the folder → check → reconcile. One door for builtin/git/chat applets; chat-authored become diffable/git-able/portable for free; the direct-to-Postgres path dies.
- **The manifest IS the preview gate**: no separate artifact — render the TOML prettily with two annotations (capability grants derived from filled fields; estimated cost/day), one tap to confirm. The user approves the thing that will exist on disk.
- **The applet is a correspondent — reply to iterate.** Every applet owns a thread; runs land as messages; **replying iterates it** ("shorter tomorrow" → authoring loop edits the manifest, diff visible). Collapses output-sink + editing-surface + v2-messaging into one existing surface: when iMessage lands, the applet just becomes a contact. Detail-page run log and the thread are the same object.
- **Schema-grounding is a context file**: materialize the data catalog (tables/columns/3 sample rows) for the authoring agent to read like AGENTS.md.
- Patterns held from the agent research: AGENTS.md as the portable convention (fix the `connect_from_env` doc drift); skills with progressive disclosure (an applet IS a skill's shape); CLI tools over MCP; targeted SEARCH/REPLACE edits with reflect-on-mismatch retry; shadow-git checkpoints as the undo/autonomy substrate; files-as-memory, no vector framework; **authoring model = hosted frontier via gateway/slots** (resolved).

## The stack — one language per layer, nothing bespoke

| Layer | Language | Built where |
|---|---|---|
| Wiring | TOML manifest | — |
| Gates | SQL (`condition`, `until`) | — |
| Judgment | the agent prompt (LLM is the interpreter) | — |
| Faces | HTML/JS in sandboxed iframe | on-box, no compile |
| Tables | `schema.sql` (PG DDL) | applied by reconcile |
| Real code | Rust + `virtues-helpers` | off-box: **applet template repo** (cargo-generate scaffold + GH Actions cross-compiling aarch64/x86_64; git-link pulls artifacts). The box never builds. |

**Rejected**: sandboxed-Python hatch (foreign runtime for a gap that doesn't exist: predicates→SQL, fuzzy→prompt, code→Rust; deferred fallback if ever needed = embedded QuickJS); Rust toolchain on the appliance; cloud-compiling AI Rust (source leaves the box); runtime-loaded Svelte (superseded by iframes).

## Permissions

Prompt on the irreversible, external, credentialed; auto-allow the reversible, local, sandboxed (Anthropic: sandboxing cut prompts 84%; 93% approval = prompts are noise; habituation is a security failure). Two axes (Codex): sandbox level × approval policy, with **`on-failure`** (escalate only when the jail actually blocks) as the elegant default. The declarative 90% are inert validated fields — nothing to sandbox; faces get the iframe jail; the `systemd-run` jail is for untrusted binaries (git-imported, `owner∈{ai,community}` code). Interrupt only at **four boundaries**: (1) granting a credential, (2) enabling a schedule/trigger, (3) side effects that send or spend, (4) promoting sandboxed→trusted. Capabilities are **derived, not declared** for declarative applets — 100% computable from the filled fields (`credential_id` → touches that account; `schema.sql` → writes own tables; face-only → read-only), so authors never write them; the preview gate derives and displays them as the consent artifact, granted once, reviewably. Declaration exists only for **power-tier binaries** (opaque code must state its egress hosts, and the jail's proxy enforces the declaration). Derived-for-display, declared-for-enforcement. Hard rules: **enforce outside the model** (never the LLM's self-assessment); **no shell denylists** (Cursor's was bypassed 4 ways then removed) — remove broad capability, expose narrow typed tools, enforce at the OS layer, keep hardcoded circuit-breakers. Egress for jailed code = localhost proxy over a Unix socket that injects the one credential and allowlists the one host; the secret never enters the sandbox.

## UI surfaces

- **List**: one flat list, owner-sectioned Yours/Built-in (`system` hidden by default), no sub-tabs, no runtime column. Row = face-thumbnail-or-glyph, name, plain-English line, last activity, on/off, run-pulse. **Needs-attention strip** on top (errored / expected-but-didn't-run / credential-expired) — failure UX v1, no new alerting infra. `+ New` points at chat. Surfaced info, never enforcement: "14 applets running, ~$0.9/day."
- **Detail**: header (name · on/off · last ran · Run now) → its face (headless = run log/thread) → the guts (schedule, triggers, limits — all editable; memory as "notes this applet keeps", bounded via compaction; source in CodeMirror). Built-ins get an explicit "managed by the system" state instead of a vanishing delete.

## Git / distribution (resolved 2026-07-19: fork-on-edit + ownership-aware push-back)

One mechanic, no reconciliation problem: **all edits — user's or AI's — land as commits in the box-owned lane; imports are never edited in place**, so clobbering is structurally impossible. An untouched import hard-resets on upstream update as today; touching one forks it into the box-owned lane with `forked_from = <url>@<sha>`. Upstream updates to a forked applet show a diff + the authoring loop offers a rebase. Ownership decides the *affordance*: if the remote is the user's (push rights), the detail page offers an explicit **"push changes upstream"** (a publish — never automatic); third-party remotes just stay forked. Shadow-git remains the agent's private undo store, invisible to both lanes. Cloned = untrusted → jail. Distribution = git URLs, no registry (sharing/app-store = v2).

## Sequence

0. **Done (2026-07-19)**: `setup_action` contract drift fixed (phantom `endpoint`/`activation_code` removed; `agent` canonical; `triggers`/`condition` advertised); id = `action_agent_<chat_id>_<slug>` (multiple applets per chat). *Committed entangled in `e62e4e87` on `feat/box-safety` — split out.*
1. **Rename + collapse** — Applet everywhere; `actions/` → `applets/` folder (do it now: git-lane paths must be stable before anyone links repos); one list + needs-attention strip; one detail page; lifecycle + archive-on-completion; drop `runtime`/`dir`.
2. **On-box authoring unlock** — iframe face runtime (`virtues.css`/`virtues.js`, scoped tokens); sandbox routing for untrusted binaries. **Dogfood proof: demote `morning_examen` (then `day_summary_eod`) from Rust to manifest+prompt** — if the flagship can't be expressed in flat fields, the schema is wrong and we want to know now; deletes code; becomes the canonical few-shot example.
3. **The authoring loop** — scoped file tools, `--check`, AGENTS.md + skills, manifest-as-preview-gate, limits enforcement (gateway + jail), correspondent threads (reply-to-iterate).
4. **Data triggers + wire the dead chaining** — the composability unlock (`parent_run_id` machinery lives).
5. **Git box-owned lane** — commit-back, rollback, reviewable diffs.
6. **Cleanup** — memory bounding, trigger/condition vocabulary sweep, AUTHORING.md→AGENTS.md + doc-drift fix.

## Open decisions

**None.** Every question raised through 2026-07-19 is resolved — see Resolved below, the judgment calls in Appendix B, and the Git section. Remaining work is build, guided by the Sequence and the Appendix B findings register (bugs + amendments).

*(Context inheritance: resolved by the intent-source contract. Git two-lane: resolved by fork-on-edit.)*

---

## Appendix A — decision log

- **Name: `Applet`** (re-litigated and confirmed 2026-07-19). Namespace picked over (IFTTT Applets, Claude Artifacts, Alexa Skills/Routines, Apple Shortcuts, HA Automations). Rejected: Artifact (inert output, not a runner), Instruments (best semantics — means-of-action + instrument-panel + document-that-enacts + Rule of St. Benedict ch.4 "Instruments of Good Works" — but too long), Daemons (techy/demons), Keeper/Steward/Familiar (personhood fails for dashboards), Practices (fails machinery; the gallery it was reserved for is cut — 2026-08-05). Persona-panel finding: engineers name the mechanism, contemplatives the meaning, laypeople only the instance — no word wins all camps; the noun just has to be unembarrassing in the nav. "Vigil" reserved as UI copy for watcher-shaped applets. ~~`action`/`app_actions` stays the code word.~~ **Superseded 2026-07-29:** the rename went all the way into the code — `app_applets`, `app_applet_runs`, `applet_runner`, `applet_templates`, `AppletInput`/`AppletOutput`. No `action` identifier survives in core.
- **Tagged-enum `spec` REJECTED** — a 7-arm union is a bespoke DSL (extend enum+validator+interpreter+migration per new shape); models are native at lingua francas, mediocre at bespoke schemas. Flat fields won. (Letta's v1 walked back its bespoke mechanisms for exactly this reason: scaffolding should stay "in-distribution.")
- **Consumer-lens filter** — REJECTED auto-pause-on-inactivity ("inactivity" undefined for a headless sync; nothing the user turned on turns itself off); DEMOTED count caps to surfaced info (they're plan-tier monetization); every limit user-editable.
- **Product layer** — chat is the front door (no catalog-first bet, no separate builder); owner system|user|ai; failure UX v1 = needs-attention strip only; last-mile delivery v2 (iMessage/text via virtues-helpers, not APNs-first); out of scope: persona ethics, duty-of-care framing, sharing (v2), output surfaces beyond chat/pages (v2).
- **Faces** — inverted-trust props-into-components demoted; sandboxed-iframe HTML adopted (2026-07-19).
- **Keystones kept** — schema-grounded generation (Glide); per-call model routing `box_local|hosted_frontier` as a spec field (Apple "Use Model"); Persona = character sheet + channel (Character.AI / eve), exemplars auto-drawn from the owner's messages.

## Appendix B — three-agent review findings (2026-07-19, status: proposed unless marked)

Three independent agents (red-team vs code · simplicity audit · writability test authoring 10 real manifests, scored 6.5/10 → 8-9 reachable with zero schema changes).

**Code-verified bugs (fix regardless):**
- `eval_condition` decodes `Option<i64>` — every boolean SQL gate (incl. the plan's own examples) errors instead of skipping; runs unhardened on the core role, no READ ONLY/timeout, `now()` = UTC not local. Fix: boolean decode, `READ ONLY` + `statement_timeout` + restricted role + `SET LOCAL timezone`.
- Runtime agents get the FULL toolset incl. `setup_action`/`delete_action`/`sql_query` — an ai-owned applet can mint new scheduled applets, bypassing all four permission boundaries. Fix: capability-derived per-applet tool allowlist; strip applet-management tools by default.
- Condition/concurrency skips create NO run row (contradicts the singleton doctrine's `skipped` claim); no `next_due_at`/`last_slot_at` anywhere → catch_up and "expected-but-didn't-run" are uncomputable; DST/tz offset frozen at job registration. Fix: persist skip rows + slot bookkeeping + re-register on tz change.
- Migration as written doesn't compile: `supervise` flag never ADDed; runner/scheduler branch on `runtime`; `subprocess_timeout` keys off `dir`. Fix: add `supervise`, generated/derived column strategy before DROP.
- Reconcile GC hard-DELETEs fan-out rows on credential blips (`reauth_required` destroys `archived_at`/memory/cursors). Fix: soft-disable for recoverable states.

**Convergent design amendments (2+ agents agree):**
- **Correspondent threads: defer full reply-to-iterate to v2** (simplicity: duplicates surfaces; writability: reply-as-input vs reply-as-edit collide; red-team: no schema substrate, id-scheme conflict, context-creep). Salvage the load-bearing part: **add a `message` wake** (post to applet thread → run with message as payload — the calorie tracker's front door) with the clean routing rule: *applet thread = I/O only; edits happen in detail page / main chat.*
- **Agent-runtime capability table is the #1 missing contract** — exactly what a declarative agent can do: read `data_*`, write own schema, post to thread, fetch URLs (grant the domain-allowlisted fetch the Permissions section already prescribes), scoped credentialed source tools (gmail-archive etc. — email rules are the canonical consumer automation and currently have no path). Belongs in AGENTS.md + enforced allowlist.
- **`--check` can't see agent prose** — imaginary tables in prompts fail soft forever. AGENTS.md doctrine: catalog-check-first, "you are the check," honest downgrades (decline + offer manual tracker).
- **Intent-sentence: one-way for v1** (provenance + staleness flag; recompile-on-edit deferred — nondeterministic recompile clobbers hand-tuned fields; needs per-field `compiled|user-pinned` provenance if ever bidirectional).
- **`catch_up` × time-of-day gate annihilate** (wake-at-7 catch-up killed by before-6 condition — examen silently skipped). Rule: a clock-time condition also gates catch-up; widen the window, don't tighten the cron.
- **schema.sql is CREATE-only — Tracker breaks on first edit.** Numbered append-only migration files per applet, applied-set tracked.
- **Face security hardening is a Phase-2 prerequisite, not "later"**: per-applet PG role + default-deny grants (the current sql_query "starts with SELECT" check is the denylist anti-pattern the plan itself forbids); service faces must not be same-origin with the box API; thumbnails = cached static snapshots, not N live token-holding iframes.
- **Persona: mark "v2 — not yet authorable" in the cookbook** (no inbound wake, no channel, no send capability — a model following the recipe today authors dead manifests).
- **Preview gate only when a boundary is crossed** (credential/spend/recurring-LLM-cost); a gate on a $0 read-only view is the habituation the Permissions section warns about.
- **Limits diet for v1**: `max_llm_cost` + `timeout` as fields; retry/catch_up become doctrine-defaults (no field); `max_runs`/`crash_loop`/`max_storage` arrive with the phase that needs them. Add later: `cooldown` (writability found it smeared across prompt text + max_runs hacks). `budget_exceeded` = distinct run status; propagate `run_id` in gateway tags; show estimated-vs-actual cost on detail.
- **Sequence: examen demotion moves to the front** — it needs no iframe/threads/git; if the flagship can't be flat-fielded, everything downstream is rework.
- **Upgrade/restart breaks singleton** for jailed/supervised work: deterministic unit names (`virtues-applet-<id>`), adopt-or-kill against systemctl at startup before reaping, drain step in upgrade preflight.

**Judgment calls — all resolved 2026-07-19 (user):**
- `memory` column **KEPT** — persistent *unstructured* data (scratchpad/cursors/prose), distinct from `applet_<slug>` tables (persistent *structured* data). Auditor's cut rejected.
- Fan-out: system syncs keep `per_credential` templates; **user/ai applets are concrete** (one row, single optional `credential_id`); multi-cred access = granted credentialed tools.
- `limits` **merged into `config`** — one JSONB (limits.*, chat_id, supervise, model slot, …).
- Manifest cron key → **`schedule`** in phase 1 (seeds the live SQL value).
- Git two-lane: **fork-on-edit + ownership-aware push-back** (see Git section).

## Appendix C — research notes

- **Caps research (2026-07)**: OpenClaw = observability only, budgets feature request closed *not planned*, guidance "cap at your provider"; Hermes = four spending lanes for diagnosis, no app-layer limits. Both punt because they don't own a provider; **Virtues owns provider + runtime → hard native caps are a differentiator.** OpenClaw's HEARTBEAT.md: "keep it short — every tick costs tokens."
- **Deep-research pass (verified 3-0 unless noted)**: Vercel eve — agent IS a directory of named slots (`instructions.md` + optional `tools/ channels/ schedules/`), presence activates capability, validate-before-serve = our `--check`; channel secrets outside the definition; no NL-authoring loop (repair = observability + eval gates); no decay story. ChatGPT Tasks — conversational authoring precedent; context-firewalled from authoring project; monitoring tasks = poll + run-to-run diff + notify-on-change + **end condition** (→ our `until`). Gemini — 10-task cap, silent auto-disable. **ChatGPT Pulse (sunset 2026-06-17)** — proactive agent steerable only by free-text with no inspectable artifact → walked back into editable scheduled tasks; the visible field-based artifact IS the product (its good idea survives: ephemeral output promoted by engagement). Claude Code SKILL.md — all-optional fields, two independent booleans over a type enum. Inngest — triggers unified as one list; flow control as flat fields (throttle enqueues, rateLimit drops). Trigger.dev — flat retry object. Cloudflare — schedules as rows beside the agent. HA practice — LLMs "stubbornly wrong" on fragmented per-card syntax; fix = ground in live inventory + small uniform schema.
- **Reference agent designs**: Goose (Rust core mirror; don't copy tools-as-MCP), Codex (Landlock+seccomp+systemd-run shape), Cline (shadow-git), Aider (SEARCH/REPLACE+reflection). Sandbox: no Docker/microVM/Firecracker — `systemd-run` + Landlock + seccomp, accept shared-kernel residual for a single trusted user.
