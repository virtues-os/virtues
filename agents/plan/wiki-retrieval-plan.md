# Wiki retrieval: find the subject, then its records

**Status: planned.** Nothing here is built. Delete this file when it is.

**Settled (owner, 2026-09-23): the wiki is the index.** Chat retrieves through
it. This plan is about how, not whether.

## Two axes

**Time is the deterministic one.** A question with a date in it — "what did I
do in March", "the week of the move" — should land on day articles and
`wiki_event` rows by date, not by similarity.

**Context is everything else, and the subject is where it resolves.** A person,
a place, an organization: many records, one thing you can point at.

## What already exists

The two-stage path is built. It is agent-mediated rather than automatic:

- `semantic_search` accepts `entities` (filters through `wiki_refs`) and
  `date_after` / `date_before`.
- The agent can find a subject's id with `sql_query` and pass it in.

**What is missing is narrower than the path.** `sql_query` finds a subject only
by a name it can match. Nothing finds one by nickname, alias, handle, a
misspelling, or "my old landlord". And nobody has measured whether the agent
uses the filters at all.

## The corpus, measured on a real box (2026-09-22)

```
communication_message       176,779
communication_transcription  29,643
financial_transaction         9,545
calendar_event                7,400
communication_email           4,267
app_chat                      4,024
wiki_event                    1,505
uploaded_document               511
app_page                        422
wiki_article                    372
```

Chunks, not documents. The wiki (`wiki_article` + `wiki_event`) is about 0.8%
of the pool. Of 914 resolved subjects, 20 are reachable by meaning, through
their articles; 894 are reachable only by exact name.

## Step 0 — measure (gates every step that costs anything)

Ten real questions from chat history on the box. For each, record three things:

1. Did a wiki chunk appear in what `semantic_search` returned?
2. Did the agent pass `entities` or a date filter?
3. Was the answer right?

The pattern decides what is wrong. Wiki chunks relevant but ranked out → the
ranking (step 3). Filters never used → the agent's instructions (step 2). The
agent could not find the subject → the lookup (step 1).

## Step 1 — subject stubs, for fuzzy lookup

Three `OntologyDescriptor`s in the registry — person, place, organization —
with `embedding: Some(…)`, `extraction: None`, `day_source: None`. The indexer
is registry-driven and `EmbeddingConfig` is SQL templates, so this is the
mechanism `wiki_article` already uses. Embedding only, on the box; no LLM call.

`embed_text_sql` carries what a person would reach for: name, nickname,
aliases, handles, relationship category, and any content the owner authored on
the subject.

**Not co-occurring subjects.** The first draft of this plan proposed them;
measured, subjects share a record 82 times person-with-person, 5 times
place-with-place, and never person-with-place, across 136,219 refs. There is
nothing to carry. A question like "the woman from the dog park" resolves
through the day or the record that mentions both, not through a stub.

`timestamp_sql` is the subject's last-seen, from `wiki_refs`. A subject is a
span rather than a moment, and the time axis belongs to days; last-seen is only
there so recency ranking has something honest to read.

## Step 2 — the agent's instructions, if step 0 says so

If the agent does not pass `entities` or dates when a question calls for them,
say so in the tool description: find the subject, then filter by it; a dated
question takes a date filter. Cheaper than any ranking change.

## Step 3 — weight the wiki, if step 0 says so

`query.rs` already has an additive ≈1σ z-boost for project members. The same
lever, pointed at `wiki_article`, `wiki_event` and the stubs. Only if relevant
wiki chunks are measurably losing on rank.

## Boundary

Stubs are a read-only lookup. Nothing here writes `wiki_refs` or decides two
records are the same thing; the graph stays deterministic and owner-authored.

## Found along the way

`entity_article_gen::build_dossier` builds its link allowlist from the same
same-record join, so it offers the model almost no subjects to link — which is
why the first articles invented links. The sanitizer now strips those, but the
allowlist it checks against is nearly empty. The subjects that appear on the
same DAYS are the join that has rows: 74,470 person-with-person pairs, 3,135
organization-with-person, 1,603 person-with-place, against 87 pairs total on
the same record. Too noisy to embed into a stub, which would make every search
for one person land on everyone who texted that day; right for an allowlist,
because the model only links what it chose to mention.

The article gains a second job as well: it is read, and it is found. Concrete
detail — a street, an hour, a phrase — serves both.
