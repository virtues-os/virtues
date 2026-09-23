# The wiki as the retrieval index, not a document in it

**Status: planned.** Nothing here is built. Delete this file when it is.

## The finding

Measured on a real box, 2026-09-22:

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

The wiki is **0.16%** of the retrieval corpus, and it competes as a peer.
Hybrid + RRF + rerank will surface the twelve thousand messages *about* a
person long before the one article that resolved them into her. The
compression loses to the thing it compressed, about 500 to 1.

`wiki_refs` holds 136,219 rows — the deterministic join between the time axis
and everything else — and `search/query.rs` uses it only to NARROW results when
the caller already knows the entity ids, plus an additive project boost.
Nothing resolves a query to a subject and then walks the refs outward.

**So the wiki is indexed as content and ignored as structure.** That is the
whole gap, and it is why the encyclopedia feels absent from chat while being
present in the database.

## The subjects are not in the index at all

`search_embeddings` carries no `person`, `place` or `organization` ontology.
On the same box that is **914 resolved subjects** — 626 people, 228
organizations, 60 places — none of them retrievable, and only 20 of them
carrying an article.

This is the piece that does not wait on article coverage. A person with no
article is still a fully resolved thing: a name, aliases, a nickname, handles,
a relationship category, a ref count, and a set of co-occurring subjects. That
is a retrievable document today, for every one of the 914, with **no model
call**. "Who was the woman from the dog park" is a subject lookup, not a
message search.

## Three moves, in order

### 1. Subject stubs into the index

Declarative. `search/indexer.rs` is registry-driven — it loops
`registered_ontologies()` filtered to those with an `embedding` config — and
`EmbeddingConfig` is pure SQL templates (`embed_text_sql`, `title_sql`,
`preview_sql`, `timestamp_sql`, `embed_where`, `content_type`). `wiki_article`
and `wiki_event` are already registered this way, so a subject is the same
mechanism rather than new machinery.

Three `OntologyDescriptor`s — person, place, organization — each with
`embedding: Some(…)`, `extraction: None`, `day_source: None` (a subject is not
something you did; the `wiki_article` entry above it explains why that field
matters).

`embed_text_sql` should carry what makes a subject FINDABLE rather than what
makes it readable: name, nickname, aliases, handles, relationship category,
the authored `content`, and — via correlated subquery — the ref count and the
names it most often co-occurs with. The last is what lets "the woman from the
dog park" land on a person whose article does not exist and whose name you
cannot remember.

**Open: what `timestamp_sql` should be.** `t.updated_at` is honest but useless
— a subject is not on the clock (the glossary is explicit: "a person is not
past or future"). First-seen/last-seen from `wiki_refs` would make
time-filtered queries work on subjects, and costs a subquery per row. Decide
before writing the descriptor, because reindexing 914 rows to change it is
cheap now and less cheap later.

### 2. Weight the wiki lanes

`query.rs` already does an additive ≈1σ z-boost for project members — "a
boost, not a multiply", because the scores can be negative. The same lever,
pointed at `wiki_article`, `wiki_event` and the new subject ontologies.

**Measure before tuning.** Take ten real questions, run them through the
current path, and count how often a wiki chunk appears at all. If the answer
is "never", a 1σ bump against a 500:1 imbalance is the wrong instrument and
move 3 is the only real fix. This measurement is the gate on the other two.

### 3. Two-stage routing

Resolve the query against the small, high-precision subject/article pool
(hundreds of chunks), then expand through `wiki_refs` to the records
underneath. The article says *who* and *when*; the refs reach the specific
record. This is what the paradigm actually asks for and what neither 1 nor 2
delivers on its own.

## The consequence nobody has priced

If the wiki is the index, the article acquires a second job: **it must be
written to be found, not only to be read.** That reframes the prose work.
Naming a street, an hour, a phrase someone actually used beats summarizing —
which is the opposite of what the entity prompt was doing when every one of the
first twenty articles described the record instead of the person.

It also reframes coverage. "0 place articles, 0 organization articles, 20 of
914 people" stops being a content gap and becomes **holes in the retrieval
index shaped like the subjects nobody wrote.**

## The open question this plan does not answer

**Is the wiki the index, or a document among documents?**

If it is the index, all three moves are right and the article's job changes.
If it is a reading surface, the 500:1 is fine, chat should keep searching raw
records, and only move 1 is worth doing — because a subject stub is useful for
lookup either way.
