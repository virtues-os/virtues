# Notebooks — paradigm + implementation plan

Status: planning (2026-07-08). Renames + folds the half-built "Spaces" paradigm into a
first-class **Notebook** — a workspace lens over your life-graph, with a **Library** of
materials (files + external snapshots + pasted text), native text extraction, notebook-scoped
grounded chat, and source-level citations. North star: NotebookLM / Claude Projects, but over
live entities you already own (federation, not upload-bin).

## The model

- **Notebook** = a workspace you *enter and work in*. A saved, named lens over the graph:
  a **Library** of materials + `instructions` + a living "state of the room" memo + many
  filed chats + a retrieval scope. Top-level, peer to Today. (Today = time-slice;
  Notebook = topic-slice; pages/chats/entities/assets are the primitives both slice.)
- **Thing** = a real-world **entity** for ER/reference (pet, car, book, concept — the
  catch-all beside person/place/org). Mentioned and referenced, not entered.
  The old "folder you can re-enter" role is **removed**.
- Internal metaphor = **room** (you enter it; it has a state and an accent tint). User-facing
  name = Notebook. Don't name the feature "Rooms".

### Naming (resolved; REVISED 2026-07-20 — see researcher-plan.md decision 0)

- ~~The materials in a notebook = its **Library**~~ **SUPERSEDED**: the "Library" noun is
  retired — a notebook *containing* a Library was a container-inside-container that
  contradicted the lens model. Things are simply **in the notebook**; the verb is
  **"Add to notebook"**, the contents are **notebook items** (`app_notebook_items`).
  Still NOT "Sources" (credential connections) and NOT "References" (`[@ref]` links).
  Internal DB role value stays `library` (= grounds chat, the default for every added
  item); `role='pin'` is schema-only for nav-only edges (e.g. related notebooks).
- **Items = anything retrievable, not just files.** Files, external snapshots, pasted text,
  AND internal entities/data/days/people — an item's chunks are resolved into the
  notebook's retrieval scope regardless of type. This is the federation superpower (don't ape
  NotebookLM's upload-bin).
- A notebook has **no local notes** — just global **Pages** it references. "Paste raw text"
  creates a `.md`/`.txt` file in Drive → becomes a Library material like any other file.
  One ingestion path for everything.

### Collection-mechanism consolidation (kills the current 3-way overlap)

| Mechanism | Was | Becomes |
|-----------|-----|---------|
| Spaces | collection + chat-binding + memo + color | **Notebooks** |
| Things (as container) | "folder you re-enter: projects/pets/goals/topics" | **removed** — fold into Notebooks; Thing stays as pure ER entity |
| Pins | flat global URL shortcuts | **keep** (different axis: cross-cutting quick-nav) |
| Tags (on pages) | stored, no UI | **keep as input to notebooks** (future smart membership); no browsing UI |

Clean three tiers: **Notebooks** (curated workspaces) · **Pins** (flat shortcuts) ·
**Tags** (labels that feed notebooks).

## Doctrines (decided)

- **Chat = many threads, Claude-Projects style.** A notebook shares context (Library +
  instructions) across *multiple* chats — e.g. characters in one, drafting in another, research
  in a third — each grounded in the same materials. NOT NotebookLM's single continuous thread.
  (This is also less work: the data already binds many chats to one notebook.)
- **Materials are snapshots you own, not live pointers.** External URL/YouTube = fetch once,
  extract readable text/transcript, store with `source_url` + `fetched_at` + `content_hash`.
  Durable, offline, citable. Re-fetch/change-detection is a separate future "watch" feature.
- **Extraction is native-text only.** No OCR for now (born-digital PDFs carry text).
  *(SUPERSEDED 2026-07-20, see [researcher-plan.md](./researcher-plan.md): extraction is now
  universal on upload — every text-bearing drive file is extracted/embedded; the Library is a
  lens (scope + up-weight), not the ingestion trigger.)*
- **Citations: source/page-level now, char-precise later.** Source-level citation is the trust
  mechanism and nearly free (retrieval already returns chunk→record_id). Char/rect highlight is
  annotation-grade and deferred.
- **One asset viewer for all MIME types**, reused full-page, in a lightbox, and in the notebook
  split-pane.

## Data model

Data is disposable (only `dev_seed.rs` seeds it) — reseed rather than write heavy migrations.

