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

### Naming (resolved)

- The materials in a notebook = its **Library** ("add to Library"). NOT "Sources" (already used
  for credential connections) and NOT "References" (collides with the existing `[@ref]` entity-link
  system). Internal DB role value = `library`.
- **Library = anything retrievable, not just files.** Files, external snapshots, pasted text,
  AND internal entities/data/days/people — a Library member's chunks are resolved into the
  notebook's retrieval scope regardless of type. This is the federation superpower (don't ape
  NotebookLM's upload-bin). The real split is **`library` (retrievable, grounds chat) vs `pin`
  (nav-only shortcut)** — not files vs entities.
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
- **Extraction is native-text only, lazy, on add-to-Library.** No OCR for now (born-digital PDFs
  carry text). Extraction is a property of *being a material*, not of *being a file*.
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
- Notebook home is a **built-in default view** (plain Svelte components — NOT the `view`-action
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
Actions
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
  **[Wiki, Drive, Actions]**.
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

### Phase 1.5 — "pristine" cleanup (cheap; do before Phase 2)
- **Surface the new columns.** `instructions`, `archived_at` (on `app_notebooks`) and item `role`
  are in the DB but ABSENT from the Rust structs (`api/notebooks.rs`) and TS interfaces
  (`client.ts`) — silent-drop landmine. Add to structs + SELECTs + client types now (read path),
  even before their UI.
- **Kill residual "space"/"room" naming.** `ChatView.svelte` vars `chatSpaceId`/`seededSpaceFor` +
  comments; `NotebookDetailView` CSS `--room-accent`/`.room-*`; `NotebooksListView:65` copy "Rooms
  you return to"; `contextMenuItems` fn names `getAddToSpaceMenuItems`/`getWorkspaceMenuItems`;
  `SidebarNavItem:183` comment. All cosmetic, all confusing — sweep them.
- **Split memo vs instructions (semantics).** `current_status` = transient "state of the room"
  memo; `instructions` = persistent behavior. Today only the memo exists and is inlined by
  `build_notebook_context` (chat.rs). Decide now: instructions goes in the system-prompt preamble
  (persistent), memo stays the catch-up line. Wire both once `instructions` is on the struct.

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
  `transcription_resolution` action (Gemini) exists and IS reusable for audio/video assets.
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
- **Notebook views as customizable `view` actions** — explicitly out. Built-in default only.
