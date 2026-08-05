# Applets — the surface audit

> Status: **findings, 2026-08-05.** Amends [`applets-overhaul-plan.md`](./applets-overhaul-plan.md)
> (design locked 2026-07-19) and [`applet-authoring-plan.md`](./applet-authoring-plan.md)
> with what phases 1–3 actually landed as, read back against what they specified.
>
> Nothing here is a new design. Every item is one of three things: a **contract
> severed** in implementation, a **regression** from the plan's own UI spec, or
> **drift** the plan predicted and the sweep never reached. The plan's model —
> flat fields, one primitive, chat as the front door — holds up; it is the last
> mile between the manifest and the person reading the screen that has come apart.

## The keystone — `description` never leaves the manifest

The plan makes one field load-bearing three separate times:

- *"`description` is the intent-source, not decoration"* — the compaction of the
  authoring chat into one sentence; the thing the preview gate blesses.
- *"the gate headline"* (authoring plan §A) — what the user approves.
- *"Row = face-thumbnail-or-glyph, name, **plain-English line**, last activity,
  on/off, run-pulse"* (plan, UI surfaces).

All 24 manifests carry one. [`applet_templates/mod.rs:43`](../virtues-core/src/applet_templates/mod.rs)
parses it into `AppletTemplate`. Then every one of the three upsert branches
([`mod.rs:1163`](../virtues-core/src/applet_templates/mod.rs)) binds thirteen
fields and `description` is not among them — because **`app_applets` has no
`description` column**. It was never in `0004`, and no migration since has added
it. It is read off disk and dropped on the floor at reconcile.

Downstream, everything the plan hangs on that sentence is therefore missing:
neither `list_applets_handler` ([`server/api.rs:219`](../virtues-core/src/server/api.rs))
nor `get_applet_handler` can return it, the `Applet` interface in `client.ts`
has no such key, the list shows no plain-English line, and the detail page has
no headline.

What stands in its place is [`applets/descriptions.ts`](../apps/web/src/lib/applets/descriptions.ts):
a hardcoded seven-entry map keyed on `function_name`, of which two keys
(`day_illustration`, `trash_purge`) name applets that do not exist in this
repo, and which is missing roughly fifteen that do. Its own header carries the
fix: *"TODO: promote this to a `description` field … so this map can go away."*

**This is the first thing to build.** The list line, the detail headline, the
authoring gate, and any future gallery all read from it, and none of them can be
designed honestly until it exists.

## Findings register