Rename + consolidate:
- `app_spaces` → `app_notebooks`; add `instructions TEXT` (persistent system prompt, distinct
  from the transient `current_status` memo).
- `app_space_items` → `app_notebook_items`; add `role TEXT NOT NULL DEFAULT 'pin'` (`library` | `pin`).
- `app_chats.space_id` → `notebook_id`.
- ID prefix `space_` → `nb_` (retire the `WORKSPACE_PREFIX` ghost too).
- `wiki_things`: drop the container role. Reseed genuine-entity things only; project-things
  become notebooks.

Asset layer:
- Storage stays `app_drive_files` (filesystem + PG metadata) — unchanged.
- `app_assets` (sidecar, keyed by `file_id`): `kind` (`upload` | `web` | `youtube` | `text`),
  `source_url`, `fetched_at`, `content_hash`, `extracted_at`, `has_text BOOL`.
- `app_asset_text`: `file_id`, `segment_index`, `page` / `timestamp`, `text`, `anchor` (nullable
  until annotation phase).
- `app_asset_annotations` (phase 6): `file_id`, `kind` (`highlight`|`note`), `anchor`, `color`, `note_text`.

Search:
- Index extracted text into `search_embeddings` with `ontology='asset'`, `record_id=file_id`
  (reuse `search/indexer.rs`).
- Notebook-scoped query: resolve `role='library'` members → asset ids + entity ids, then
  filter/boost in `search/query.rs` (reuse the existing `entities` filter path).

## Componentry

Asset viewer:
```
AssetViewer.svelte           dumb renderer: (metadata, contentUrl) → dispatch by MIME
  panes/ImagePane            inline + zoom/pan
  panes/PdfPane              pdf.js: text layer, page nav, #page deep-link
  panes/VideoPane            player
  panes/AudioPane            player
  panes/TextPane             markdown/code
  panes/FallbackPane         icon + download
AssetHeader.svelte           title, MIME badge, size, download, Add-to-Notebook, in-N-notebooks
Hosts:
  tabs/views/AssetView.svelte    full-page host at /asset/{id}   (refactor existing)
  AssetLightbox.svelte           generalize MediaLightbox to all types; quick-look overlay
  AssetPane.svelte               notebook split view (Library │ viewer │ chat)
```

Notebook home = tabbed: **Overview** (state-of-room + instructions) · **Library** · **Chats** · **Map**.
- Notebook home is a **built-in default view** (plain Svelte components — NOT the `view`-applet
  system, no user customization layer). "Pluggable" only in the mild sense that Library and Map are
  separate components, so a **Timeline** or **Outline** module could be added later without a
  rewrite. v1 ships **Library + Map** only.
- **Map** = an auto **concept graph**, not files-linking-files. Nodes = entities referenced/extracted
  across the notebook's materials + pages; edges = co-occurrence; clusters = themes. Actionable:
  click node → filter Library to materials mentioning it; **hubs** = key texts; **orphans**
  (materials nothing references) = "unread / not yet synthesized".

Routes: canonical `/asset/{id}`; alias `/drive/file_{id}`. Notebooks at `/notebooks` + `/notebook/{id}`.

## Sidebar restructure

Fold **Narrative** into **Wiki**; Wiki becomes the whole **life-graph**: contextual entities
(person/place/org/thing) + temporal (years/days) + narrative (telos/identity). Keep **Today**
top-level anyway (highest-frequency destination). Semantic split = **workspace (top) vs.
substrate (bottom)**.

```
Ask or search…  ⌘K

Home            ← rhythm
Today

Chats           ← create
Pages
Notebooks

Wiki            ← substrate: life-graph · files · automation
Drive
Applets
```

(Note: "Wiki" fits entities better than narrative/telos — flag for possible rename if the
temporal+narrative expansion makes it read oddly. Not renaming now.)

## How it links together

```
Notebook ──(item role=library)──▶ /asset/file_x
Asset ──"in N notebooks" + Add-to-Notebook──▶ Notebook
Asset ──[@ref]──▶ page/chat (ref-picker already finds files)
Asset ──extracted+embedded──▶ notebook-scoped search ──▶ chat answer
Chat citation ──deep-link──▶ /asset/file_x#page=3
Annotation ──promote──▶ note-page ──pin──▶ Notebook   (later)
```

## Order: first → last

