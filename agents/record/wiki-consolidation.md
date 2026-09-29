# Wiki consolidation

**Written 2026-09-29**, of work built 2026-07-31 through 2026-09. How six
separate stores of prose about a subject became one — an article is a page —
and what that decision cost and taught. Supersedes `agents/plan/wiki-plan.md`
(deleted). What remains open from that plan (the note writer, merge) is in
[wiki-notes-plan.md](../plan/wiki-notes-plan.md); the standing rules it
produced are in [wiki-editor.md](../build/wiki-editor.md).

---

## What was wrong

Six independent implementations of one idea, "prose about a subject":

| Where | Machine-written | Person-written |
|---|---|---|
| `wiki_people` | `article`, `article_updated_at`, `article_ref_count` | `content`, `notes` |
| `wiki_places`, `wiki_orgs` | same three | `content` |
| `wiki_days` | `autobiography`, `autobiography_sections` | — |
| `wiki_stories` | — | `content` |
| `wiki_narrative_identity` | drafted by an applet | `content` |

None had revision history, an off switch for machine rewriting, or a shared
editor. `app_pages` had all three: Yjs editing, `app_page_versions`, and an AI
write path (`tools/page_editor.rs`) that goes *through* the CRDT and snapshots
before it edits. Beside them sat a notes table with zero producers and
`dirty_at` columns stamped by four call sites and read by none.

## An article is a page

The wiki owns a join row, `wiki_articles (subject_type, subject_id, page_id)`;
the prose lives in `app_pages` with `kind = 'article'`. A prose column would
have been a second AI write path with no CRDT and no pre-edit snapshot.

`kind` sits on the page because a predicate someone forgets to write is a
leak and a column with a default is not; the wiki's bookkeeping sits on the
join row. The two encode one fact twice, and that is contained by there being
exactly one creation path, `api::wiki_articles::create_article`, which writes
both in one transaction. It is **pool-only** because applets link
virtues-core as a library and hold nothing richer than a `PgPool`; a creation
path in the server layer would have broken the invariant for every applet.

A first write stores markdown with no `yjs_state`, and the Yjs layer seeds
`Y.Text` from it on first open. An *edit* is different: once `yjs_state`
exists it is authoritative, and a pool-only write to `content` is silently
discarded on the next save. That is why the editor runs where a `YjsState`
is held, never in an applet subprocess.

`create_article` leaves `app_pages.date` NULL even for a day's article:
the page ontology's day source filters on it, so the day would show its own
article as "you wrote a page today".

## The `embed_where` trap

Two ontologies over one table do not split on their own — they double-index,
because an embedding is keyed `{ontology}:{record}:{chunk}`. `EmbeddingConfig`
had no filter field, so the split needed a registry change: `embed_where`,
spliced into the indexer's backlog query (`search/indexer.rs`).

The staleness test that query already carried is a disjunction — never indexed,
*or* indexed from different text. Appended unparenthesised, a scope parses as
`se.id IS NULL OR (stale AND kind = 'page')`, and every never-indexed row of
the *other* ontology satisfies the left branch and is embedded under the wrong
name while the run reports success. The indexer parenthesises the staleness
test first; two registry tests hold the rest —
`embed_where_carries_its_own_and` and
`tables_shared_by_two_ontologies_are_scoped`.

The split also broke `attach_record_refs` in `tools/sql_query.rs`, which
bailed when more than one ontology matched a table, so every SQL-tool row
touching `app_pages` silently lost its citation. It now counts tables, not
ontologies.

## A version row is a snapshot taken before an edit

`page_editor.rs` writes a version *before* it edits, stamped with the editor
about to write, and nothing after. So `created_by` names the author of the
*next* state, and the current text is in no version row. Reading the table
naively gets authorship backwards.

