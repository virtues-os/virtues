# Applets — the overhaul plan

> Status: **design, not built.** Captures decisions locked in discussion + open questions still to resolve. Supersedes the "actions" framing in [`architecture.md`](./architecture.md) at the *concept/UX* layer; the execution engine (manifest + reconcile + runner) stays.

## Thesis

The backend already did the hard consolidation: **one table (`app_actions`), one primitive.** A sync, a daemon, a dashboard, and an AI job are the same row with different fields set. Almost everything wrong today is the *concept and UX* stretched across that one primitive — a weak name, an ugly generic detail form, a four-way runtime taxonomy in the user's face, and an authoring path that can only make scheduled prompts.

The fix is **more consolidation, not a split.** One noun, one list, one detail page, one authoring loop, one blessed stack, one distribution mechanism, one safety layer. What we're building, stated honestly:

> **A supervisor for small local programs + an intent→spec compiler that writes them.** The AI turns natural language into a small, typed, *validated spec* (a serde Rust enum over ~5 archetypes), which trusted, already-shipped Rust+Svelte machinery interprets. It is **not** a coding agent. A real coding agent (Claude Code in the terminal, or git-link) is the **power-user tier only**.

The scope *is* the advantage, and it's bigger than "bounded codegen." The research (IFTTT, Zapier, Apple Shortcuts, Notion/Airtable, Glide/Retool, Val Town, v0) proves that ~90% of personal-app requests are **natural-language → declarative config, not code.** So the common case emits *data* (a rule / a dashboard spec / a prompt / a template binding), not a program — which means no compile, no build-toolchain wall, and mostly nothing to sandbox. Real code is confined to three narrow escape-hatch slots (a filter predicate, a bespoke sync transform, a novel view) and the power tier.

### The archetype catalog (the "what")

| Archetype | Example | Spec is… | Real code? |
|---|---|---|---|
| **Rule** (trigger→action) | "when Mom texts during work, auto-reply" | trigger + condition + action | No — IFTTT-style |
| **Reflect** (scheduled) | "daily examen from my data" | schedule + data query + prompt + output sink | No — a prompt |
| **View** (dashboard) | "custom homepage of xyz" | data query + component choice + props | No — inverted-trust generative UI |
| **Tracker** (CRUD) | "track my calories" | schema + form + views | No — template |
| **Sync** (pipeline) | "pull my X" | source + field-map | Only for novel transforms |
| **Persona** (acts-in-my-voice) | "reply to Mom as me when busy" | identity + boundaries + exemplars-from-my-data + channel | No — a character sheet |

Applet = a **typed Rust enum of archetypes**, each a small serde spec, validated/repaired on parse. The artifact is inert data the box owns, inspects, diffs, versions.

### Keystones the reframe unlocks