**First (foundation + de-risk):**
1. Rename Spaces→Notebooks, **remove Things-as-container**, and apply the **sidebar restructure**
   (Narrative→Wiki, workspace/substrate split). One migration/reseed. Cut 4 of 5 prototype Home
   variants while here.
2. Viewer spine — `AssetViewer` + panes, generalize lightbox, wire Drive/refs to open it,
   pdf.js, route `/asset/{id}`. Frontend-only, no schema.
3. Library + **external URL / YouTube snapshot ingestion** + paste-text-as-file + native text
   extraction, lazy on add-to-Library. `role='library'`, Library panel, `AssetPane` split view.

**Middle (the payoff — the NotebookLM moment):**
4. Extraction → embed (`ontology='asset'`) + **notebook-scoped retrieval** + **source-level
   citations** (page-level for PDFs via `#page=N`). Many grounded chats per notebook.

**Last (power + polish):**
5. Auto-maintained "state of the room" briefing (fed by the salience engine) + Home surface +
   the **Map** concept-graph tab.
6. Annotations + char-precise highlights + deep-link citations.
7. Smart/rule membership ("everything tagged X / referencing [@Person]"); external-material
   change-detection ("watch").

## Phase 1 — detailed (rename + consolidation + IA)  ✅ IMPLEMENTED 2026-07-09

Status: done. Migration `0032_notebooks_rename.sql`; core (`api/notebooks.rs` + consumers,
`nb_` prefix) compiles clean; frontend renamed (client, store, views, breadcrumb, registry,
routing maps) — no new type errors; sidebar restructured; Things demoted to reference entities.
Home-variant cull done: kept `HomeViewSpread` ("open notebook"), deleted the other 4 + the
`HomeSwitcher` harness; Home renders Spread directly. Deferred sub-items: Narrative link inside
WikiView (route + search still reach it), `role`/`archived_at`/`instructions` UI (later phases).


Scope: rename Spaces→Notebooks, demote Things to a pure entity, restructure the sidebar, cull the
Home prototypes. **No new UI, no viewer, no extraction.** Data is disposable (dev_seed only) → reseed.
Explicitly NOT in Phase 1: Library/Map/tabbed home, asset viewer refactor, extraction, scoped chat.

### 1a. Schema — new migration `virtues-core/migrations/00NN_notebooks.sql`
- `app_spaces` → `app_notebooks`; add `instructions TEXT`, `archived_at TIMESTAMPTZ`.
- `app_space_items` → `app_notebook_items`; add `role TEXT NOT NULL DEFAULT 'pin'` (`library`|`pin`);
  rename `space_id` → `notebook_id`.
- `app_chats.space_id` → `notebook_id`; rename index `idx_chats_space` → `idx_chats_notebook`.
- `wiki_things`: audit for any container/membership columns; drop them (membership already lives in
  items). No new columns — Thing stays a plain entity.
- (Columns `instructions`/`archived_at` land now but UI wiring is later; cheap to add during rename.)

### 1b. Rust rename (virtues-core)
- `src/api/spaces.rs` → `src/api/notebooks.rs`: `Space`→`Notebook`, `SpaceItem`→`NotebookItem`,
  `SpaceSummary`/`SpaceDetail` likewise; `set_chat_space`→`set_chat_notebook`; add `instructions`,
  `archived_at`, item `role` to structs + queries.
- `src/api/mod.rs`: update re-exports.
- `src/server/mod.rs`: routes `/api/spaces*` → `/api/notebooks*` (~L680-699).
- `src/server/api.rs`: rename the 8 handlers (~L2970-3053).
- `src/api/chats.rs`, `src/api/chat.rs`, `src/api/pages.rs`: `space_id`→`notebook_id` in request
  structs + calls (`add_space_item`→`add_notebook_item`, etc.); update the chat system-prompt
  "salience lens" inlining to read notebook.
- `src/ids/mod.rs`: `SPACE_PREFIX "space"` → `NOTEBOOK_PREFIX "nb"`; delete `WORKSPACE_PREFIX`.
- Final sweep: `rg -i '\bspace' virtues-core/src` for stragglers.

### 1c. Seed + Things demotion
- `src/dev_seed.rs`: reseed notebooks with `nb_` ids; seed only genuine-entity things (no
  project/goal "folders"); move any project-things into notebooks.