`get_article_history` recovers it at read time by pairing `version[n]`
against `version[n+1]`, or the live page for the newest. Changing how
versions are written would have invalidated every row on disk. Diff text comes
from `yjs_snapshot`; `content_preview` holds a label ("Auto-saved before AI
edit"), never prose. The History feed left the wiki rail but `/wiki/history`
still exists and is linked from the wiki.

## A drop is a one-way door

Once a column is gone the previous binary cannot boot against the database,
and `virtues upgrade` restarts the prior slot on its own when a start fails
after migrations succeed (`flip_back` in `cli/upgrade.rs`). Boot migrations
run with `set_ignore_missing(true)`, so an older binary boots cleanly against
a newer schema and fails per query — the worst shape a failure can take. So:

- a column is dropped only once the release `rollback` would land on has
  stopped reading it — expand, migrate, contract, with the contract a release
  later;
- readers come out in the commit *before* the drop;
- the drops were batched into two terminal migrations, 0024 (what article
  resolution replaced) and 0025 (columns with a reader and no writer).

A drop also passes CI and 500s on a real box if `.sqlx/` is stale, since
every build runs `SQLX_OFFLINE=true`; `ci.yml` now runs
`cargo sqlx prepare --check --workspace`. The first reason given for
expand/contract — a concurrent-reader window — was wrong: `upgrade` stops the
old binary before it migrates. Rollback is the real reason.

## Smaller decisions that held

- **Links target subjects, never articles.** A mention of someone with no
  prose still surfaces on their subject view. `get_subject_backlinks` derives
  them at read time from `LIKE '%/{route}/{id})%'`; no edge table, because
  nothing measured a need for one.
- **Notes and marginalia are two things.** `app_marginalia` anchors to a
  passage in a document you are reading; `wiki_notes` is about a subject and
  never anchors to a character range. A unified table was tried in design and
  dropped. The `/api/annotations` routes kept their path because the installed
  iOS build does not update with the box.
- **Ranking, not deletion.** People, places and orgs sort by a ref count
  derived per query from `wiki_refs` (`api/wiki.rs`); automated senders sink
  rather than being hidden by a bar someone has to justify.
- **The self row is a pointer on the profile** (`app_user_profile.self_person_id`,
  no FK, like `home_place_id`), so one column on a one-row table makes a
  second self structurally impossible.
- **The masthead is computed, not written**, so there is no Overview article.
- **Lifeline lanes derive from the registry `domain`**, bucketed in Postgres.

## What overruled the plan

- **Articles were opt-in** — nothing written until asked. Overruled by
  [article-resolution.md](article-resolution.md): every subject gets an
  article. 0022 added `maintenance` defaulting to `'auto'`; 0034 cut it to
  `auto`/`never`, since `'always'` behaved identically.
- **The one-pen rule** — the first human edit claimed an article and froze it.
  Overruled by the same record: ownership never flips, and the server refuses
  a machine edit that loses the person's sentences (`api/wiki_editor.rs`).
  `auto_update` and `wiki_days.last_edited_by` were dropped.
- **Narrative identity** as its own table. `wiki_narrative_identity` was
  dropped in 0024; the life document is an article whose subject has no brief,
  so the editor structurally cannot touch it. Proposals arrive as notes through
  `propose_narrative_identity_edit`.
- **The day article** shipped, moving `autobiography` into article pages. Its
  narration is being redone from the day's own evidence in
  `agents/plan/day-article-plan.md`.
- **`wiki_stories.content` was to be kept.** It was dropped in 0024 with the
  rest of the table's writerless columns; a story's prose is its article's.

## Refuted along the way

- That the AI page-write path carried a permission gate. It never did;
  `edit_page` runs freely because a page edit is reversible.
- That the day narration could write notes. Its dossier held message
  *counts*, and a note is made entirely of what was said.
- A dedupe key for notes. A note is prose with citations and has no structured
  identity; two phrasings of one observation would get two keys.
- A post-hoc matcher linking bare names: it would rewrite human text.
- Month and week rooms, and level-of-detail caching for the lifeline — the
  first names nothing a person names, the second solved a scale no box had.
