# The Researcher — complete archetype plan (Phase D, superseding scope) · v2

Status: planned 2026-07-20, **revised same day after a code-verified review** (claims
below checked against the codebase, not inherited from older plans). Extends and
partially supersedes the Phase D section of [notebooks-plan.md](./notebooks-plan.md).
North star: the researcher / PhD / academic archetype, built to be **complete in v1**
— corpus, reading/annotation, scholarly metadata, and the synthesis bridge.
NotebookLM's trust loop + Heptabase's highlight-to-note loop + Zotero's reference
layer, over the life-graph, on the box.

## Why us (the wedge)

- **Privacy is the architecture — stated honestly.** Your corpus, index, and
  annotations never leave the box. Chat excerpts go only to the model you chose
  (gateway/BYO-key) at ask-time; a future local-LLM slot completes the story.
  Never overclaim "nothing leaves the box" — retrieval is local, inference today
  is not. (Researcher privacy concern is at 58% and rising; "don't put unpublished
  work in cloud AI" is standard advice. An appliance is the only honest answer.)
- **Federation beats the upload-bin**: a Library holds the PDF, the advisor's email
  thread, the person, and last Tuesday as peers in one retrieval scope.
- **Citations = refs (already doctrine)**: a cited answer opens the exact page —
  and after D2, the exact passage. NotebookLM's dead-end "Source 3" chips are the
  thing our Phase C design was built to beat.
- **No export seam**: synthesis happens in Pages inside the same graph; BibTeX and
  annotations export out. NotebookLM's #1 power-user complaint cannot occur.

## Decisions locked (2026-07-20, incl. review revisions)

1. **Universal extraction on upload.** Every text-bearing drive file is extracted,
   chunked, and embedded — the whole drive is corpus. The Library is a *lens*
   (scope + up-weight), not a container that triggers ingestion. Supersedes
   notebooks-plan's "lazy, on add-to-Library". Existing drive files are
   **backfilled** (migration seeds `extraction_status='pending'`; cron drains).
2. **Naming: "Open" vs "Scoped" chat** (user-facing); internal
   `ScopeMode::Weighted | Exclusive`. Old Boost/Strict names retired.
   **Weighted stays an ADDITIVE z-space boost** — the existing fusion adds (z-scores
   go negative; multiplying inverts rankings — code rationale at query.rs:55-57 is
   correct and kept). Default = Open; one visible per-chat toggle to Scoped
   (hard filter + grounded prompt line).
3. **Anchoring is quote-based, not offset-based.** Rust-extractor char offsets do
   NOT reliably index pdf.js's text layer (different reading order/whitespace) —
   verified risk. Therefore:
   - **Chunk citations are self-contained**: `?page=N&q=<short quote snippet>` —
     landed by text-searching the pdf.js layer. No resolve endpoint; survives
     re-extraction (chunk ids are ephemeral by design).
   - **User highlights** anchor by quote + prefix/suffix context (W3C
     TextQuoteSelector style) + page + normalized rects captured in-viewer at
     creation; deep link `?page=N&hl=<annotation_id>` (annotations are durable rows).
   - Chunks still store `char_start/char_end` into the *extractor's* canonical text
     for bookkeeping — never for viewer landing.
4. **No whiteboard.** Heptabase's spatial canvas is out; its loop (highlight →
   excerpt → note) is in, targeting Pages.
