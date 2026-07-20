# The Researcher — complete archetype plan (Phase D, superseding scope)

Status: planned 2026-07-20. Extends and partially supersedes the Phase D section of
[notebooks-plan.md](./notebooks-plan.md). North star: the researcher / PhD / academic
archetype, built to be **complete in v1** — corpus, reading/annotation, scholarly
metadata, and the synthesis bridge. NotebookLM's trust loop + Heptabase's
highlight-to-note loop + Zotero's reference layer, over the life-graph, on the box.

## Why us (the wedge)

- **Privacy is the architecture**: unpublished manuscripts never leave the box. The
  2025→2026 surveys show researcher privacy concern at 58% and rising; "don't put
  unpublished work in cloud AI" is now standard advice. Nobody in the NotebookLM
  class can match an appliance.
- **Federation beats the upload-bin**: a Library holds the PDF, the advisor's email
  thread, the person, and last Tuesday as peers in one retrieval scope.
- **Citations = refs (already doctrine)**: a cited answer opens the exact page —
  and after D2, the exact passage. NotebookLM's dead-end "Source 3" chips are the
  thing our Phase C design was built to beat.
- **No export seam**: synthesis happens in Pages inside the same graph. NotebookLM's
  #1 power-user complaint (no export, citations don't survive copy) cannot occur.

## Decisions locked (2026-07-20)

1. **Universal extraction on upload.** Every text-bearing drive file is extracted,
   chunked, and embedded — the whole drive is corpus. The Library is a *lens*
   (scope + up-weight), not a container that triggers ingestion. This supersedes
   notebooks-plan's "extraction is lazy, on add-to-Library." Rationale: purer
   reading of notebooks-as-lens; makes add-to-Library instant; enables drive-wide
   "which of my files mention X" with no ceremony.