| # | Where | Finding | Plan says |
|---|---|---|---|
| **D1** | `app_applets` schema · `mod.rs:1163` · `api.rs:219` | No `description` column; parsed then dropped; absent from both API handlers and the TS type | intent-source · gate headline · the list's plain-English line |
| **D2** | `applets/descriptions.ts` | Hardcoded 7-entry stand-in; 2 keys name nonexistent applets, ~15 real ones missing | (the file's own TODO: delete it) |
| **L1** | `AppletsPanel.svelte:350` | `defaultViewMode="table"` — the card with the run-pulse and output excerpt is hidden unless the user switches to grid | the row *is* glyph + name + line + activity + toggle + pulse |
| **L2** | `AppletsPanel.svelte:170–212` | Default table is six columns of machine vocabulary (Origin · Lifecycle · Schedule · Last run · Status); no plain-English line, no on/off | as above |
| **L3** | `AppletsPanel.svelte:199–211` | The `Status` column keys on `enabled` but renders the *last run's* status. Enabled/disabled is unreachable from the table except through a filter | on/off is a row affordance |
| **L4** | `AppletsPanel.svelte:288–297` | `Reconcile` ("re-read applet manifests from disk") is a header button beside `New` | not in the spec; this is an operator verb on a consumer page |
| **L5** | `AppletsPanel.svelte:355` | Empty state is `"No applets yet — ask for one in chat."` — no examples, no starter set | the contemplative starter set markets as **"Practices"**; `Practices` appears nowhere in the codebase |
| **T1** | `AppletDetailView.svelte` | Four editable fields: Name, Agent prompt, Schedule, Memory. No description, condition, triggers, `until`, limits, cost, credential/source | header → face → *the guts* (schedule, triggers, limits — all editable) |
| **T2** | `AppletDetailView.svelte:222–228` | The face is not on the page; it is an "Open view" button to a separate tab | the face **is** the detail body; headless falls back to the run log |
| **T3** | `AppletDetailView.svelte:68` | `isSystem` gates on `owner === 'system'` — the exact mistake the list already fixed by switching to `origin`. Your Gmail sync's page therefore claims to be system-managed and locks its name | `owner` is write-authority, not provenance |
| **T4** | `AppletDetailView.svelte:497–499` | `.muted-inline` resolves to `--color-warning`, so every applet's "runs forever" renders orange | lifecycle is a neutral fact |
| **T5** | `AppletDetailView.svelte:243` | Hint reads `Managed by templates.toml` — a file that no longer exists (per-folder manifests since the rename) | — |
| **T6** | `AppletDetailView.svelte:259, 299, 304` | "Delete **action**", "System **action** — managed automatically", "What should this **action** do each run?" | Applet everywhere (phase 1) |
| **T7** | `AppletDetailView.svelte:287–293` | Memory is a bare textarea labeled "Memory" / "Persistent markdown scratchpad, carried across runs" | *"notes this applet keeps"* |
| **F1** | `applet-views/index.ts` · `TabContent.svelte:32` | A second face system survives: the Svelte `ui/Card.svelte` / `ui/Detail.svelte` registry keyed on `config.view.name`. `hello_world` ships **both** it and `face/index.html` | *"`{view:{name}}` dies with iframe faces"* |
| **F2** | `applet-views/index.ts` | `loadCard` is referenced nowhere — dead | — |
| **F3** | `applets/palette.ts:32–134` | `paletteFor` and its six palettes (~110 lines) are entirely unreferenced, and every stop is a hardcoded light-only hex gradient | dead code; also violates the no-hardcoded-colors rule if ever revived |
| **M1** | 23 of 24 manifests | `runtime = "…"` still declared | phase 1 drops `runtime` — it is derived from which fields are set |
| **M2** | 16 manifests | `default_cron`; only 2 use the canonical `schedule` (loader accepts both via `alias`) | manifest key → `schedule` in phase 1 |
| **M3** | `MANIFEST_SCHEMA.json`, `AUTHORING.md` | Both still speak "action", reference `actions/<name>/`, and route `config.view.name` at `apps/web/src/lib/applets/<name>/` — a path that does not exist (real path is `applets/<name>/ui/`). `AUTHORING.md` announces "the three runtimes" and then lists two | phase 6 cleanup: AUTHORING.md → AGENTS.md + doc-drift fix |
| **M4** | all 24 descriptions | No shared voice. "A small dog who lives on your box" sits beside "Sweep via_proxy credentials whose access tokens are nearing expiry" and "the binary self-gates to the user's local maintenance hour" | the sentence is the user's intent, in the user's terms |
| **A1** | `applets/user/heart_rate_explorer/manifest.toml` | A face-only dashboard carries `agent = "…If run, do nothing and report that this is a display-only dashboard."` `setup_applet` permits an agent-less face ([`applet_setup.rs:63–74`](../virtues-core/src/tools/applet_setup.rs)), but AGENTS.md never says so — so the model invented a no-op prompt, and "Run now" spends a model call to say nothing | capability contract must be exhaustive; the model writes only what it is told exists |
| **A2** | `applets/AGENTS.md` | No rule for face-only applets; no rule that `description` is what the user will read on the list row | the sentence is compiled from intent and blessed at the gate |

## Reading of the whole

Three separate things are being conflated on both pages, and untangling them is
most of the UX work:

1. **What this applet is for you** — the description sentence, the face, the
   last thing it produced. This is what a person opens the page to see, and it
   is the layer that is almost entirely absent.
2. **How it is wired** — schedule, triggers, condition, `until`, limits. Present
   but partial on detail, and shown as raw cron and raw SQL with no gloss.
3. **How it is administered** — reconcile, origin, owner, source path, forking.
   Currently the most prominent layer on both pages, and the one a user has the
   least reason to think about.

The plan already ordered these correctly (header → face → guts). The
implementation inverted them: the list leads with Origin and Lifecycle badges,
and the detail page leads with a form.

The second reading: **`owner` keeps leaking into places that want `origin`.** The
list page learned this and documents it well ([`AppletsPanel.svelte:36–42`](../apps/web/src/lib/components/applets/AppletsPanel.svelte));
the detail page (T3) and `is_system` in the API response never got the memo.
`origin` should be the only provenance field any UI touches.

## Proposed sequence

Ordered by what unblocks what, not by size.

1. **The description spine.** Claim a migration number (`make migration`), add
   `description TEXT` to `app_applets`, bind it in all three reconcile branches,
   return it from both handlers, add it to the `Applet` type, accept it in
   `setup_applet`'s manifest write, delete `descriptions.ts`. *(D1, D2)*
2. **Rewrite all 24 descriptions** in one user-facing voice — what it does for
   you, not how it is implemented. Cheapest large gain on the whole surface, and
   it is what every subsequent screen renders. *(M4)*
3. **The listing page.** Card-first with the plain-English line; inline on/off;
   honest status; demote Reconcile out of the header; a real empty state that
   names the starter set. *(L1–L5)*
4. **The detail page.** Face-first; the full guts, editable and glossed;
   `origin` not `owner`; the verbiage sweep. *(T1–T7)*
5. **Collapse the second face system** and delete the dead palette/registry
   code. *(F1–F3)*
6. **Manifest and doc sweep** — `schedule` everywhere, drop `runtime`, rename
   `AUTHORING.md`'s vocabulary, fix `MANIFEST_SCHEMA.json`, add the face-only
   and description rules to AGENTS.md. *(M1–M3, A1, A2)*

Steps 1–2 are a coherent first slice and touch no UI. Steps 3–4 are the
user-visible payload. Steps 5–6 are cleanup that can ride along with either.
