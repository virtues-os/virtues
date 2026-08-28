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
- **Federation beats the upload-bin**: a notebook holds the PDF, the advisor's email
  thread, the person, and last Tuesday as peers in one retrieval scope.
- **Citations = refs (already doctrine)**: a cited answer opens the exact page —
  and after D2, the exact passage. NotebookLM's dead-end "Source 3" chips are the
  thing our Phase C design was built to beat.
- **No export seam**: synthesis happens in Pages inside the same graph; BibTeX and
  annotations export out. NotebookLM's #1 power-user complaint cannot occur.

## Decisions locked (2026-07-20, incl. review revisions)

0. **No "Library" noun (renamed 2026-07-20).** The two-noun structure
   (a notebook *containing* a Library) contradicted the lens model. Things are
   simply **in the notebook** — user-facing verb is **"Add to notebook"**, the
   contents are **notebook items** (matching `app_notebook_items`). Internally
   every added item defaults to `role='library'` (grounds chat — that is what
   membership means); `role='pin'` survives schema-only for nav-only edges
   (e.g. related notebooks), with no chooser UI in v1. Where these docs say
   "Library", read "the notebook's items".
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
5. **OCR IS in v1 — classic tier on the NPU** (reversed 2026-07-20 after model
   research; see D5). PP-OCRv5 det+rec via QNN on the Q6A Hexagon; scanned PDFs
   flow `no_text → ocr → indexed`. The VLM parser tier stays out (see D5).
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
6. **Ingestion UX**: drag-drop files onto a notebook = upload + auto-add
   (`role='library'`). (Chat-attachment unification is NOT in D1 — see leftovers.)