2. **Naming: "Open" vs "Scoped" chat** (user-facing). Internal enum
   `ScopeMode::Weighted` (search everything, multiply Library members' scores) and
   `ScopeMode::Exclusive` (hard-filter to Library members + grounded prompt line).
   The old plan-doc names Boost/Strict are retired. Default = Open/Weighted;
   one visible toggle per chat to Scoped.
3. **No whiteboard.** Heptabase's spatial canvas is out (new editor paradigm).
   Its *loop* — highlight → excerpt → note — is in, targeting Pages.
4. **No OCR** (unchanged). Born-digital text only; scanned PDFs are detected and
   honestly labeled "no text layer", never silently empty.
5. **Annotation-grade, Zotero-grade in v1** (reverses the earlier deferral).
   Char-precise anchors ship in D1's schema so D2 lands on them without rework.
6. **Chunking**: paragraph-aware, ~400–600 tokens, 10–15% overlap, chunks may
   cross pages; every chunk anchors `page_num` (starting page) + `char_start/end`
   into the file's canonical extracted text. Embedder = gte-small-384 (invariant).

## What already exists (do not rebuild)

- Streaming upload + ranged download + `?disposition=inline` (Phase 2, shipped).
- PdfPane: pdf.js text layer, `?page=N` deep links, last-page memory (Phase 3).
- Text/CSV/markdown panes; drive click-to-open-beside (Phase 1).
- Generic embedding indexer (`search/indexer.rs`) — any ontology with
  `embed_text_sql` gets embedded by the existing cron.
- `ToolContext.notebook_id` populated from chat.rs (currently ignored by search).
- `CitedMarkdown.svelte` renders refs; ref-chips open-beside (Phase C).
- Notebook membership stores `/drive/file_<id>` URLs (skipped by scope resolution
  today at `search/query.rs:171`).

---

## D1 — Corpus (extract → chunk → embed → scope → cite) · ~1.5–2 wks

The trust loop: drop PDFs → watch them index → ask → click citation → land on page.

1. **Extraction pipeline** (`document_extraction` cron, cron-drain doctrine):
   - Allowlist: `pdf`, `txt`, `md`, `html`(strip tags), `json`? (no), code files (no —
     retrievable via chunks adds noise; revisit). Size cap (~50MB extracted-text cap).
   - PDF extractor: evaluate `pdfium-render` (layout-quality reading order; C lib,
     needs pdfium binary per-arch — check Jetson) vs pure-Rust `pdf-extract`/`lopdf`
     (no native dep, weaker order). Decision gate in week 1; wrap behind a trait so
     the choice is swappable. Scanned/no-text PDFs → `extraction_status='no_text'`.
   - State on `app_drive_files`: `extraction_status` (`pending|extracting|done|no_text|failed|skipped`)
     + `extracted_at`. Trash/purge cascades chunks.
2. **Migration `extracted_document_chunks`**: `(id, file_id FK, chunk_index,
   page_num, char_start, char_end, text, created_at)` + per-page canonical text
   table `extracted_document_pages (file_id, page_num, text)` — pages are the
   anchor substrate annotations (D2) and precise-landing need.
3. **Ontology `uploaded_document`** (virtues-registry) with `embed_text_sql` over
   chunk text → indexer embeds for free. `search_embeddings.source_table='extracted_document_chunks'`.
4. **Scope wiring** (`search/query.rs`): resolve `role='library'` members →
   files → chunk filter; pages/days/sources → `(ontology, record_id)`; entities →
   existing `wiki_entity_refs` path. `ScopeMode::Weighted` = score multiplier in
   z-fusion; `Exclusive` = AND-clause. Thread `ToolContext.notebook_id` through
   `semantic_search`. Dynamic member resolution per call (locked earlier).
5. **Chat citation events** (the real work): emit citation events from the agent
   stream when tool results carry refs; render `CitedMarkdown` in ChatView. Chunk
   hits cite `/drive/file_{id}?page=N`. `chat_mode: open|scoped` on ChatRequest;
   Scoped adds the grounded prompt line + `ScopeMode::Exclusive`.
6. **Library/drive status UI**: per-material chips — `queued · extracting ·
   indexed (14 pages) · no text layer · failed` — in the notebook Library list AND
   as a column in DriveView. No silent states.

## D2 — Reading & annotation (the surface becomes ours) · ~1–1.5 wks

1. **Migration `app_annotations`**: `(id, file_id FK, page_num, char_start,
   char_end, quote_text, rects JSONB, color, note_md, created_at, updated_at)`.
   Global to the file (visible from every notebook); `rects` = normalized page-space
   quads captured from the pdf.js text-layer geometry at creation (render-scale
   independent).
2. **PdfPane annotation layer**: text-selection → floating toolbar (highlight
   colors + "note"); overlay rendering of rects (multiply blend); click highlight →
   popover with note (markdown), edit/delete. TextPane gets the same anchors
   (char-range highlights over plain/md text — simpler rendering, same table).
3. **Annotations are retrievable**: ontology `document_annotation` with
   `embed_text_sql` over `quote_text + note_md`. Your own marks out-rank raw text
   naturally (they're denser signal); "what did I highlight about X" just works.
4. **Char-precise citation landing**: extend deep links to
   `?page=N&hl=<char_start>-<char_end>`; PdfPane scrolls to page, maps the char
   range to text-layer geometry, flashes/underlines the passage. Citation events
   from D1 upgrade automatically (chunks already carry char ranges).
5. **Annotation index views**: per-file annotation rail in AssetView (jump list);
   notebook-level "Highlights" tab aggregating across the Library.

## D3 — Scholar layer (Zotero-grade metadata) · ~1 wk

1. **Migration `app_document_meta`**: `(file_id PK/FK, title, authors JSONB,
   year, venue, doi, citekey UNIQUE, abstract, meta_source, updated_at)`.
2. **Metadata extraction** in the extraction cron: PDF XMP/Info dict + first-page
   heuristics (title/author/DOI regex). Optional **Crossref enrichment** action
   (network call → permissioned/settings-gated like Unsplash; public metadata only).
3. **Citekey generation** (`author2026word`) + collision handling.
4. **References view**: notebook Library as bibliography table (UniversalDataGrid
   per list doctrine): authors · year · title · venue · status. Sort/filter.
5. **BibTeX export** of a notebook's Library (`GET /api/notebooks/:id/bibtex`) +
   copy-citekey affordance. This is the anti-NotebookLM exit door.
6. **Dedup**: existing SHA-256 + DOI match surfaced in UI ("already in Drive").
7. Schema designed to receive a future **Zotero/BibTeX importer** (fast-follow,
   not v1).

## D4 — Synthesis bridge (Heptabase's loop, our surfaces) · ~0.5 wk

1. **Send highlight → Page**: from a highlight popover or the annotation rail,
   append to a chosen Page: blockquote of the quote + ref link to
   `/drive/file_{id}?page=N&hl=…` (+ citekey suffix when D3 metadata exists).
   Reading feeds writing inside one graph; no export seam.
2. **New-page-from-notebook** flows and polish: keyboard for highlight colors,
   counts on the Library, empty-states that teach the loop.

## Sequencing & risks

- Order: D1 → D2 → D3 → D4. Each phase ships standalone value; D1 unblocks all.
- **Risk: PDF extraction quality** (reading order, headers/footers) — the trait
  boundary + early pdfium-vs-pure-Rust decision gate contains it. Jetson arm64
  availability of pdfium binaries must be checked in week 1.
- **Risk: rect mapping across zoom/DPR** in D2 — store normalized page-space
  coords, never pixel-space; map through the same viewport transform pdf.js uses.
- **Risk: agent-stream protocol change** for citation events — coordinate with
  chat streaming consumers (web SPA + iOS HTTPWire).
- Embedding volume: universal extraction grows `search_embeddings`; acceptable
  (gte-small 384-d halfvec), monitor via existing telemetry.

## Explicit non-goals (v1)

Whiteboard/spatial canvas · OCR · URL/YouTube snapshot ingestion (separate lane,
doctrine-approved) · Zotero importer (fast-follow) · literature *discovery*
(Elicit's corpus — not a box's job) · audio overviews / studio artifacts ·
auto-NER concept maps (prose ER is paused).
