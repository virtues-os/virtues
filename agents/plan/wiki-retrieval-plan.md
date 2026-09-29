# Wiki retrieval

**Status: planned.** Delete when built. **Settled (owner, 2026-09-23): the wiki
is the index.** Chat retrieves through it.

## The problem

Chat can reach a day by its date, but it can find a person, place or
organization only by a name it matches exactly. "Dave", a misspelling, or "my
old landlord" finds nothing. Nobody has measured whether the agent uses the wiki
at all.

## Two doors

| The question | The door | Built? |
|---|---|---|
| Names a thing: "March 3", "2025", "David" | **Lookup**: `sql_query` on `wiki_day_prose`, subject tables, `wiki_articles` → `app_pages` | Yes. Days work; subjects only by exact name |
| Describes a thing: "when I felt stuck at work" | **Search**: `semantic_search`, with `entities` and date filters | Yes, but the wiki is 0.8% of the pool and 894 of 914 subjects have no embedding |

No new lookup tool. It would duplicate `sql_query` for days, and matching names
as text misses the same cases wherever it runs.

## Step 0: measure (gates the rest)

Ten real questions from chat history on the box, chosen by the owner. For each,
record the tokens, the tool calls, whether a wiki chunk came back, whether the
agent used `entities`, a date filter, or a wiki table, and whether the answer
was right. Run them again after each step.

## Step 1: subject stubs

Three registry `OntologyDescriptor`s (person, place, organization), embedding
only, with no LLM call. The text: name, nickname, aliases, handles, relationship
category, and anything the owner has written on the subject. The timestamp is
when the subject was last seen in `wiki_refs`.

Leave out co-occurring subjects. Subjects share a record 87 times in 136k refs;
sharing a day is common but it is noise, and it would make a search for one
person land on everyone who texted that day.

## Step 2: search the wiki inside every search (option B)

`semantic_search` runs a second, small pass over articles, day articles, events
and stubs, reusing the same query embeddings and date filters.

- Up to 3 hits, shown first under **From your wiki**, then **From your
  records**.
- A subject hit carries its id, so the next call can filter by it with
  `entities`.
- An article replaces its subject's stub, and no hit appears twice.
- Skipped when the agent asks for a specific domain ("search email").
- One line in the tool description replaces the parked "wiki-first" wording.

This keeps the wiki's score apart from everything else's. Adding one weight
across different score scales is fragile, which is the reranker problem again.

## Open

- **The relevance cutoff.** A small pool always returns its top k, however weak.
  Set the cutoff from the reranker scores on the step 0 questions.
- The ten questions.

## Boundary

Read-only. Nothing here writes `wiki_refs` or decides that two records are the
same thing.

## Also found

`entity_article_gen::build_dossier` builds its link allowlist from subjects that
share a record (87 pairs), so the model is offered almost nothing to link.
Subjects that share a day (about 80k pairs) are the right source for that list:
too noisy to embed, but fine for an allowlist, because the model only links what
it chose to mention.
