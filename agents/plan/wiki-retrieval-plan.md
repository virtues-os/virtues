# Wiki retrieval

**Status: planned.** Delete when built. **Settled (owner, 2026-09-23): the wiki
is the index.** Chat retrieves through it.

## The problem

Search never returns the wiki. Chat can open a day by its date through
`sql_query`, but it can find a person, place or organization only by a name it
matches exactly.

## Step 0: measured on the box (2026-09-29)

Every chat turn since 2026-09-01 that used retrieval: 108 turns, 200
`semantic_search` calls.

- **Search returned a wiki chunk once**: one event, and never an article.
- **The agent already uses the wiki directly.** Since 09-22 it read wiki tables
  with `sql_query` in 14 of 27 turns, passed `entities` in 15 and a date filter
  in 13. Its instructions are not the problem.
- **Both filters remove the whole wiki.** Every wiki chunk has a null
  `occurred_at`, so a date filter drops all of them. No article is in
  `wiki_refs`, so `entities` drops all of them. The agent is penalised exactly
  when it filters correctly: 64 of the 200 calls filtered.
- **Unfiltered, articles rank out.** Across twelve real queries, the best
  article's dense rank was in the tens of thousands out of 239k chunks, even
  when the subject named in the query has an article. Short messages outscore
  long articles on gte-small.
- **A wiki-only pool isn't enough on its own.** Dense similarity alone puts
  generic events such as "Short trip" above the named person's article.
- **Cost:** a question that names a subject takes a median of 14 tool calls and
  68 KB of results.

Not measured: whether the answers were right (the owner has to judge), and how
BM25 and the reranker rank a wiki-only pool (this needs the engine; run it on
the spare box).

## Step 1: stop filters dropping the wiki (built 3fc8725b, unverified on a box)

Worse than measured: the indexer stored no date on ANY chunk, not just the
wiki's, so every date-filtered search returned nothing (25 of 25). Fixed at the
source, with a backfill for chunks already indexed. A day's article is dated
noon on its day; other articles are undated. `entities` also matches the
subject's own article. Verify on dragon after release: `occurred_at` is filled,
and a date or person filter returns wiki rows.

## Step 2: the wiki leads every search

**Built (84a2d1f0, unverified on a box):** the deterministic rows. When the
queries name a subject (by full name, nickname or alias; a first name only when
it is unique and nothing matched in full) or pass it as `entities`, its article
leads under `from_your_wiki`, or a card with its id when it has none. A date
range of a week or less brings those days' articles. 4,000-character budget;
no repeats below; skipped when domains are named or the project scope is
exclusive. On one box's history, 35 of 160 searches would have had a lead.

**Not built: the similarity row** (up to 3 wiki hits for searches that name
nothing). Run hybrid search and the reranker over a wiki-only pool on real
queries first; dense similarity alone ranked the wrong things.

## Step 3: subject stubs

The 894 subjects without an article get an embedding-only stub, with no LLM
call: name, nickname, aliases, handles, relationship, and anything the owner has
written on the subject. Leave out co-occurring subjects: they share a record 87
times in 136k refs, and sharing a day is noise.

## Open

- The relevance floor, set from reranker scores on real queries.
- Re-run step 0 after each step.

## Also found

`entity_article_gen::build_dossier` offers links only to subjects that share a
record (87 pairs). Subjects that share a day (about 80k pairs) are the right
source for that allowlist.