7. **Status UI truth**: per-item chips in the notebook (`queued · extracting ·
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
   "Highlights" tab aggregating across the notebook's items.

## D3 — Scholar layer (Zotero-grade, local-only) · ~3 days

1. **Migration `app_document_meta`**: `(file_id PK/FK CASCADE, title, authors JSONB,
   year, venue, doi, citekey UNIQUE, abstract, meta_source, updated_at)`.
2. **Local metadata extraction** in the cron: PDF XMP/Info dict + first-page
   heuristics (title/author/DOI regex). **No network calls.**
3. **Metadata edit form** (heuristics will be wrong; this is the correction lane —
   and the quality gate for citekeys/BibTeX).
4. **Citekeys** (`author2026word`) + collision suffixes.
5. **References view**: the notebook's documents as a bibliography grid (UniversalDataGrid):
   authors · year · title · venue · status.
6. **BibTeX export** (`GET /api/notebooks/:id/bibtex`) + copy-citekey.
7. **Dedup**: SHA-256 (exists) + DOI match surfaced ("already in Drive").

## D4 — Synthesis bridge · ~0.5 wk

1. **Send highlight → Page**: blockquote + ref link (`?page=N&hl=<id>`, citekey
   suffix when meta exists). **Must reconcile with Yjs** (append via the Yjs doc /
   server-side update, not blind REST content replace — open-editor clobber risk).
2. **Bulk annotations export** (markdown per file / per notebook).
3. Polish: keyboard for colors, item counts, empty-states that teach the loop.

## Sequencing & risks

- D1 → D2 → D3 → D4; each ships standalone value.
- **Extractor quality** (reading order, headers/footers): trait boundary + week-1
  pdfium-vs-pure-Rust decision gate on Q6A (linux-aarch64).
- **Quote-landing misses** (extractor text vs pdf.js text differ enough that the
  quote isn't found): fallback = page-only landing; tune snippet length.
- **Yjs append** (D4): coordinate with pages' live layer before building.
- Embedding volume from universal extraction: acceptable (halfvec 384); watch via
  existing telemetry.

## Verify inline while building (no standalone spikes — decided 2026-07-20)

Extractor presumed fine (`pdfium-render`, pdfium-binaries ships linux-aarch64 for
the Q6A; MuPDF excluded — AGPL). Checks folded into the build itself:
- D1: quote-landing hit rate on a handful of real papers (whitespace-normalize +
  de-hyphenate; page-only fallback covers misses); sub-window dedup by record_id
  in search results (verify, add if missing); watch backfill pacing through the
  QNN embed daemon on the first real drive.
- D2: selection→normalized-rects zoom/DPR invariance before locking rect JSON.

## Open questions (decide during D1, cheap but real)

- Scope toggle: **per-chat, persisted** (recommended) vs per-message.
- Chunk hits in tool output should carry **doc title + page** so the model cites
  by name ("per Smith 2024, p. 6"), not by filename.
- **Trash semantics**: a notebook item whose file is in trash — show a
  "in trash" chip state, exclude from scope resolution.
- **Shared pages**: `shared_file_download` validates file membership in the
  shared page — confirm `?page/q` params flow through and no chunk/annotation
  data leaks via share tokens.
- Highlights spanning page boundaries: **disallow in v1** (anchor model is
  per-page).
- Add-to-notebook affordances: drag-drop (D1) + an "add from Drive" picker —
  picker ships when trivial, else fast-follow.

## D5 — OCR — ❌ CUT FROM v1 (decided 2026-07-21, after a full hardware spike)

**Decision: no OCR in v1 at all.** The spike proved the architecture works but
that quantized recognition can't reach research-corpus accuracy on the Q6A, and
— more decisively — that **the feature's reach is narrow**: pdfium already
covers every born-digital PDF. OCR only unlocks *scanned* PDFs and image
uploads. For a corpus that is mostly born-digital, it is redundant; for a small
number of scans, CPU-only OCR (7.4 s/page in a background cron) would have been
sufficient without any NPU work.

Measured on the real Dragon Q6A (QCS6490 / HTP v68) via Qualcomm AI Hub:

| Finding | Result |
|---|---|
| det on NPU | 23.9 ms/page, 225/225 layers NPU |
| rec backbone on NPU (split-head) | 1.92 ms/line, 164/164 layers NPU |
| rec neck + CTC (CPU) | 4.61 ms/line |
| split correctness (float) | **25/25 lines byte-identical** to monolithic |
| **w8a8 quantized accuracy** | **85.0%** char-acc |
| **w8a16 quantized accuracy** | **93.4%** (weak calib) → **94.11%** (rich calib) |
| w16a16 / int16 | ❌ won't compile on v68 (graph-compose error 14) |
| fp16 on NPU | ❌ unsupported (v68 is an integer engine) |

Richer calibration bought only **+0.7pp**, so the loss is **intrinsic to int8
weight quantization**, not a tuning artifact — quantized rec plateaus ~94%
(≈1 error per 17 chars), which corrupts citations and poisons embeddings.

Accuracy-preserving fallbacks, if OCR is ever revived: **det-NPU + rec-CPU-float
(~2.1 s/page, ~100% accuracy, 3.5× faster than all-CPU)**, an earlier split point
as a tunable accuracy/speed dial, or rec in fp16 on the Adreno 643 GPU (zero
quantization loss, speed untested). Full details + reusable AI Hub recipe in the
`project_ocr_npu_spike` memory.

### (historical) the original D5 plan

Decided 2026-07-20 after model research: OCR ships in v1 as the **classic
det+rec tier** — not a VLM. The user-visible win is singular: **scanned PDFs
and image uploads stop being dead ends.** Today a photographed page or scanned
paper extracts to `no_text` and vanishes from search/chat; after D5 it flows
into the *same* corpus loop as born-digital PDFs. D5 adds **zero new
retrieval/chat surface** — it is a new *producer* feeding the D1 pipeline. This
is why it precedes D3: it widens the core loop for everyone, not just the
citation-writing subset.

**Models: PP-OCRv5 mobile det+rec** (single-digit-M params, tens of MB —
~100× smaller than the VLM doc-parsers, ~95% of the value for printed scans).
ONNX via PaddleOCR/RapidOCR, Apache-2.0, char dict bundled (no
pdfium/MuPDF-style licensing landmine). English/Latin rec in v1; the
angle-classifier model and multilingual rec are cheap later knobs (printed
scans are upright).

### Locked decisions (2026-07-20)

1. **Runtime = ONNX Runtime (`ort` crate), QNN execution provider on the box,
   CPU EP on the DIY floor.** *Not* consolidated into `virtues-qnnd`. The
   deciding argument is the CPU floor: OCR is explicitly not appliance-exclusive,
   and ORT runs the **same ONNX on both tiers from one codebase** — swap the EP,
   nothing else. Consolidating into `virtues-qnnd` (raw QNN) would force a
   *second, separate* CPU implementation (the way embedding carries QNN-daemon +
   llama.cpp), and OCR models are CNNs/CRNNs — not GGUF, so llama.cpp isn't the
   CPU answer anyway. The known counter-risk: two runtime stacks then contend for
   the Hexagon HTP (gte-small in `virtues-qnnd` + OCR via ORT QNN EP). Mitigation
   is sequenced, not upfront — prove correctness on ORT **CPU EP** first, flip to
   **QNN EP**, and lift OCR into `virtues-qnnd` *only if* NPU-sharing actually
   hurts. Correctness-first, consolidate-if-needed.
2. **Scan highlighting = page-level in D5, synthetic text layer as D5.5
   fast-follow.** v1 landing on OCR'd docs is page-level (`?page=N`); `?q=`
   quote-flash and D2 highlighting do **not** work on a scan (it's an image —
   no pdf.js text layer to select or flash). D5 stores word boxes; **D5.5**
   renders a synthetic transparent text layer from them → full D2 parity
   (selectable, highlightable, quote-flash). Split this way so D5 stays focused
   on "get the text into the corpus" while the first release doesn't visibly
   regress the highlighting users just got in D2.
3. **Quantization = budget a real calibration + accuracy eval pass.** HTP needs
   w8a8; quantized **rec** can drop accuracy on small fonts — the one place D5
   could disappoint. Calibrate on a handful of scanned crops and eval against a
   known-good scan, don't ship RapidOCR's quantized weights on a vibe check.

### Pipeline (bolts onto the existing extractor)

- **The `no_text` queue is the OCR queue** — it already exists, it just needs a
  consumer. Text extraction runs as today; a PDF whose pdfium text is <~N
  chars/page average → `no_text`, which (when OCR is enabled) becomes
  `ocr_pending`. Image uploads (PNG/JPG) route here directly — no pdfium raster.
- The extraction cron drains `ocr_pending`: pdfium (already the extractor dep)
  rasterizes each page (~250 DPI, **streamed one page at a time** — a 250 DPI
  page is ~17 MB raw, never hold the whole doc) → **det** (line boxes) → **rec**
  (text/line) → reading-order sort → page text → the *existing* chunker →
  *existing* embeddings. `extraction_status` gains `ocr_pending|ocr_done`.
- **rec dominates cost**, not det (det = 1 inference/page; rec = 1 per detected
  line, ~40–60/page). **Batch rec by width bucket** — the biggest single perf
  lever on both CPU and NPU.
- **Runs inside the existing extraction cron** — models loaded once per drain,
  released on exit. No new resident daemon in v1; load cost (<1s) amortizes over
  a bursty batch (upload a scan → many pages at once). Promote to a warm resident
  service only if single-page drains become common.
- **Store normalized word boxes** with the OCR text (`app_document_ocr`:
  per-page `text` + `boxes` JSONB). det+rec gives boxes for free; VLMs mostly
  don't. This is the data D5.5's synthetic text layer needs.
- **Honest progress**: chips show `ocr — 41%` (~1s/page on NPU, a few hundred
  ms/page on CPU; a 300-page scan takes minutes and must never look stuck).

### Sub-phases

- **D5.1** — ORT wiring + model packaging (det/rec ONNX + dict); CPU EP proving
  text on a known scan. ~1 d
- **D5.2** — full pipeline: raster → det → rec → reading-order assembly → page
  text into chunker; `ocr_pending/ocr_done` + progress chips. ~1–1.5 d
- **D5.3** — QNN EP on Dragon; quantization + accuracy eval; rec batching. ~1 d
- **D5.4** — word-box storage (`app_document_ocr`) + image-upload routing. ~0.5 d
- **D5.5** *(fast-follow)* — synthetic text layer → D2 parity on scans. ~0.5–1 d

**Named non-goal — the VLM parser tier** (vision-language models: an LLM with
an image encoder that *writes out* the page — tables/math/handwriting/layout).
Current leaders: PaddleOCR-VL-0.9B/1.6 (OmniDocBench SOTA), Surya 2 (650M —
the one to watch for a future CPU-cron tier), DeepSeek-OCR, OCRFlux-3B.
None fit the QCS6490 NPU (they target 8-Gen-2-class silicon and up); future
paths are CPU cron, a permissioned-cloud lane (Crossref posture), or beefier
box hardware. Not v1.

## Explicit non-goals (v1)

Whiteboard/spatial canvas · VLM document-parser tier (see D5 — classic-tier OCR
IS in v1) · Crossref/network metadata enrichment (opt-in
later) · URL/YouTube snapshot ingestion (separate lane) · Zotero/BibTeX importer
(fast-follow; schema is receiver-ready) · literature discovery (Elicit's corpus) ·
audio overviews / studio artifacts · auto-NER concept maps (prose ER paused) ·
draft/version succession (v2-of-a-manuscript linking — named here so it's a
decision, not an oversight) · chunks in ⌘K global search · server citation stream
event · **chat-attachment → Drive unification** (own project: touches message
parts schema + clients; the old Phase-5 leftover list).