### 1d. Frontend rename (apps/web)
- `src/lib/api/client.ts`: `Space*` types → `Notebook*` (~L1077-1243); endpoints → `/api/notebooks`;
  `listSpaces`→`listNotebooks` etc.; `ChatSession.space_id`→`notebook_id`.
- `src/lib/stores/space.svelte.ts` → `notebook.svelte.ts`: `SpaceStore`→`NotebookStore`,
  `spaceStore`→`notebookStore`, `setChatSpace`→`setChatNotebook`.
- Views: `SpacesListView.svelte`→`NotebooksListView.svelte`,
  `SpaceDetailView.svelte`→`NotebookDetailView.svelte` (straight rename — keep current UI,
  the tabbed Overview/Library/Chats/Map home is a later phase).
- `src/lib/components/chat/ChatSpaceBreadcrumb.svelte` → `ChatNotebookBreadcrumb.svelte`.
- `src/lib/tabs/registry.ts`: `space` tab type → `notebook`; routes `/spaces`→`/notebooks`,
  `/space/space_`→`/notebook/nb_`; update the `thing` entry's description (drop "folder you
  re-enter" → entity). `ThingsView`/`ThingDetailView` stay.
- `src/routes/(app)/+layout.svelte`: `spaceStore.load()` → `notebookStore.load()`.
- Final sweep: `rg -i '\bspace' apps/web/src`.

### 1e. Sidebar restructure — `src/lib/sidebar/sections.ts`
- `SPACES` section → `NOTEBOOKS` (`id: sys_notebooks`, `href: /notebooks`, pick icon).
- Regroup into three gap-separated clusters: **[Home, Today]** · **[Chats, Pages, Notebooks]** ·
  **[Wiki, Drive, Applets]**.
- Remove **Narrative** as a top-level item; surface it inside Wiki (minimal: a link/entry in
  `WikiView`). NOTE: the full "Wiki = entities + temporal + narrative" landing redesign is a
  follow-on, NOT Phase 1 — Phase 1 only demotes Narrative and regroups.
- `LEGACY_ID_MAP`: add `sys_spaces` → `sys_notebooks`.

### 1f. Home prototype cull (parallel, optional)
- Pick one `HomeView*` variant; delete the other four + `HomeSwitcher`; point registry `home` at it.

### Decisions inside Phase 1 (locked)
- **Migration style: new additive `ALTER … RENAME` migration** — auto-applies at boot via core's
  `sqlx::migrate!`, preserves the local/box core `virtues` DB (no reset). Existing rows keep their
  `space_…` ids (opaque text, still valid); only new notebooks get `nb_`. NOTE: notebooks live in
  the core `virtues` DB — this does NOT touch the `atlas_test`/`virtues_api_test` staging DBs
  (billing/entitlements only). Staging is irrelevant to this work; develop against `make dev`.