5. **No OCR** (unchanged). Scanned PDFs detected and labeled "no text layer".
6. **No Crossref in v1** (privacy: titles/DOIs of what you read — and unpublished
   drafts' titles — must not leak by default). Local metadata heuristics + a
   **manual metadata edit form**. Crossref returns later as explicit opt-in.
7. **Chunking**: paragraph-aware, ~400–600 tokens, 10–15% overlap, chunks may cross
   pages; anchor `page_num` (starting page) + char range + leading quote snippet.
   The generic indexer sub-windows each chunk row to ~128-token embeddings — this
   is **accepted and good** (multi-vector per chunk row, better recall, zero work).
   Embedder = gte-small-384 (invariant).
8. **No chat-stream protocol change in D1.** A full client-side citation pipeline
   already exists (`semantic_search` returns `ref` per hit; `citations/builder.ts`
   + `CitedMarkdown` render chips from tool-output parts). Chunk hits emitting
   `/drive/file_{id}?page=N&q=…` refs light it up with zero protocol work. A server
   citation event is optional future polish — and if ever added, check the native
   iOS app (lives outside this repo) as a stream consumer first.

## Verified state of the codebase (2026-07-20 review — do not re-derive)

- `ToolContext.notebook_id` is threaded end-to-end and **already used**: an additive
  notebook boost ships today (query.rs:242-253, `NOTEBOOK_BOOST` z-boost).
- `resolve_notebook_scope` exists but uses **ALL members** — the `role` column
  (`'library'|'pin'`, migration 0032) is a **schema stub**: the add-member API never
  sets it and search never reads it. `/drive/file_` members are stored but skipped.
- Indexer is generic via `EmbeddingConfig.embed_text_sql` over registry ontologies;
  `source_table` is auto-written. Vectors live in `search_vectors` (halfvec, dims
  set programmatically). A chunk-row ontology embeds with `embed_text_sql: "t.text"`.
- `search_embeddings` has **no FK** to source tables. Indexer has stale-chunk GC
  (indexer.rs:438-465) — **verify** it collects embeddings for deleted chunk rows;
  add manual delete-by-record_id only if it doesn't.
- Hard purge of drive files is a plain DELETE; FK `ON DELETE CASCADE` (pattern
  exists, 0003:156) covers the new sidecar tables automatically.
- Chat = SSE (AI-SDK v6 events); no citation event; citations built client-side.
- Pages have REST `update_page` (full-content replace) **and a live Yjs layer** —
  programmatic append must reconcile with Yjs or it can clobber an open editor.
- SearchModal (⌘K) is a client-side title filter over loaded stores — corpus search
  is a separate lane; surfacing chunks there is future work, not D1.
- `app_assets` / `app_asset_text` do not exist (old-plan names; nothing to reuse).
- Shipped already (Phases 1–3): streaming byte-path + Range/206 + inline; PdfPane
  (pdf.js text layer, `?page=N`, last-page memory); text/CSV panes; real-disk quota.

---

## D1 — Corpus (extract → chunk → embed → scope → cite) · ~2 wks

The trust loop: drop PDFs on a notebook → watch them index → ask → click citation
→ land on the page.

1. **Extraction pipeline** (`document_extraction` cron, cron-drain doctrine):
   - Allowlist: `pdf`, `docx` (unzip + document.xml text pull — researchers live in
     Word), `txt`, `md`, `html` (tag-strip). Size cap (~50MB extracted text).
   - PDF extractor behind a trait: evaluate `pdfium-render` (layout quality; check
     linux-aarch64 binary availability — **prod box is Q6A/QCS6490, not Jetson**)
     vs pure-Rust fallback in week 1. Avoid MuPDF (AGPL).
   - Scanned/no-text → `extraction_status='no_text'` (honest, never silent).
   - State on `app_drive_files`: `extraction_status`
     (`pending|extracting|done|no_text|failed|skipped`) + `extracted_at`;
     migration seeds `pending` for existing files (backfill). Failed rows get a
     **re-extract** affordance in UI.
   - Telemetry: counts and timings only — never content.
2. **Migration `extracted_document_chunks`**: `(id, file_id FK ON DELETE CASCADE,
   chunk_index, page_num, char_start, char_end, quote_head, text, created_at)`.
   (`quote_head` = leading snippet for self-contained citation links.)
   No page-text table — quote anchoring removes the need.
3. **Ontology `uploaded_document`** (registry) with `embed_text_sql: "t.text"` →
   indexer embeds for free. Verify stale-GC covers deleted chunk rows (above).
4. **Scope finishing** (not from scratch): set `role='library'|'pin'` in the
   add/update member API + UI; filter `resolve_notebook_scope` to `role='library'`;
   resolve `/drive/file_` members → chunk record filter; add
   `ScopeMode::Exclusive` (AND-clause) beside the existing additive boost;
   `chat_mode: open|scoped` on ChatRequest; Scoped adds the grounded prompt line.
5. **Citations via the existing pipeline**: `semantic_search` chunk hits carry
   `ref = /drive/file_{id}?page=N&q=<quote_head>`; model cites per refs doctrine;
   `CitedMarkdown` renders; PdfPane lands on the page (D2 upgrades to passage).
6. **Ingestion UX**: drag-drop files onto a notebook's Library = upload + auto-add
   (`role='library'`). (Chat-attachment unification is NOT in D1 — see leftovers.)
7. **Status UI truth**: per-material chips in the Library (`queued · extracting ·
   indexed (14 pages) · no text layer · failed·retry`) + an indexed column in
   DriveView; aggregate count when a bulk drop is draining.

## D2 — Reading & annotation · ~1.5 wks

1. **Migration `app_annotations`**: `(id, file_id FK CASCADE, page_num, quote_text,
   quote_prefix, quote_suffix, rects JSONB (normalized page-space), color, note_md,
   created_at, updated_at)`. Global to the file; visible from every notebook.
2. **PdfPane annotation layer**: selection → floating toolbar (colors + note);
   overlay rendering from rects (multiply blend); click → popover (markdown note,
   edit/delete). TextPane gets the same quote anchors (simpler rendering).
3. **Find-in-document** (⌘F) in PdfPane — table stakes for a 300-page PDF; text is
   already client-side via pdf.js.
4. **Precise citation landing**: `?page=N&q=…` (chunks) and `?page=N&hl=<id>`
   (annotations) → scroll to page, text-search the pdf.js layer for the quote,
   flash/underline the passage. Fallback when quote not found: land on page only.
5. **Annotations retrievable**: ontology `document_annotation`
   (`embed_text_sql` over `quote_text || note_md`) — "what did I highlight about X".
6. **Annotation index views**: per-file rail in AssetView (jump list); notebook
   "Highlights" tab aggregating across the Library.

## D3 — Scholar layer (Zotero-grade, local-only) · ~3 days

1. **Migration `app_document_meta`**: `(file_id PK/FK CASCADE, title, authors JSONB,
   year, venue, doi, citekey UNIQUE, abstract, meta_source, updated_at)`.
2. **Local metadata extraction** in the cron: PDF XMP/Info dict + first-page
   heuristics (title/author/DOI regex). **No network calls.**
3. **Metadata edit form** (heuristics will be wrong; this is the correction lane —
   and the quality gate for citekeys/BibTeX).
4. **Citekeys** (`author2026word`) + collision suffixes.
5. **References view**: notebook Library as bibliography grid (UniversalDataGrid):
   authors · year · title · venue · status.
6. **BibTeX export** (`GET /api/notebooks/:id/bibtex`) + copy-citekey.
7. **Dedup**: SHA-256 (exists) + DOI match surfaced ("already in Drive").

## D4 — Synthesis bridge · ~0.5 wk

1. **Send highlight → Page**: blockquote + ref link (`?page=N&hl=<id>`, citekey
   suffix when meta exists). **Must reconcile with Yjs** (append via the Yjs doc /
   server-side update, not blind REST content replace — open-editor clobber risk).
2. **Bulk annotations export** (markdown per file / per notebook).
3. Polish: keyboard for colors, Library counts, empty-states that teach the loop.

## Sequencing & risks

- D1 → D2 → D3 → D4; each ships standalone value.
- **Extractor quality** (reading order, headers/footers): trait boundary + week-1
  pdfium-vs-pure-Rust decision gate on Q6A (linux-aarch64).
- **Quote-landing misses** (extractor text vs pdf.js text differ enough that the
  quote isn't found): fallback = page-only landing; tune snippet length.
- **Yjs append** (D4): coordinate with pages' live layer before building.
- Embedding volume from universal extraction: acceptable (halfvec 384); watch via
  existing telemetry.

## Spikes required (proofs before/while building — none of these are done)

- **S1 · Extractor bake-off on Q6A (gates D1, week 1).** `pdfium-render` with a
  linux-aarch64 pdfium binary (pdfium-binaries publishes arm64) vs pure-Rust
  fallback, run on the Q6A: 10 real papers (two-column, math-heavy, footnoted);
  eyeball reading order, header/footer noise, chunk quality; measure speed + RAM.
  Include the DOCX quick-check (unzip + document.xml). MuPDF excluded (AGPL).
- **S2 · Quote-landing hit rate (gates the citation trust loop).** Rust-extracted
  snippets searched against pdf.js `getTextContent` with whitespace normalization
  + de-hyphenation across the same 10 papers. Target ≥95% found; tune snippet
  length; page-only fallback covers misses. Cheap browser-side script.
- **S3 · Selection → normalized rects (gates D2 schema).** Capture multi-span
  text-layer selections as normalized page-space quads; verify zoom/DPR
  invariance on re-render. Half-day.
- **S4 · Fusion sanity with chunks in the mix.** Mixed query over life-data +
  document chunks: do ranks interleave sensibly; and when the indexer's
  sub-windows of ONE chunk all match, does search collapse them to one result
  (dedup by record_id) — verify, add dedup if not.
- **S5 · Backfill throughput on Q6A.** A 300-page book + a 50-PDF drop:
  end-to-end time through extraction (CPU) + embedding (QNN NPU daemon), cron
  pacing, no starvation of other crons.

## Open questions (decide during D1, cheap but real)

- Scope toggle: **per-chat, persisted** (recommended) vs per-message.
- Chunk hits in tool output should carry **doc title + page** so the model cites
  by name ("per Smith 2024, p. 6"), not by filename.
- **Trash semantics**: a Library member whose file is in trash — show a
  "in trash" chip state, exclude from scope resolution.
- **Shared pages**: `shared_file_download` validates file membership in the
  shared page — confirm `?page/q` params flow through and no chunk/annotation
  data leaks via share tokens.
- Highlights spanning page boundaries: **disallow in v1** (anchor model is
  per-page).
- Add-to-Library affordances: drag-drop (D1) + an "add from Drive" picker —
  picker ships when trivial, else fast-follow.

## OCR — position (asked and answered)

**Not in v1, but the pipeline is OCR-ready by design.** `extraction_status =
'no_text'` files ARE the OCR queue: the extractor sits behind a trait, so an OCR
backend slots in without schema or pipeline changes, and scanned PDFs already
render fine in PdfPane (they're images) — reading works today, only retrieval
waits. When we do it (fast-follow spike, not now): candidates are `ocrs`
(pure-Rust, CPU, could run on-box), tesseract (C dep, weak on academic layouts),
or a permissioned cloud-vision path (same opt-in posture as Crossref). A
QNN-hosted vision model on the Q6A NPU is a research project, not a plan item.
The scanned-book/archival corpus (humanities researchers) is real demand — the
v1 line is honesty ("no text layer") rather than silent emptiness.

## Explicit non-goals (v1)

Whiteboard/spatial canvas · OCR · Crossref/network metadata enrichment (opt-in
later) · URL/YouTube snapshot ingestion (separate lane) · Zotero/BibTeX importer
(fast-follow; schema is receiver-ready) · literature discovery (Elicit's corpus) ·
audio overviews / studio artifacts · auto-NER concept maps (prose ER paused) ·
draft/version succession (v2-of-a-manuscript linking — named here so it's a
decision, not an oversight) · chunks in ⌘K global search · server citation stream
event · **chat-attachment → Drive unification** (own project: touches message
parts schema + clients; the old Phase-5 leftover list).
