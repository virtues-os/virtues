# Researcher: corpus, annotation, and the OCR spike

**Written 2026-10-20.** The researcher work was planned 2026-07-20 and revised
the same day after a review against the code. This record covers the parts
that were built: the corpus (D1), reading and annotation (D2), and sending a
highlight to a page (D4.1), plus the OCR spike of 2026-07-21 and why OCR was
cut. What is still open is in
[researcher-plan.md](../plan/researcher-plan.md). Projects themselves are in
[projects.md](projects.md).

The target user is a researcher, PhD student or academic: a corpus, reading
and annotation, scholarly metadata, and a way to move from reading to writing.
The reference products were NotebookLM (answers cite their sources), Heptabase
(highlight to note) and Zotero (references), all working over the owner's own
data on the box.

## Why the box

- Retrieval is local. The corpus, index and annotations stay on the box. Chat
  excerpts go to the model the owner chose at the time of asking, so the claim
  is "retrieval is local", never "nothing leaves the box".
- A project can contain the PDF, the advisor's email thread, the person, and a
  day, all in one retrieval scope.
- Citations are refs: a cited answer opens the exact page, and after D2 the
  exact passage.
- Writing happens in Pages inside the same graph, so there is no export step
  between reading and writing; BibTeX and annotations export out.

## Decisions (2026-07-20)

0. **No "Library" noun.** Items are simply in the project. The user-facing
   verb is "Add to project", the contents are project items
   (`app_project_items`). An added item defaults to `role='library'` (grounds
   chat); `role='pin'` is for navigation-only edges. Where older docs say
   "Library", read "the project's items".
1. **Every upload is extracted.** Every text-bearing drive file is extracted,
   chunked and embedded. Project membership scopes and up-weights retrieval;
   it does not trigger ingestion. Existing drive files were backfilled.
2. **"Open" and "Scoped" chat** (internal `ScopeMode::Weighted | Exclusive`).
   Weighted is an additive boost in z-space, because z-scores go negative and
   multiplying would invert rankings. Open is the default; one per-chat toggle
   switches to Scoped (a hard filter plus a grounded prompt line).
3. **Anchors use quotes, not offsets.** The Rust extractor's character offsets
   do not reliably index pdf.js's text layer (reading order and whitespace
   differ). So:
   - Chunk citations are self-contained: `?page=N&q=<short quote snippet>`,
     landed by searching the pdf.js text layer. There is no resolve endpoint,
     and links survive re-extraction (chunk ids are ephemeral by design).
   - User highlights anchor by quote plus prefix and suffix (W3C
     TextQuoteSelector style), page, and normalized rects captured in the
     viewer; deep link `?page=N&hl=<annotation_id>` (annotations are durable
     rows).
   - Chunks still store `char_start`/`char_end` into the extractor's text for
     bookkeeping, never for landing in the viewer.
4. **No whiteboard.** Heptabase's spatial canvas is out; its highlight →
   excerpt → note loop is in, targeting Pages.
5. **No OCR** (see below).
6. **No Crossref in v1.** Titles and DOIs of what someone reads, including
   titles of unpublished drafts, must not leave the box by default. Crossref
   may come back later as an explicit opt-in.
7. **Chunking:** paragraph-aware, about 400–600 tokens, 10–15% overlap, chunks
   may cross pages; anchored by `page_num` (starting page), a char range and a
   leading quote snippet. The generic indexer splits each chunk row into
   several embeddings, which is intended (better recall, no extra work). Nothing
   depends on the embed slot's model or dimensions.
8. **No chat-stream protocol change.** Citations are built client-side from
   tool-output parts: `semantic_search` returns a `ref` per hit, and chunk hits
   carry `/drive/file_{id}?page=N&q=…`. If a server citation event is ever
   added, check the native iOS app as a stream consumer first.

## What was built

- **D1, corpus:** extract, chunk, embed, scope, cite.
  `virtues-core/src/extraction`, the `document_extraction` applet, the
  `uploaded_document` ontology over `extracted_document_chunks`, `/drive/file_`
  members resolved into project scope (`search/query.rs`), `chatMode`
  open|scoped, and per-file extraction state in the project and Drive.
- **D2, reading and annotation:** `app_marginalia` (highlights and margin
  notes, global to the file), the PdfPane annotation layer, find in document,
  citation landing (`?q=` and `?hl=`), the `document_marginalia` ontology, the
  per-file annotation rail, and per-file annotation export
  (`GET /api/annotations/export`).
- **D4.1, send highlight to a page:** a blockquote plus a ref link, appended
  through the Yjs document so an open editor is not overwritten.

## OCR: spiked and cut, 2026-07-21

Decision: no OCR. The architecture worked, but quantized recognition could not
reach the accuracy a research corpus needs on the Q6A. More importantly, the
feature would help few files: pdfium already covers every born-digital PDF, so
OCR only adds scanned PDFs and image uploads. For a few scans, CPU-only OCR
(7.4 s/page in a background cron) would have been enough without NPU work.

Measured on the Dragon Q6A (QCS6490, HTP v68) through Qualcomm AI Hub:

| Finding | Result |
|---|---|
| det on NPU | 23.9 ms/page, 225/225 layers NPU |
| rec backbone on NPU (split-head) | 1.92 ms/line, 164/164 layers NPU |
| rec neck + CTC (CPU) | 4.61 ms/line |
| split correctness (float) | 25/25 lines byte-identical to monolithic |
| w8a8 quantized accuracy | 85.0% char accuracy |
| w8a16 quantized accuracy | 93.4% (weak calibration), 94.11% (rich calibration) |
| w16a16 / int16 | does not compile on v68 (graph-compose error 14) |
| fp16 on NPU | unsupported (v68 is an integer engine) |

Richer calibration added only 0.7 points, so the loss comes from int8 weight
quantization itself and not from tuning. Quantized recognition plateaus near
94% (about one error per 17 characters), which corrupts citations and
embeddings.

If OCR is ever revived, these keep accuracy: detection on the NPU with
recognition in float on the CPU (about 2.1 s/page, about 100% accuracy, 3.5×
faster than all-CPU); an earlier split point as an accuracy/speed setting; or
recognition in fp16 on the Adreno 643 GPU (no quantization loss, speed
untested). The models considered were PP-OCRv5 mobile det+rec (ONNX,
Apache-2.0). The VLM document parsers (PaddleOCR-VL, Surya, DeepSeek-OCR) do
not fit the QCS6490 NPU.