- **Narrative → Wiki** (not Settings — it's reflective *content*, not config). Phase 1 = demote +
  reachable-in-Wiki only; defer the rich "Wiki = entities + time + narrative" landing redesign.
- Add `archived_at`/`role` now (cheap) even though their UI comes later.

## Loose ends & validated design (code audit 2026-07-09)

Four read-only deep-dives audited the feature against the real code. Findings + locked decisions:

### Phase 1.5 — "pristine" cleanup (cheap; do before Phase 2) — precise spec

NOTE: **keep the "room" metaphor** (`--room-accent`, `.room-icon/.room-trigger/.room-name`, "the
room this chat lives in") — it's the documented internal design language. Only the stale OLD NAME
("Space"/"Spaces") and user-facing "Rooms" copy are wrong.

**(1) Surface the new columns** — `api/notebooks.rs`:
- `Notebook` + `NotebookSummary` structs: add `instructions: Option<String>`, `archived_at: Option<Timestamp>`.
- `NotebookItem` struct: add `role: String`.
- Add `instructions, archived_at` to every notebook SELECT/RETURNING: `list_notebooks` (~L110),
  `get_notebook` (~L131), `create_notebook` RETURNING (~L173), `update_notebook` SELECT+RETURNING (~L193,234).
- Add `role` to the item SELECTs: `get_notebook` items (~L145) + `add_notebook_item` RETURNING (~L304).
- `UpdateNotebookRequest`: add `instructions: Option<Option<String>>` (tri-state) + `archived: Option<bool>`
  (→ set/clear `archived_at`); extend the UPDATE SET + binds.
- `client.ts`: add `instructions`/`archived_at` to `Notebook` (Summary+Detail inherit) and `role` to
  `NotebookItem`; add `instructions?`/`archived?` to the store `update()` patch type.
- `role` is READ-ONLY in 1.5 (setting library-vs-pin is Phase 3); `add_notebook_item` keeps DB default 'pin'.

**(2) Split memo vs instructions + wire into chat** — `chat.rs build_notebook_context` (~L767):
- Add an `<instructions>` element (from `detail.notebook.instructions`) distinct from `<memo>`
  (`current_status`); update the preamble to describe both (instructions = persistent behavior,
  memo = transient catch-up). get_notebook already returns it once the struct carries it.
- Optional (deferrable): a small "Instructions" textarea in `NotebookDetailView` beside the memo.

**(3) Naming sweep — only old-name + user-facing "Rooms"** (NOT the room metaphor):
- `ChatView.svelte`: `chatSpaceId`→`chatNotebookId`, `seededSpaceFor`→`seededNotebookFor`
  (L412-437, 782, 1550-1552); comments L409/779 "Space (room)"→"Notebook (room)"; L929 "Load Spaces"→"Load Notebooks".
- `chat.rs`: `MAX_SPACE_ITEMS_INLINED`→`MAX_NOTEBOOK_ITEMS_INLINED` (L763,789,792,795).
- `NotebooksListView.svelte:65` copy "Rooms you return to"→"Notebooks you return to".
- `contextMenuItems.ts`: `getAddToSpaceMenuItems`→`getAddToNotebookMenuItems`, `getWorkspaceMenuItems`→
  `getNotebookMenuItems`; update `SidebarNavItem.svelte` import+call (L10,184) + its comment.

### Retrieval scoping — LOCKED approach (Phase 4)
- Add `notebook_id: Option<&str>` + `ScopeMode { Boost, Strict }` to `search()` (query.rs).
  Resolve `role='library'` members → filter: `/page`,`/day`,`/source` → `(ontology, record_id)`;
  `/person`,`/org`,`/thing` → the existing `entities` path (`wiki_entity_refs` join — already built);
  files/URLs → new `ontology='asset'`. **Boost** = score CASE-multiplier in the z-fusion; **Strict**
  = AND-clause. Slots into query.rs filter_sql (~L187-213) + main SQL (~L219-268).
- **Free win:** `ToolContext.notebook_id` is ALREADY populated from chat.rs but the
  `semantic_search` tool ignores it. Thread it through (tool → `engine.search(notebook_id, Boost)`).
- **Decisions:** dynamic member resolution per search call (not snapshot — adding a material
  mid-chat should count); reject `/notebook/` as a `library` member (cycle risk; allow as `pin`).

### Citations — infra exists, not wired to chat (Phase 4)
- `SearchResult` already carries `ontology`+`record_id`+`title`+`preview`+`timestamp` → enough for
  **record-level** citation deep-links. Add `chunk_index` (in DB, just not serialized) for
  page/segment anchors (`/asset/{id}#page=N`).
- `CitedMarkdown.svelte` exists and renders source/page/highlight citations — but is wired to WIKI
  pages only. The chat stream has NO citation event. So chat citations = net-new plumbing (emit
  citation events from the agent stream + render CitedMarkdown in ChatView). Real work, Phase 4.

### Asset layer — validated (Phase 2/3)
- **HTTP Range requests are NOT supported** by the drive download path (streams whole file). This
  BLOCKS video/audio seeking and pdf.js streaming. Add `Storage::download_range` + a 206/Content-
  Range handler — a required Phase 2 backend task, not optional.
- `http_client` (reqwest) exists; no readability/web-fetch/YouTube yet. No PDF text extraction. The
  `transcription_resolution` applet (Gemini) exists and IS reusable for audio/video assets.
- Sidecars `app_assets` + `app_asset_text` keyed by `file_id`; new `ontology='asset'` registered in
  the indexer (`registered_ontologies`). Keep `/drive/file_{id}` canonical, add `/asset/{id}` alias.

### The Map — RE-SCOPED (no NER available)
- Entity resolution runs ONLY over structured data (calendar/email/location → `wiki_people`/
  `wiki_places` via `wiki_entity_refs`). There is **NO NER over free text** (pages, chats, extracted
  PDF text). So an auto concept-map over uploaded materials would be EMPTY.
- **Decision:** Map v1 = graph of entities **explicitly referenced** (`[@ref]` links + structured
  `wiki_entity_refs`) across the notebook's materials — buildable on existing data. Auto-NER-over-
  material-text is a separate later project, not a v1 dependency. (Keeps the payoff without the blocker.)

### Strict grounded chat mode (Phase 4)
- Add a `chat_mode`/scope value (`grounded` vs `open`) to `ChatRequest` (sits beside `agent_mode`);
  `grounded` → `ScopeMode::Strict` + a prompt line "answer only from this notebook's materials, cite
  everything, else say it's not in the materials." The PhD-trust toggle.

### Pins / tags — no change, no dead code
- Keep the three tiers distinct (Notebooks / Pins / Tags). Tag-browsing stays deferred; tags remain
  the future input to smart membership. No consolidation debt found.

## Deferred (explicitly not now)

- Export / portability / sharing links (involves sharing infra we don't want yet).
- OCR (born-digital PDFs carry text).
- External-material change-detection / re-fetch ("watch").
- Char-precise citation highlights.
- **Templates + suggest-a-notebook** (adoption on-ramp) — much later, not worth it now.
- Extra views (**Timeline**, **Outline/Board**) — built-in modules could be added later; not in v1.
- **Notebook views as customizable `view` applets** — explicitly out. Built-in default only.

---

## Phase C — Citations (source-anchored, Ref-native)  ← NEXT

**Goal:** when the model answers, a load-bearing claim carries a **named source chip** (the source's own name + icon) that **opens the real source** — the page, person, day, notebook source, or uploaded doc it rests on. Not a generic "5 matches" blob, not an opaque `[1]`.

**Decided (2026-07-10):** named source chips · click opens the real source (split-pane) · cite load-bearing claims only · applies everywhere retrieval runs (not just notebooks).

### Ref/citation unification (decided 2026-07-10)
**A citation is a ref — the same primitive, not a parallel system.** There is already one shared style contract (`ref-badge.css`) consumed by `Ref.svelte` (rendered chat markdown), `ChatInput.svelte` (composer), and CodeMirror `.cm-entity-link` (page editor). A cited source renders through that same `Ref` — one hover-preview, one open-beside, one look.

**Two densities, split by surface (not by page-vs-chat):**
- **Rendered output → Wikipedia-style link.** Accent-colored source name, hairline underline, tiny leading type icon, *no fill*. Used wherever markdown is rendered for reading — chat answers, embeds, previews (`Ref.svelte`). Load-bearing citations recur in prose, so they must disappear into the text.
- **Editable surface → filled pill (status quo).** The tinted rounded `@Name` pill stays in the composer and the CodeMirror editor, where a ref is a *token you inserted* and wants tangible chrome.
- Implementable boundary: **is this an editable surface (pill) or rendered output (link)?** Pages are an always-live CodeMirror editor, so their refs stay pills; a future read-only page render would use the link treatment.

**Click model for citations:** plain click **opens beside** (`windowShellStore.openRouteBeside`, splits the pane) — flip of today's peek-on-plain / open-on-⌘. Hover still peeks.

### The key realization: citations *are* Ref pills
Those four choices collapse the design. The app **already** renders `[text](/page/<id>)` / `/person/<id>` links inside `CitedMarkdown` as `<Ref>` pills — named chip, hover preview, click-to-source (the `link` snippet, `CitedMarkdown.svelte:153-166`; routes in `refRoutes.ts`). So the model doesn't need a bespoke citation syntax at all: **it cites by emitting a normal markdown link to the source's ref route**, and the existing `Ref` system does the chip, the preview, and the navigation for free. The numeric `[1]` / `InlineCitation` / `CitationPanel` stack is **not used for internal sources** — it stays only for `web_search` (external URLs have no Ref).

### Honest state of the current citation system (audited 2026-07-10)
- **Dormant for our case.** The main chat prompts (`prompt.rs` BASE / TOOL_USAGE / AGENT_MODE) never instruct the model to cite. Only deep-research/subagent modes do. So in normal chat the whole pill stack renders nothing.
- **Fragile by construction.** A citation `id` is a frontend positional counter (`1,2,3…`) the model never sees — it can't reliably map a claim to a source. Works for `web_search` only by coincidence of ordering. `semantic_search` collapses to one "N matches" pill.
- **Cruft to delete in this pass:** `buildCitationsFromGrounding` + `mergeCitationContexts` (Google-grounding path, ~90 lines) are exported but **never called** — dead. The `web_search` expansion is **duplicated** (`citationForSource` + inline loop, `builder.ts:187-299`). `buildPreview` has cases for tools that may no longer exist (`virtues_query_narratives`, `query_location_map`).

### The one hard problem: not every hit has a viewer
Ref routes exist for `page / day / person / place / org / thing / source / chat / notebook / file`. But a `semantic_search` hit is often a **raw ontology record** — an email, calendar event, transaction — which has **no viewer route**. "Open the real source" can't point at those. Resolution for v1:
- **Cite only hits that resolve to a viewable route.** Map each hit `(ontology, record_id)` → a ref URL where one exists (a page, the entity it's about via `wiki_entity_refs`, the ELT `source` it came from, an uploaded doc).
- **In a notebook, fall back to the owning member** (which is always viewable — that's what a member *is*).
- **Raw, unviewable hits still inform the answer but get no chip.** Better silent than a dead link. This keeps "open the source" a real promise, not a sometimes-404.

### Steps
1. ✅ **Emit a `ref` per result → the record itself (backend + new viewer).** Superseded the first entity-anchored cut: every raw `data_*` record now has its own viewer, so a citation points at the *actual source record*, not the entity it's about. `semantic_search` emits `ref = "/record/<ontology>/<record_id>"` for every hit (universal coverage — no "no chip" case). Backed by a new **data viewer**: `GET /api/records/:ontology/:record_id` (`api/records.rs`, registry-allowlisted `to_jsonb` fetch) + `DataView.svelte` (clean key-value render, `/record/<ontology>/<id>` tab type). This is also the Today/Day drill-down surface and the structured-record half of the asset track's viewer layer (files → `AssetView`, records → `DataView`). Entities become chips *inside* the record view, not the citation target. (Reverted the `wiki_entity_refs`/`resolve_refs` machinery — no longer needed.)
2. ✅ **Prompt contract (backend, global).** `<citations>` block added to `TOOL_USAGE_PROMPT` (included in every mode, `prompt.rs:382`): cite a load-bearing claim as a markdown link to the tool's returned `ref`, link text = source name; never fabricate a `ref`; no source twice in a row.
3. ✅ **Frontend link treatment + click.** `ref-badge.css` now has two treatments: `.ref-pill` (editable surfaces) and `.ref-link` (rendered output — accent name, hairline underline, small icon, prose size). `Ref.svelte` uses `.ref-link` and **plain click opens beside** (`openRouteBeside`); hover still peeks. No numeric-pill conflict — Streamdown skips `[text](url)`, so ref links render through the existing `link`→`Ref` path.
4. ✅ **Cleanup** (prior commit `6bf9169f`) — dead grounding helpers + re-exports removed, `web_search` expansion de-duped. Numeric `InlineCitation`/`CitationPanel` kept for `web_search` only.
5. ⬜ **Optional: per-answer "Sources" footer.** Dedupe distinct cited refs under the answer. Deferred — inline links already carry it.

**Still to verify (runtime, not compile):** end-to-end citation render (model emits `[title](/record/…)` → chip → click opens `DataView`); `DataView` field rendering across a few ontologies (email/calendar/transaction); the `id` column assumption (indexer keys record_id on `t.id`, so it should hold); and the notebook-scoped boost (separately untested).

**Follow-on ✅ (done):** `OntologyDataTable` rows now open `/record/<ontology>/<id>` beside on click (`onItemClick`), so the Day/Today page *and* the ontology detail page drill into the data viewer. `get_record` accepts the table name too (the table carries `data_calendar_event`, not `calendar_event`).

### Cut / deferred
- Char-precise span highlights (already deferred). Named chip per load-bearing claim is enough.
- No new schema. Citations ride in the markdown itself (a ref link) + `message.parts` as today.
- No new inline-citation UI. We're *removing* machinery, not adding it.

### Done when
Ask any question that retrieves → load-bearing claims show named source chips → clicking one opens the exact page / person / day / source it rests on. Raw records with no viewer inform the answer without a dead chip.

---

## Phase D — Asset track (upload → extract → embed → view → cite)

> **SUPERSEDED 2026-07-20 by [researcher-plan.md](./researcher-plan.md)** — the full
> researcher-archetype plan (D1 corpus · D2 annotation · D3 scholar metadata · D4 synthesis
> bridge). Key deltas: universal extraction on upload (Library = lens, not trigger);
> annotation-grade + Zotero-grade now IN v1; scope modes renamed Open/Scoped
> (`ScopeMode::Weighted|Exclusive`); Range + viewer page-jump already SHIPPED (Phases 2–3).
> The section below is kept for its inventory notes.

**Goal:** a notebook can hold an uploaded PDF / text / doc whose **native text** is extracted, embedded, retrievable (scoped, via Phase C's citations), and viewable. This is the researcher/PhD archetype. Bigger than C; sequence it after.

### What already exists (do not rebuild)
- **Upload is done.** `POST /api/drive/upload` (multipart, quota, SHA-256 dedup, disk storage) + frontend `uploadDriveFile()` with progress (`virtues-core/src/api/drive.rs`, `apps/web/src/lib/api/client.ts`).
- **Viewer is done and routed.** `AssetView.svelte` renders image/audio/video/PDF(iframe)/download by MIME (`apps/web/src/lib/components/tabs/views/AssetView.svelte:1-237`), fed by `getDriveFile()` + `/api/drive/files/:id/download`.
- **Indexer is generic.** The embedding cron embeds any ontology with an `embed_text_sql` (`virtues-core/src/search/indexer.rs`) — so a new document-chunk ontology gets embedded "for free" once its table exists.
- **Notebook membership already stores `/drive/file_<id>` URLs**; `resolve_notebook_scope` just skips them today (`virtues-core/src/search/query.rs:171`).

### The real missing pieces (in build order)
1. **Extraction (the core lift).** New async step: on upload (or a `document_extraction` cron, matching our cron-drain doctrine), parse **born-digital text only — no OCR** (already decided). PDF via a Rust text-extract crate (evaluate `pdf-extract` / `lopdf`; fall back to skip on scanned/no-text); `.txt`/`.md`/`.html` direct. Chunk to ~pages/paragraphs.
2. **`extracted_document_chunks` table (new migration).** `(id, file_id FK→app_drive_files, chunk_index, page_num, text, char_start, char_end, created_at)`. Add `extracted_at TIMESTAMPTZ` to `app_drive_files` as the extraction cursor/state.
3. **New ontology `uploaded_document`** in `crates/virtues-registry/src/ontologies.rs` with `embed_text_sql` selecting chunk text keyed by chunk id. Indexer picks it up; `search_embeddings.source_table = 'extracted_document_chunks'`.
4. **Wire `/drive/file_<id>` into notebook scope.** In `resolve_notebook_scope` (`query.rs:157-171`), resolve a `/drive/file_` member to its chunk record-ids so scoped retrieval + boost include the doc. Phase C citations then deep-link a hit to `AssetView` **at its `page_num`**.
5. **HTTP Range support (viewer quality).** Add `Range`/`206 Partial Content` to the drive download handler (`drive.rs:1045-1050`) so PDF/video seek works over the network. Currently full-file only.
6. **Viewer: extraction status + citation jump.** In `AssetView`, show "indexing…/N chunks" state and accept a `?page=` (or `#page=`) to jump the PDF iframe to a cited page.

### Sequencing within D
Extraction + chunk table + ontology (1–3) are the spine — ship that first and confirm an uploaded PDF becomes searchable. Then scope-wiring (4) lights up notebook grounding for docs. Range (5) and viewer polish (6) are quality passes, last.

### Cut / deferred (unchanged)
- OCR, external-URL/webpage/YouTube ingestion, change-detection "watch", export/sharing — all stay deferred.
- Non-text office formats (`.docx`/`.pptx`) after PDF+text prove out.

### Done when
Drop a PDF into a notebook → it extracts + embeds → ask the notebook → answer cites the PDF → clicking the pill opens the doc at the cited page.

---

## Build order across C + D

1. **Phase C** (citations) first — small, self-contained, immediately visible, unlocks trust in retrieval already built.
2. **Phase D spine** (extract → chunk → embed) — prove an uploaded PDF becomes searchable.
3. **Phase D wiring** — `/drive/file_` into notebook scope; C's citations now cover docs.
4. **Phase D polish** — Range requests, viewer page-jump, extraction status.

Commit checkpoint before starting (Phase 1 + 1.5 + scoped retrieval + UI redesign are still uncommitted).
