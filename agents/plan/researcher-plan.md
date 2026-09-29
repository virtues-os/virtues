# The Researcher — complete archetype plan (Phase D, superseding scope) · v2

Status: planned 2026-07-20, revised the same day after a code-verified review.
**D1 (corpus), D2 (reading and annotation) and D4.1 (send highlight to a Page)
are built**; what remains is D3, the rest of D2 and D4, and the project steps
at the end. Projects themselves are recorded in
[projects.md](../record/projects.md). North star: the researcher / PhD /
academic archetype — corpus, reading/annotation, scholarly metadata, and the
synthesis bridge. NotebookLM's trust loop + Heptabase's highlight-to-note loop +
Zotero's reference layer, over the life-graph, on the box.

## Why us (the wedge)

- **Privacy is the architecture — stated honestly.** Your corpus, index, and
  annotations never leave the box. Chat excerpts go only to the model you chose
  (gateway/BYO-key) at ask-time; a future local-LLM slot completes the story.
  Never overclaim "nothing leaves the box" — retrieval is local, inference today
  is not. (Researcher privacy concern is at 58% and rising; "don't put unpublished
  work in cloud AI" is standard advice. An appliance is the only honest answer.)
- **Federation beats the upload-bin**: a project holds the PDF, the advisor's email
  thread, the person, and last Tuesday as peers in one retrieval scope.
- **Citations = refs (already doctrine)**: a cited answer opens the exact page —
  and after D2, the exact passage. NotebookLM's dead-end "Source 3" chips are the
  thing the citation design was built to beat.
- **No export seam**: synthesis happens in Pages inside the same graph; BibTeX and
  annotations export out. NotebookLM's #1 power-user complaint cannot occur.

## Decisions locked (2026-07-20, incl. review revisions)

0. **No "Library" noun (renamed 2026-07-20).** The two-noun structure
   (a project *containing* a Library) contradicted the lens model. Things are
   simply **in the project** — user-facing verb is **"Add to project"**, the
   contents are **project items** (matching `app_project_items`). Every added
   item defaults to `role='library'` (grounds chat); `role='pin'` is for
   nav-only edges. Where older docs say "Library", read "the project's items".
1. **Universal extraction on upload.** Every text-bearing drive file is extracted,
   chunked, and embedded — the whole drive is corpus. Project membership is a
   *lens* (scope + up-weight), not the trigger for ingestion. Existing drive
   files were backfilled.
2. **Naming: "Open" vs "Scoped" chat** (user-facing); internal
   `ScopeMode::Weighted | Exclusive`. **Weighted stays an ADDITIVE z-space
   boost** — z-scores go negative, so multiplying inverts rankings. Default =
   Open; one visible per-chat toggle to Scoped (hard filter + grounded prompt
   line).
3. **Anchoring is quote-based, not offset-based.** Rust-extractor char offsets do
   NOT reliably index pdf.js's text layer (different reading order/whitespace).
   Therefore:
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
5. **No OCR.** Cut 2026-07-21 after a hardware spike (see D5): pdfium covers
   every born-digital PDF, and scanned PDFs extract to `no_text`, honestly.
6. **No Crossref in v1** (privacy: titles/DOIs of what you read — and unpublished
   drafts' titles — must not leak by default). Local metadata heuristics + a
   **manual metadata edit form**. Crossref returns later as explicit opt-in.
7. **Chunking**: paragraph-aware, ~400–600 tokens, 10–15% overlap, chunks may cross
   pages; anchor `page_num` (starting page) + char range + leading quote snippet.
   The generic indexer sub-windows each chunk row into several embeddings — this
   is **accepted and good** (multi-vector per chunk row, better recall, zero work).
   The embedder is whatever the embed slot runs; nothing here depends on its
   model or dimensions.
8. **No chat-stream protocol change.** Citations are built client-side from
   tool-output parts: `semantic_search` returns a `ref` per hit, and chunk hits
   carry `/drive/file_{id}?page=N&q=…`. A server citation event is optional
   future polish — and if ever added, check the native iOS app as a stream
   consumer first.

---

## D1 — Corpus · BUILT

Extract → chunk → embed → scope → cite: `virtues-core/src/extraction`, the
`document_extraction` applet, the `uploaded_document` ontology over
`extracted_document_chunks`, `/drive/file_` members resolved into project scope
(`search/query.rs`), `chatMode` open|scoped, per-file extraction state in the
project and Drive.

## D2 — Reading & annotation · BUILT, two items open

Built: `app_marginalia` (highlights + margin notes, global to the file), the
PdfPane annotation layer, find-in-document, precise citation landing
(`?q=` and `?hl=`), the `document_marginalia` ontology, and the per-file
annotation rail. Open:

1. **TextPane quote anchors** — the same highlight model on text files
   (simpler rendering; no rects).
2. **Project "Highlights" view** aggregating annotations across a project's
   items.

## D3 — Scholar layer (Zotero-grade, local-only) · ~3 days

1. **Migration `app_document_meta`**: `(file_id PK/FK CASCADE, title, authors JSONB,
   year, venue, doi, citekey UNIQUE, abstract, meta_source, updated_at)`.
2. **Local metadata extraction** in the cron: PDF XMP/Info dict + first-page
   heuristics (title/author/DOI regex). **No network calls.**
3. **Metadata edit form** (heuristics will be wrong; this is the correction lane —
   and the quality gate for citekeys/BibTeX).
4. **Citekeys** (`author2026word`) + collision suffixes.
5. **References view**: the project's documents as a bibliography grid (UniversalDataGrid):
   authors · year · title · venue · status.
6. **BibTeX export** (`GET /api/projects/:id/bibtex`) + copy-citekey.
7. **Dedup**: SHA-256 (exists) + DOI match surfaced ("already in Drive").

## D4 — Synthesis bridge

1. ~~Send highlight → Page~~ **BUILT**: blockquote + ref link, appended through
   the Yjs doc so an open editor is not clobbered.
2. **Bulk annotations export** — per file is built
   (`GET /api/annotations/export`); per project is open.
3. Polish: keyboard for colors, item counts, empty-states that teach the loop.

## Sequencing & risks

- D3 and the D2/D4 remainders each ship standalone value.
- **Quote-landing misses** (extractor text vs pdf.js text differ enough that the
  quote isn't found): fallback = page-only landing; tune snippet length.
- Citekey quality depends on the metadata heuristics; the edit form is the gate.

## Open questions

- Chunk hits in tool output should carry **doc title + page** so the model cites
  by name ("per Smith 2024, p. 6"), not by filename.
- **Trash semantics**: a project item whose file is in trash — show a
  "in trash" chip state, exclude from scope resolution.
- **Shared pages**: `shared_file_download` validates file membership in the
  shared page — confirm `?page/q` params flow through and no chunk/annotation
  data leaks via share tokens.
- Highlights spanning page boundaries: **disallow in v1** (anchor model is
  per-page).
- Add-to-project affordances: drag-drop + an "add from Drive" picker —
  picker ships when trivial, else fast-follow.

## D5 — OCR — ❌ CUT (decided 2026-07-21, after a full hardware spike)

**Decision: no OCR at all.** The spike proved the architecture works but
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
quantization loss, speed untested). The models considered were PP-OCRv5 mobile
det+rec (ONNX, Apache-2.0); the VLM document-parser tier (PaddleOCR-VL, Surya,
DeepSeek-OCR) does not fit the QCS6490 NPU.

## Project steps still open

The rest of the projects design that never shipped (the built part is
[projects.md](../record/projects.md)):

- **Room briefing.** `current_status` is only ever written by the owner. The
  design was an auto-maintained "state of the room" memo, fed by the salience
  engine (itself designed, not built), surfaced on Home.
- **Map.** The explicit-ref concept graph shipped as a "Mentions" filter on
  the member grid. The designed tab (hubs = key texts, orphans = materials
  nothing references yet) did not.
- **Smart membership.** "Everything tagged X" or "everything referencing
  [@Person]" as a rule rather than a hand-filed item. Tags exist and are the
  intended input.
- **Watch.** Change detection and re-fetch for external materials. Nothing
  fetches a project's external material today; the box's URL fetcher
  (`virtues-core/src/fetch`) serves bookmarks.

## Explicit non-goals (v1)

Whiteboard/spatial canvas · OCR and the VLM document-parser tier (see D5) ·
Crossref/network metadata enrichment (opt-in later) · URL/YouTube snapshot
ingestion (separate lane) · Zotero/BibTeX importer (fast-follow; schema is
receiver-ready) · literature discovery (Elicit's corpus) · audio overviews /
studio artifacts · auto-NER concept maps (prose ER paused) · draft/version
succession (v2-of-a-manuscript linking — named here so it's a decision, not an
oversight) · chunks in ⌘K global search · server citation stream event ·
**chat-attachment → Drive unification** (own project: touches message parts
schema + clients).