- **Views by inverted trust (v0/generative-UI's real lesson):** the model emits *(query + component choice + props)*, never Svelte. The code stays in the audited component library (`MovementMap`, timeline, charts — already shipped). This **dissolves the build-toolchain wall and the view-sandbox problem at once** — a dashboard is props rendered at runtime, no compile, no rebuild, no jail. Don't load views; render specs.
- **Schema-grounded generation (Glide's move):** generate against the box's *real* `data_*` tables/columns/sample rows. The box *is* the data — the structural advantage no SaaS tool has.
- **Per-call model routing (Apple's "Use Model"):** a first-class spec field, `box_local | hosted_frontier` per step; default local, escalation explicit and visible.
- **Persona = a character sheet, not a program (Character.AI):** identity + boundaries + **exemplars auto-drawn from the owner's own messages** ("show, don't tell"), bound to a **channel** (the one concept worth stealing from Vercel eve). "Auto-reply to Mom" is the smallest instance.

## Decisions locked

- **Name: `Applet`** — one word, unit and thing. No two-tier "automations vs artifacts" split (that re-drew a line the schema erased). No separate word for the collection. `action`/`app_actions` stays as the internal/code word; **Applet** is the user-facing noun everywhere.
- **One primitive, one flat list**, sectioned only by `owner` → **"Yours" / "Built-in."** That's a filter, not a taxonomy of kinds.
- **`runtime` stops being a type the user picks.** It collapses into two orthogonal *properties* every applet may have: does it do **background work** (scheduled / persistent / triggered) and does it have a **face** (UI). Zero, one, or both. `function` vs `service` is just a lifecycle knob; `view` is just "has a face."

## The model

| Concept | What it is |
|---|---|
| **Applet** | A thing that runs for you. Folder at `actions/<name>/` (manifest + optional code/face). |
| **Background work** | Optional. Cron / persistent service / trigger-driven. (subsumes today's `function` + `service`) |
| **Face** | Optional. A rendered UI. (subsumes today's `view`) |
| **Owner** | `system` (built-in, reconcile-managed) or `user` (yours). The only sectioning. |
| **Definition** | On disk (manifest + source). Git-able. |
| **State** | In Postgres (enabled, schedule, runs, memory). Never on disk. |

## The seven threads

### 1. Listing page
Kill the `Actions / Templates / History` sub-tabs. **One flat list**, owner-sectioned (Yours / Built-in). Drop the runtime column. Each row: a glyph or a **live thumbnail of its face** if it has one, name, one plain-English line, last activity, on/off, a run-pulse. Faced applets render richer; headless ones are a status row — gallery and table in one list, no mode switch. `+ New` becomes primary and points at chat.

### 2. Detail page
One template that **degrades gracefully, view-first.** Header (name · on/off · last ran · Run now). Then **its face** — a dashboard fills the canvas; a headless applet's "face" is its run log / last output. Below/behind: the guts — schedule, triggers, credential, memory, and **the source in a CodeMirror editor** (reuse the Pages editor). "Edit" reveals the source; this is where AI iterations land. Delete stops vanishing on built-ins → explicit "Built-in — managed by the system" state.

### 3–5. The authoring engine (chat + coding-agent + sandbox = ONE project)
- **Don't adopt opencode / OpenHands / pi wholesale.** They're general-purpose harnesses with heavier (Docker) sandboxes than we need. Level up the **existing loop** (`agent/mod.rs`, already "production-ready").
- **Three tiers, one primitive (Zapier's split, refined):**
  - **Tier 1 — declarative applet** (default, ~90%): NL → validated spec over the archetype catalog; deterministic, auditable, **zero per-run frontier cost**, mostly nothing to sandbox.
  - **Tier 1.5 — standing-instruction agent**: for genuinely fuzzy goals; still *declarative to author* (instructions + tools + channel), LLM-in-the-loop at runtime (the eve / Zapier-Agents shape).
  - **Tier 2 — real code** (power user, ~10%): Claude Code in Virtues' terminal, or git-link a repo. Full jail + full authoring loop + git-import. This is the only tier that is actually a "coding agent."
- **A plan/preview gate before materializing** (Replit's move): render the proposed spec — trigger, data, model-routing, capabilities — for one-tap confirm. Cheap, because the artifact is a small typed struct; doubles as the capability-grant surface.
- **The closed loop is the whole game:** write → reconcile → run/render → read stdout+errors+the rendered face → fix → repeat until it works. One-shot codegen doesn't compile; the feedback cycle is what ships working applets. Simple architecture, real feedback. *Simple ≠ shallow.*
- **Sandbox is an edge case, not the centerpiece — and it's ~80% built.** Because the declarative 90% are *inert validated specs* interpreted by trusted machinery (a spec can't escape), there is nothing to sandbox for them; safety there = **capability grants derived from the archetype + params** (a Rule that sends SMS structurally needs send-SMS; a View structurally only reads `data_health_*`), shown once at a **plan/preview gate**, plus the four boundary gates. The jail is reserved for the three code escape-hatch slots + the power tier. When it *is* needed: `code_interpreter` already runs in a `systemd-run` jail (PrivateNetwork, MemoryMax, seccomp, DynamicUser, ProtectSystem=strict; refuses to run unsandboxed in release). Work = **routing + a second profile**: run applet subprocesses through that jail when `owner ∈ {ai, community}` or during the authoring loop; trusted user/system applets keep the fast bare path.
- **The concrete stack is exactly where the local-first agents converged** (Claude Code = bubblewrap+seccomp; Codex = Landlock+seccomp+bubblewrap) — validating "no Docker, no microVM": **Landlock + seccomp driven from Rust** (`landlock` crate for FS + TCP-connect, `extrasafe`/`seccompiler` for syscalls), **wrapped in a `systemd-run` transient unit** for the resource caps + `DynamicUser` Landlock can't do. Skip Firecracker/gVisor/e2b (hypervisor/daemon/cloud deps against the grain); accept the residual "shared host kernel" risk as proportionate for a single trusted user.
- **Egress = a localhost proxy over a Unix-domain socket that holds and injects the one credential and allowlists the one host**, with direct network denied (netns removal or `IPAddressDeny=any`+allow). The secret never enters the sandbox — the cleanest privacy story, and again the exact pattern Claude Code/Codex use. This is the "scoped egress + one injected credential" profile, distinct from `code_interpreter`'s fully-sealed no-network one.

### 6. Memory
Keep the per-applet `memory` scratchpad (right for agent applets — daily continuity). But: **bound it** (cap + summarize on overflow, reuse `api/compaction.rs`); **hard line — definition on disk (git-able), runtime state in Postgres.** No memory-files-on-disk (breaks the DB-is-the-backup / no-SQLite model). Coded applets persist via SQL, not the text blob. Surface memory in the detail page as an editable "notes this applet keeps."

### 7. Triggers & conditions
Collapse the muddled "triggers / activations / gates" vocabulary to **two words: Trigger** (what wakes it) and **Condition** (whether it proceeds). Then add the missing one: **a data/event trigger** — "run when new `data_health_sleep` lands," "when applet X finishes," "when significance crosses a threshold" — backed by the dirty-window/projection mechanism. This lights up the **already-scaffolded but dead** transform-chaining (`parent_run_id`/`transform_stage` — CRUD exists, nothing creates child runs) and turns applets from isolated cron jobs into a **reactive dataflow where applets compose.** Keep raw-SQL `condition` for power users; offer legible presets.

## The authoring agent (a narrow Claude Code)

Steal the converged 2025–26 patterns, all implementable in pure Rust with no new protocol:

- **AGENTS.md as portable conventions.** Rename/dual-home `actions/AUTHORING.md` → `actions/AGENTS.md` (the neutral standard — Linux Foundation, read by 20+ tools, nearest-in-tree). Then an outside Claude Code / Cursor session can also author applets, not just the in-box agent. **Fix the doc drift first** — [`AUTHORING.md:55`](../actions/AUTHORING.md#L55) shows `connect_from_env()` but the real sig is `connect_from_env(app_name: &str)`; the agent will trust it literally.
- **Skills with progressive disclosure** for authoring recipes ("how to write a sync," "a dashboard," "a credential source"). Folder + `SKILL.md` (YAML name/description + body). Metadata always-on (cheap), body loaded on demand — decouples breadth of installed know-how from per-request token cost. **Note the symmetry: an applet *is* a skill's shape** (folder + manifest, loaded on demand). Our MEMORY.md auto-memory already proves we have the filesystem-loaded-context primitive.
- **CLI tools over MCP.** Expose box capabilities to the agent as CLI tools it calls with `--help`, not standing MCP servers (which cost tens of thousands of always-on tokens and add a protocol + long-lived processes). Reserve MCP for genuine external integrations.
- **Targeted edits, not whole-file rewrites** (Aider/Cline-style SEARCH/REPLACE) with **reflect-on-mismatch retry** (report the failed match back to the model, retry with a cap). Diff-apply mismatch is the #1 reliability bug class in every agent studied — build the applier carefully (exact-match, order-invariant).
- **Shadow-git checkpoints as the autonomy substrate** (Cline's best idea). Snapshot the applet's files/state *before each agent action* in a repo separate from the user-facing one; offer 3-way restore (files / conversation / both). This is what makes "let it run" safe — and it resolves the git two-lane tension below: shadow-git = the agent's private working/undo store; the user-facing applet repo is a separate, clean history.
- **Files-as-memory, not a vector framework.** The 2025–26 consensus (Anthropic memory tool, CLAUDE.md/auto-memory, Willison) is a directory of markdown the agent edits with file tools — auditable, diffable, zero infra. **Skip mem0/Letta/Zep** (vector+graph DBs, Python services, and the loudest cargo-cult warning — they solve a temporal-personalization problem a single-user coding agent doesn't have). If semantic recall over a large corpus is ever needed, wire the box's existing Postgres BM25 + halfvec + gte-small-384 as a *retrieval tool*, not a framework.
- **The model is hosted frontier via the gateway/slots — same as chat today. Not on the box.** So the loop is frontier-grade at tool-calling and multi-step verify; no local-model mitigations (toolshim, grammar-constrained decoding, context-window ceilings) apply. This is a resolved decision, not an open one.

**Reference designs to mine (Apache-2.0, mechanism not wholesale adoption):** **Goose** (Block) — Rust core + Axum HTTP/WS + SQLite session store is the closest architectural mirror of what an on-box agent service looks like; **Codex CLI** — the Landlock+seccomp+bubblewrap sandbox model (below); **Cline** — shadow-git checkpoints + model-tagged command risk; **Aider** — SEARCH/REPLACE + reflection edit primitive. Do **not** copy Goose's "every built-in tool is an MCP server" (indirection + surface you don't want) or OpenHands' Docker-in-Docker sandbox (too heavy).

## Permissions

Principle: **prompt on the irreversible, external, and credentialed; auto-allow the reversible, local, and sandboxed.** The research makes the case sharper than a hunch: Anthropic reports **sandboxing cut permission prompts 84%**, users approved **93%** of prompts (so prompts are mostly noise), and prompt-fatigue is a *security failure* — habituation trains users to click through the one dangerous prompt. So the sandbox isn't just safety; it's the permission-UX unlock.

**Model it as two orthogonal axes (Codex's design), not one mode:**
- **Sandbox level** — read-only / workspace-write / full-access.
- **Approval policy** — the elegant primitive is **`on-failure`: run optimistically inside the jail, only escalate to a human when the sandbox actually blocks something.** Boundary-crossing, not step-by-step.

Inside the jail, writing/reconciling/running/rendering/reading `data_*` read-only need **no prompts** (contained + reversible). Interrupt only at the **four boundary crossings:**

1. **Granting a credential** to an applet (now it touches real Google/Plaid/bank data). *The* boundary.
2. **Enabling a schedule/trigger** (it will run unattended). Prompt once at turn-on.
3. **External side effects that send or spend** (email, messages, money, deletion).
4. **Promoting sandboxed → trusted** (removing the jail).

Make it legible: **declare capabilities in the manifest** (needs: egress to `api.plaid.com`, the Google credential, write to `data_health_*`) — grant once, up front, reviewably. Better than per-action runtime prompts.

**Two hard rules from the research (both learned the painful way by others):**
- **Enforce outside the model.** The gate is harness code, never the LLM's judgment. A model self-tagging its own command as "safe" (Cline) is *advisory only*.
- **Denylists over a general shell are unenforceable.** Cursor's command denylist was bypassed four ways (base64-pipe, subshell, write-then-run, quoting) and they *removed* it. Don't allowlist shell verbs — **remove the broad capability and expose a narrow typed tool** (deny `curl`; give a domain-allowlisted fetch), enforce at the OS/sandbox layer, and keep hardcoded circuit-breakers (`rm -rf` on `/`/home) that fire even in any "auto" mode.

## The stack — and the build-toolchain wall

**The single most important constraint in the whole design:** the box has **no build toolchain.** Rust applet binaries are compiled in **CI and shipped precompiled** (`release-linux.yml` → `/usr/local/libexec/virtues`); there is no on-box `cargo`. And `view` applets go through a **build-time** Vite glob, so a new Svelte face needs the *web bundle* rebuilt too. ⇒ **The only applet shape authorable on-box with zero build is the agent-prompt** — which is exactly why `setup_action` only makes that shape today.

Resolution:

| Layer | Language | Built where | Notes |
|---|---|---|---|
| **Built-in core** | Rust + `virtues-helpers` + `virtues-actions` lib | CI, precompiled | Blessed path. 18/18 existing actions are Rust. Helpers give DB, OAuth-vault, HTTP, credential-decrypt for free. |
| **AI-authored logic** | agent-prompt (no code) OR sandboxed interpreted script | on-box, no compile | Reuse the `systemd-run` Python path already present for `code_interpreter`. |
| **AI-authored faces** | Svelte via **runtime loading** (NOT the build-time glob) | on-box | ⚠️ Keystone: without runtime-loadable views there are no AI-authored dashboards at all. |
| **Coded Rust applets** | Rust | CI / dev | A developer/power-user artifact, not something in-box chat spins up live. |

Rejected: shipping a Rust toolchain on the appliance (GBs, slow ARM builds, turns the box into a dev machine); cloud-compiling AI-written Rust (applet source would leave the box — privacy compromise). If live AI-authored Rust is ever a hard requirement, on-demand cloud build is the only thin-appliance option and it's an explicit privacy tradeoff.

## Git / distribution

Two lanes, because today's import is **one-way and destructive** (`git clone --depth 1` then `git reset --hard FETCH_HEAD` — "pulled code wins"; no commit-back):

- **Box-owned lane** — a repo the box commits to. AI-authored applets get version history, one-command rollback, and every AI change becomes a **reviewable diff** (an audit log + a natural permission gate). *New work* — must be reconciled with, not layered on, the one-way import.
- **Imported lane** — upstream-owned, read-only / hard-reset. `POST /api/admin/actions/import-git` already does clone/fetch/reconcile/diff; one repo = one folder = one-or-more applets. Distribution = git URLs, no registry. **Cloned = untrusted → sandbox it.**

(Also: git-import is admin-HTTP only, no CLI.)

## Sequence

1. **Rename + collapse the surface** — Applet everywhere; one owner-sectioned list (no sub-tabs); one degrading detail page. Cheap; fixes half the complaints; everything lands on top.
2. **Unlock on-box authoring** — runtime-loadable views + the sandboxed-script path + sandbox routing. This is what makes live AI authoring *possible at all*. Flagship.
3. **The closed authoring loop** — file-authoring tools scoped to `actions/<name>/`, write→run→see→fix, AGENTS.md + authoring Skills, capability-manifest permissions.
4. **Event/data triggers + wire the dead chaining** — the composability unlock.
5. **Git box-owned lane** — commit-back, rollback, reviewable diffs.
6. **Cleanup** — memory bounding, delete state, trigger/condition vocabulary, doc-drift fix.

## Open decisions

- **On-box build resolution** — confirm runtime-loadable views approach (dynamic Svelte compile vs sandboxed-iframe render vs simpler runtime format); confirm sandboxed-interpreted-script language + whether a runtime ships or is assumed present.
- **Git two-lane reconciliation** — how box-owned commits coexist with one-way imports without clobbering (shadow-git checkpoints likely resolve the agent-working-state half).
- **Should coded-Rust applets ever be on-box-authorable**, or permanently a CI/dev artifact.

## Resolved (was open)

- **The authoring model is hosted frontier via the gateway/slots — not on the box.** Same posture as chat/transcription today. No on-box model; local-model authoring is out of scope. Privacy = data stays on the box, inference goes to the gateway as it already does.
