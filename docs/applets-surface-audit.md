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

> **FIXED 2026-08-05.** Migration `0093` adds the column, the loader gained the
> field, all three upsert branches bind it, both handlers return it, and the 22
> shipped descriptions were rewritten in one voice. `descriptions.ts` is gone.
> Kept below because the shape of the failure is worth remembering.

All 24 manifests carry one. The `Template` struct **never had the field** —
the `description` at `mod.rs:43` belongs to `Source`, a different struct — and
there is no `deny_unknown_fields`, so serde discarded the key silently on every
load. It was not parsed-then-dropped; it was never read. Downstream, every one
of the three upsert branches bound thirteen fields with no `description` among
them, because **`app_applets` had no `description` column** either: not in
`0004`, and no migration since had added one.

So everything the plan hung on that sentence was missing: neither handler could
return it, the `Applet` interface had no such key, the list showed no
plain-English line, and the detail page had no headline.

What stood in its place was `applets/descriptions.ts` — a hardcoded seven-entry
map keyed on `function_name`, of which two keys (`day_illustration`,
`trash_purge`) named applets that do not exist in this repo, and which was
missing roughly fifteen that do. When it missed, it fell through to the first
sentence of the agent **prompt**, so `morning_examen` introduced itself as *"You
are the user's morning reflection companion"* — an instruction addressed **to**
the applet, leaking out as a description **of** it. The file's own header
carried the fix: *"TODO: promote this to a `description` field … so this map can
go away."*

**The lesson worth keeping:** a field can be in every manifest, documented as
load-bearing in two plans, and still never exist — because nothing between the
TOML and the screen was ever required to carry it, and no test asked. Both ends
now have one.

## Findings register

| # | Where | Finding | Plan says |
|---|---|---|---|
| ~~**D1**~~ | `app_applets` schema · `mod.rs:1163` · `api.rs:219` | **FIXED (0093).** No `description` column; never parsed (the loader struct lacked the field); absent from both API handlers and the TS type | intent-source · gate headline · the list's plain-English line |
| ~~**D2**~~ | `applets/descriptions.ts` | **FIXED.** File deleted. Hardcoded 7-entry stand-in; 2 keys named nonexistent applets, ~15 real ones missing | (the file's own TODO: delete it) |
| ~~**L1**~~ | `AppletsPanel.svelte:350` | **FIXED.** Cards are the default; the table stays for anyone who prefers it (the choice is remembered per entity type) | the row *is* glyph + name + line + activity + toggle + pulse |
| ~~**L2**~~ | `AppletsPanel.svelte:170–212` | **FIXED.** `What it does` carries the sentence, Origin and Lifecycle demoted to filters, and the card leads with the plain-English line | as above |
| ~~**L3**~~ | `AppletsPanel.svelte:199–211` | **FIXED.** Split into `On` and `Last result`; the card carries an explicit `off` pill, since dimming alone reads as loading | on/off is a row affordance |
| ~~**L4**~~ | `AppletsPanel.svelte:288–297` | **FIXED.** Behind the overflow, worded as "Re-read from disk" — reachable, not offered | not in the spec; this is an operator verb on a consumer page |
| ~~**L5**~~ | `AppletsPanel.svelte:355` | **FIXED (partly).** The empty state now shows what to ask for. A curated "Practices" collection is still unbuilt | the contemplative starter set markets as **"Practices"**; `Practices` appears nowhere in the codebase |
| ~~**T1**~~ | `AppletDetailView.svelte` | **FIXED.** Was four fields. Now carries the intent sentence, what wakes it, the gate, lifecycle, limits, and per-run cost | header → face → *the guts* (schedule, triggers, limits — all editable) |
| ~~**T2**~~ | `AppletDetailView.svelte:222–228` | **FIXED.** The face is the page; the button now opens it full-screen rather than being the only way to see it | the face **is** the detail body; headless falls back to the run log |
| ~~**T3**~~ | `AppletDetailView.svelte:68` | **FIXED.** Editability still follows `owner` (that is what the server enforces); the EXPLANATION now follows `origin`, so a source applet says it belongs to a connection you made | `owner` is write-authority, not provenance |
| ~~**T4**~~ | `AppletDetailView.svelte:497–499` | **FIXED.** `.muted-inline` resolved to `--color-warning`, so every applet's "runs forever" rendered orange | lifecycle is a neutral fact |
| ~~**T5**~~ | `AppletDetailView.svelte:243` | **FIXED.** | — |
| ~~**T6**~~ | `AppletDetailView.svelte:259, 299, 304` | **FIXED.** | Applet everywhere (phase 1) |
| ~~**T7**~~ | `AppletDetailView.svelte:287–293` | **FIXED.** Now "Notes it keeps" — what this applet wrote down for its own next run | *"notes this applet keeps"* |
| ~~**F1**~~ | `applet-views/index.ts` · `TabContent.svelte:32` | **FIXED.** Registry deleted, `hello_world/ui/` deleted, `[config.view]` gone from the manifest and the schema. Biscuit now renders through the iframe face like everything else | *"`{view:{name}}` dies with iframe faces"* |
| ~~**F2**~~ | `applet-views/index.ts` | **FIXED.** Gone with the registry | — |
| ~~**F3**~~ | `applets/palette.ts:32–134` | **FIXED.** Deleted; the file is 194 → 70 lines and keeps only the schedule/time formatters anything actually imports | dead code; also violates the no-hardcoded-colors rule if ever revived |
| **M1** | 23 of 24 manifests | `runtime = "…"` still declared | phase 1 drops `runtime` — it is derived from which fields are set |
| **M2** | 16 manifests | `default_cron`; only 2 use the canonical `schedule` (loader accepts both via `alias`) | manifest key → `schedule` in phase 1 |
| **M3** | `MANIFEST_SCHEMA.json`, `AUTHORING.md` | Both still speak "action", reference `actions/<name>/`, and route `config.view.name` at `apps/web/src/lib/applets/<name>/` — a path that does not exist (real path is `applets/<name>/ui/`). `AUTHORING.md` announces "the three runtimes" and then lists two | phase 6 cleanup: AUTHORING.md → AGENTS.md + doc-drift fix |
| ~~**M4**~~ | all 24 descriptions | **FIXED.** No shared voice. "A small dog who lives on your box" sits beside "Sweep via_proxy credentials whose access tokens are nearing expiry" and "the binary self-gates to the user's local maintenance hour" | the sentence is the user's intent, in the user's terms |
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
