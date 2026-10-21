# Researcher: what is left

The researcher work over projects was planned 2026-07-20. The corpus (D1),
reading and annotation (D2), sending a highlight to a page (D4.1), the
decisions they rest on, and the OCR spike are recorded in
[record/researcher.md](../record/researcher.md). Projects themselves are in
[record/projects.md](../record/projects.md). This plan lists the rest. Delete
it when the list is empty.

## D2: two annotation items

1. **Quote anchors in TextPane:** the same highlight model on text files
   (simpler rendering, no rects).
2. **A project "Highlights" view** that collects annotations across a
   project's items.

## D3: scholar layer (Zotero-level, local only), about 3 days

1. **Migration `app_document_meta`:** `(file_id PK/FK CASCADE, title, authors JSONB,
   year, venue, doi, citekey UNIQUE, abstract, meta_source, updated_at)`.
2. **Local metadata extraction** in the cron: PDF XMP/Info dict plus
   first-page heuristics (title, author and DOI regex). No network calls.
3. **A metadata edit form.** The heuristics will be wrong, so this is where
   the owner corrects them, and it is what keeps citekeys and BibTeX correct.
4. **Citekeys** (`author2026word`) with collision suffixes.
5. **References view:** the project's documents as a bibliography grid
   (UniversalDataGrid): authors, year, title, venue, status.
6. **BibTeX export** (`GET /api/projects/:id/bibtex`) and copy-citekey.
7. **Dedup:** SHA-256 (exists) plus a DOI match, shown as "already in Drive".

## D4: the rest of the reading-to-writing path

1. **Annotation export per project.** Per file is built
   (`GET /api/annotations/export`).
2. Polish: keyboard shortcuts for colors, item counts, empty states that show
   how highlight-to-page works.

## Sequencing and risks

- D3 and the D2/D4 remainders are each useful on their own.
- **Quote landing can miss** when the extractor's text and pdf.js's text
  differ too much. Fallback: land on the page only; tune the snippet length.
- Citekey quality depends on the metadata heuristics; the edit form is the
  check.

## Open questions

- Chunk hits in tool output should carry the document title and page, so the
  model cites by name ("per Smith 2024, p. 6") instead of by filename.
- **Trash:** a project item whose file is in the trash should show an "in
  trash" chip and be excluded from scope resolution.
- **Shared pages:** `shared_file_download` checks that the file belongs to
  the shared page. Confirm that `?page`/`q` params pass through and that no
  chunk or annotation data leaks through share tokens.
- Highlights that span a page boundary: disallow in v1 (anchors are per page).
- Add-to-project: drag and drop, plus an "add from Drive" picker, which ships
  if it is trivial and otherwise right after.

## Project steps that never shipped

- **Room briefing.** Only the owner ever writes `current_status`. The design
  was an automatically maintained summary of the project's state, fed by the
  novelty engine (designed, not built), shown on Home.
- **Map.** The explicit-ref concept graph shipped as a "Mentions" filter on
  the member grid. The designed tab (hubs = key texts, orphans = materials
  nothing references yet) did not.
- **Smart membership.** "Everything tagged X" or "everything referencing
  [@Person]" as a rule instead of hand-filed items. Tags exist and are the
  intended input.
- **Watch.** Change detection and re-fetch for external materials. Nothing
  fetches a project's external material today; the box's URL fetcher
  (`virtues-core/src/fetch`) serves bookmarks.

## Out of scope for v1

A whiteboard or spatial canvas; OCR and the VLM document parsers (see the
record); Crossref or other network metadata (possible later as opt-in);
URL/YouTube snapshot ingestion (separate work); a Zotero/BibTeX importer
(soon after; the schema can receive it); literature discovery (Elicit's
corpus); audio overviews and studio artifacts; automatic NER concept maps
(prose entity resolution is paused); linking successive drafts of a
manuscript; chunks in ⌘K global search; a server citation stream event; and
unifying chat attachments with Drive (its own project: it touches the message
parts schema and every client).
