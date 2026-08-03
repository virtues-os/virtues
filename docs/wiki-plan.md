# The Wiki — the personal wikipedia

Status: DESIGNED 2026-07-30, **audited 2026-07-31** (conversation-complete,
unbuilt). Supersedes the article-column approach shipped in 0072 and the
marginalia section of [archive/stories-plan.md](./archive/stories-plan.md) §3.6.

Every claim below was checked against the codebase and against real box data.
The audit pass verified each file:line citation and corrected six claims that
were wrong — most importantly that the AI page-write path carries a permission
gate (it does not), that the ontology split is a config edit (it needs a
registry field), and that links point article→article (they point at subjects).
Corrections are kept inline rather than deleted, so nobody re-derives them.

## The paradigm

**Every subject in your life has one page, and the page is prose written from
the record, with citations.**

Four rules give that sentence teeth:

1. **The record is readable without any AI.** Entities, days, the lifeline and
   search all work on a box that has never made a model call. Prose is something
   you add, not something you wait for.
2. **Nothing is written that you did not ask for.** No article exists until you
   ask for one; none is maintained until you turn maintenance on.
3. **The machine proposes; it never decides.** Its only channel into the record
   is a cited note, which you accept or dismiss. It may not write the graph.
4. **You can edit all of it**, and every edit — yours or its — is a revision you
   can read and revert.

Scope for this arc: **Overview, Lifeline, Narrative Identity, People, Places,
Orgs, Days, History.** Stories are hidden from the room, not dropped
(`wiki_stories` shipped in 0075) — a row removed from `WIKI_MODE`, nothing
more. **Years stays in the room** (decided 2026-08-03): a year is a subject a
person names, and `YearPage` already renders it. Months and weeks do not get
rooms or pages — a week has no narrative identity, and "March" already has a
home as a note's date range; if a month view is ever wanted it is a grouping
of the Days index, not a subject. **Notes are not a room**: a note belongs to the
subject it is about, so it lives as a rail on that article and as one module on
the Overview. A separate destination would detach every note from its subject,
which is the whole point.

## Why this exists

Six independent implementations of one idea — "prose about a subject":

| Where | Machine-written | User-written |
|---|---|---|
| `wiki_people` | `article`, `article_updated_at`, `article_ref_count` (0072) | `content`, `notes` |
| `wiki_places` | same three (0072) | `content` |
| `wiki_orgs` | same three (0072) | `content` |
| `wiki_days` | `autobiography`, `autobiography_sections`, `last_edited_by` | — |
| `wiki_stories` | — | `content` (0075) |
| `wiki_narrative_identity` | drafted by `narrative_identity_draft` | `content` |

None has revision history, an on/off switch for AI rewriting, or a shared
editor. `app_pages` has all three — Yjs editing, `app_page_versions` with a
`created_by` column, and an AI-write path in production.

(An earlier draft claimed that path carries a permission gate. It does not:
`edit_page` runs freely because a page edit is reversible
([page_editor.rs:240](../virtues-core/src/tools/page_editor.rs:240)); gating
applies only to `run_applet`/`delete_applet`. The `permission_needed` line in
`prompt.rs:59` is a stale prompt string with no code path behind it. §2's
consent model therefore rests on the sweep's own gate, not on an inherited one.)

And two things exist wired to nothing:

- **`wiki_marginalia`** (0033) — `subject_type`/`subject_id`, `kind`, `body`,
  `author IN ('ai','human')`. **Zero producers.**
- **`dirty_at`** on `wiki_events`, `wiki_days`, `wiki_stories` (0033) — "new
  evidence landed." **Never read as a queue.**

### What the box actually holds (measured 2026-07-30, `virtues_boxcopy`)

| | |
|---|---|
| people / orgs / places | 573 / 159 / 31 |
| **articles that exist** | **0** |
| entities that would pass the old `refs >= 15` bar | **226** |
| days | 42 (2026-02-25 → 07-29), 13 with prose |
| wiki notes | 0 |
| pages | 18 |
| all lifeline lanes, all rows | ~330k (169k messages, 100k location points) |

Several decisions below come straight off these numbers rather than from taste:
there is **nothing to backfill**; the old thresholds would have generated **226
unrequested articles** on a dev box holding five months; both note tables are
effectively empty, so renaming them is free; and the corpus is small enough that
**level-of-detail machinery is premature**.

## Decisions

### 1. An article is a page. The wiki owns a join row, not the prose.

The deciding fact: **the AI already writes pages through the CRDT.**
`page_editor.rs` reads via `get_page_content()`
([:189](../virtues-core/src/tools/page_editor.rs:189)), snapshots a version
([:255](../virtues-core/src/tools/page_editor.rs:255)), then edits via
`apply_text_edit()` ([:265](../virtues-core/src/tools/page_editor.rs:265)). A
separate `wiki_articles.content` column would mean a second AI-write path with
no CRDT reconciliation and no pre-edit snapshot — strictly worse than the one
that works.

```sql
-- Claim the number with `make migration NAME=wiki_articles` first.

ALTER TABLE app_pages ADD COLUMN kind TEXT NOT NULL DEFAULT 'page';  -- 'page' | 'article'

CREATE TABLE wiki_articles (
    id            TEXT PRIMARY KEY,
    -- 'organization', NOT 'org' — see "One word for organizations" below.
    subject_type  TEXT NOT NULL CHECK (subject_type IN
                    ('person','place','organization','day','story','narrative_identity')),
    subject_id    TEXT NOT NULL,
    page_id       TEXT NOT NULL UNIQUE REFERENCES app_pages(id) ON DELETE CASCADE,
    auto_update   BOOLEAN NOT NULL DEFAULT false,
    refresh_after_new_refs INTEGER NOT NULL DEFAULT 10
                    CHECK (refresh_after_new_refs > 0),
    dirty_at         TIMESTAMPTZ,
    source_ref_count INTEGER NOT NULL DEFAULT 0,
    last_written_at  TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX idx_wiki_articles_subject ON wiki_articles (subject_type, subject_id);
CREATE INDEX idx_wiki_articles_dirty ON wiki_articles (dirty_at) WHERE dirty_at IS NOT NULL;
-- Singletons: UNIQUE(subject_type, subject_id) does NOT stop two NI articles
-- with different subject_ids. Same idiom as idx_narrative_telos_single_active.
CREATE UNIQUE INDEX idx_wiki_articles_singleton ON wiki_articles (subject_type)
    WHERE subject_type = 'narrative_identity';
CREATE TRIGGER set_updated_at BEFORE UPDATE ON wiki_articles
    FOR EACH ROW EXECUTE FUNCTION tg_set_updated_at();

-- NOT a btree on `kind`. Two values, and selectivity INVERTS over time — 18
-- pages today vs 573+ articles later — so the planner would use it for
-- neither. Partial indexes on the queries the Pages list actually runs:
CREATE INDEX idx_pages_updated_pages ON app_pages (updated_at DESC) WHERE kind = 'page';
CREATE INDEX idx_pages_title_pages   ON app_pages (title)           WHERE kind = 'page';
```

Every index is named. 167 of the 168 index statements in `migrations/` are
explicit `idx_*`; an auto-named index also cannot be renamed later by the
pattern 0033 used. `updated_at` + trigger because five of these columns are
mutable and every `wiki_*` table in 0006 carries the trigger.

**One word for organizations.** There are three in the tree today:
`wiki_entity_refs.entity_type` says `'organization'` (0006) and every live query
agrees — `entity_article_gen.rs:79`, `day_summary.rs:599`, `places.rs:250`,
`sql_query.rs:56` — while `wiki_marginalia.subject_type` says `'org'` and the
frontend route is `/org`. This plan uses **`'organization'` in all new schema**,
because §2's sweep and §3's `interaction_count` both join `wiki_articles` to
`wiki_entity_refs`, and `'org'` would make those joins **silently return zero
org rows**. The frontend route stays `/org`; the mapping happens at the edge,
once. (`wiki_entity_refs.entity_type` also still permits `'thing'`, whose table
0071 dropped — worth cleaning in the same migration.)

**`kind` on the page, bookkeeping on the join row.** A predicate someone forgets
to write is a leak; a column with a default is not — so the discriminator sits
where every list query already looks. But the page does not need to know it is
about a person named Sarah, and `auto_update`/`dirty_at`/`source_ref_count` are
meaningless on 99% of rows, so those stay in the wiki's own table.

The denormalization is deliberate: `kind='article'` and "has a `wiki_articles`
row" encode the same fact twice and can drift. Accepted, because the alternative
is remembering a join at every call site, and contained by there being exactly
**one** creation path — a `create_article()` helper that writes both. Never mint
an article page any other way.

**Where that helper lives is constrained.** Applets link core as a library
(`applets/Cargo.toml` declares `virtues = { path = "../virtues-core" }`), so
`create_article()` must be a `pub fn(&PgPool, …)` in **virtues-core** — beside
`pages::create_page`, or in a new `api/wiki_articles.rs`. It must not live in
`server/api.rs` (axum + `AppState`) and must not take `YjsState`, or the
"one creation path" invariant breaks on day one for every applet.

**Both halves of the write path already exist.** First write:
`create_page(title, content)`
([page_editor.rs:124](../virtues-core/src/tools/page_editor.rs:124)) stores
markdown with no `yjs_state`, and the Yjs layer seeds `Y.Text` from that column
on first open ([yjs.rs:105](../virtues-core/src/server/yjs.rs:105)) — the CRDT
is created lazily, correctly, with nothing server-side constructing one.
Appending (what NI's *Add* does) has a primitive too: `YjsState::append_markdown`
([yjs.rs:543](../virtues-core/src/server/yjs.rs:543)).

**Visibility is asymmetric.** Articles are excluded from the Pages list and tree
— an article is not a document you made — and included in search, because it is
prose about your life. Later, `kind` allows a filter chip rather than permanent
hiding.

**The ontology must split on `kind` in the same phase.** `app_pages` has an
`OntologyDescriptor` ([ontologies.rs:875](../crates/virtues-registry/src/ontologies.rs:875))
that indexes it as `content_type: "page"` with a `day_source` of
`source_type: "page"`. Untouched, that produces three bugs on day one:

1. Every article rewrite appears **in the day view** as "you wrote a page
   today" — authorship the user did not perform, at applet volume. Confirmed
   live: the descriptor uses `timestamp_column: "updated_at"` with
   `use_date_filter: true`, and every CRDT flush bumps `updated_at`. This is the
   same provenance failure that already bit the day summary once, when calendar
   RSVPs were read as attendance.
2. The record reads as the user's own writing — the descriptor's own comment
   calls `app_pages` *"your own writing"*
   ([ontologies.rs:885](../crates/virtues-registry/src/ontologies.rs:885)); the
   user-facing label is "Page Edits", from a hand-written map in
   `apps/web/src/lib/wiki/ontology.ts`.
3. Articles become retrievable as evidence for the next article. Not true
   *today* — `entity_article_gen::build_dossier` is pure SQL over
   `wiki_entity_refs`/`wiki_days` and never touches `search_embeddings` — but it
   is true of chat retrieval now, and of any retrieval-based writer later.

**The fix needs a registry change, not a config edit.** `EmbeddingConfig`
([ontologies.rs:46](../crates/virtues-registry/src/ontologies.rs:46)) has **no
filter field**; only `DaySourceConfig` has `extra_where`. The indexer builds
`FROM {table} t` with no user predicate
([indexer.rs:191](../virtues-core/src/search/indexer.rs:191)). So two
descriptors over `app_pages` do not split — they **double-index**: rows key on
`(ontology, record_id, chunk_index)`, so every page gets embedded twice under
two ontology names. Two further breakages, both silent:

- `api/records.rs:49` resolves an ontology by `table_name` with `.find()` —
  first match wins, so `/record/app_pages/<id>` becomes arbitrary.
- `attach_record_refs` ([sql_query.rs:588](../virtues-core/src/tools/sql_query.rs:588))
  **bails when more than one ontology matches a table name**, so every SQL-tool
  result touching `app_pages` quietly loses its citation ref. No error.

So Phase 1 must add an `embed_where` to `EmbeddingConfig` and thread it into the
indexer's backlog query — and it must carry its own leading `AND`, with the test
that already exists for the `DaySourceConfig` equivalent
(`extra_where_carries_its_own_and`, written because `activity_app_session`
shipped without it and app sessions vanished from every day while the nightly
cron reported success).

Then: `app_page` scoped to `kind='page'`, a `wiki_article` descriptor for
`kind='article'` with `day_source: None` and its own `content_type`, and the
matching entry in `ontology.ts` or articles render as a raw identifier.
**Articles are excluded from the day-summary dossier and the article-writing
dossier — the AI never reads its own prose back as evidence — and stay in
user-facing search.**

**Indexing is free and stays fresh** — confirmed, not assumed. The indexer's
backlog query is `WHERE se.id IS NULL OR se.doc_hash IS DISTINCT FROM
md5(embed_text)` ([indexer.rs:196](../virtues-core/src/search/indexer.rs:196)),
so an edited article re-embeds. (`on_content_updated`
([yjs.rs:414](../virtues-core/src/server/yjs.rs:414)) is a stub, but it is a
latency optimization — push instead of the 15-minute cron — not a correctness
hole.)

**Deletion is six handlers, not one, and one of them does not exist yet.**
`page_id → app_pages ON DELETE CASCADE` means deleting the *page* removes the
join row. Nothing cascades from the *subject*: `subject_id` has no FK and cannot
have one, since it points at six tables. Measured state today —

| Subject | Delete path | After |
|---|---|---|
| place | `api/entities.rs:285` — nulls `home_place_id`, nothing else | orphan article + orphan page + orphan `wiki_entity_refs` |
| person | `entity_resolution/people.rs:921`, an internal cleanup path only | same |
| organization / story / narrative_identity | **no delete handler at all** | n/a |
| day | no `DELETE FROM wiki_days` anywhere — days are upserted | low risk |

The user-facing delete arrives in §13/Phase 7, so **Phase 1 ships with orphaned
article pages possible** until then. State it rather than discover it.

And deletion must clear the index: `api/pages.rs` deletes a page without
touching `search_embeddings`, and nothing else reaps vanished records — the only
`DELETE` there trims surplus chunks of a record being re-embedded. Deleted
people would stay searchable as prose. `annotations.rs:293` already does this
correctly (`DELETE FROM search_embeddings WHERE ontology = … AND record_id = $1`);
`create_article()` needs a `delete_article()` peer that does the same.

**The frontend composes separately.** The article view is the page renderer with
different chrome — a byline, the maintenance toggle, the Notes rail. Same
editor, different wrapper.

### 2. Articles are opt-in. Nothing is written until asked.

Today `entity_article_gen` sweeps **every** entity past a hardcoded bar
([entity_article_gen.rs:30](../virtues-core/src/api/entity_article_gen.rs:30)):
`MIN_REFS_TO_WRITE = 15`, `MIN_NEW_REFS = 10`, `MAX_ENTITIES_PER_RUN = 2`. On
the measured box that is **226 entities** eligible for prose nobody requested —
an invisible recurring cost on five months of data, and thousands on a box with
years.

| Old constant | Becomes |
|---|---|
| `MIN_REFS_TO_WRITE` | **Gone.** The gate is the user clicking *Write the article*. Survives at most as a hint ("only 3 records — this will be thin"), never a block. |
| `MIN_NEW_REFS` | **Per-article** `refresh_after_new_refs`, default 10, read only when `auto_update = true`. |
| `MAX_ENTITIES_PER_RUN` | **Stays as a system rail, not a policy** — a timeout/cost limiter. The eligible pool is now user-bounded, so it can rise (≈5). |

Lifecycle: an entity with no article shows its records and a **Write the
article** button → one explicit generation → the article exists with
`auto_update = false` → a separate **Keep this updated** toggle opts into
maintenance. Writing once and maintaining forever are different consents.

**`auto_update = false` means the AI never touches this article.** It is not a
pending-approval queue and nothing is held for review: the maintenance sweep
simply skips the article. It changes only when the user regenerates it by hand,
triggers it explicitly, or turns the toggle on. So there is no approval gate to
build — the switch *is* the consent, and History (§16) plus revert is the review
surface for articles that do have it on.

**"Write the article" needs a handler, and the applet path will not do.** The
rails look like they exist — `POST /api/applets/:id/run` with trigger `manual`,
and `entity_article` already declares `triggers = ["cron","manual","tool"]` — but
three things block it: the applet ships `default_enabled = false` and
`prepare_run` **rejects disabled applets outright**, so the button 404s on a
fresh box; the singleton concurrency gate turns a second click into a `skipped`
run, which is exactly wrong for per-subject work; and `entity_article/main.rs`
calls `read_input()` then ignores it, so there is no "write *this* one" entry
point at all.

So: a plain HTTP handler that calls generation directly for one subject, leaving
the applet as the cron/maintenance host only. It returns the article; no polling
`app_applet_runs`.

**There is nothing to migrate here.** Zero entity articles exist on the real
box; `entity_article_gen` has never produced one. (The 13 `wiki_days`
autobiographies are the only prose that moves — §15.)

*Later, not now:* the natural home for choosing an initial set is onboarding —
"here are your most-contacted people and most-visited places, shall I write
these?" — which derives the same result from data the user recognizes, as an
act they perform. Out of scope for this arc.

### 3. Entity hygiene comes before prose.

Measured on the real box: **`interaction_count` is 0 on all 573 people.** The
column exists and nothing maintains it, so the People index has no order at all
— what looks like a ranking is an arbitrary scan.

Provenance splits cleanly:

| Source | Rows | What they are |
|---|---|---|
| `ios_contacts` | 531 | the user's own address book — **not noise; show them all** |
| (none) | 42 | minted by `extract_name_from_email()` in `entity_resolution/people.rs` for any unseen sender |

The second group is where `Gusto <automated@gusto.com>`,
`Slack <no-reply@slack.com>` and `The Plaid Team <info@email.plaid.com>` came
from — and Gusto and Slack each appear **twice**, one row per sending address.
The company problem and the duplicate problem are one bug: an email address
becomes a person with no test for whether a person is on the other end.

**The fix is ranking, not deletion.** `wiki_entity_refs` holds **130,259 message
refs across 314 distinct people** — a real interaction signal that is simply
never written to the column. Derive it, and the wall sorts itself: people you
message rise, address-book contacts with no traffic sit below them, and
`no-reply@` lands at the bottom with two email refs. No classifier, no AI, no
deletion.

So, in order:

1. **Derive one measure — `ref_count`, from `wiki_entity_refs`** — and make it
   the default sort of every entity index. Importance as a proxy, never a
   judgment about who matters. It must be a *new* uniform measure, not the
   existing columns: `wiki_people` has `interaction_count`, `wiki_places` has
   **`visit_count`**, and visits are not refs, so "the default sort of every
   index" is otherwise three different quantities. Treat both legacy columns as
   deprecated. Read-only `GROUP BY` over 131k rows with the indexes 0006 already
   provides, plus an `UPDATE` of 573 rows — seconds, not a hazard.
2. **Show every person.** The record stays complete; the noise sorts to the
   bottom rather than being hidden by a bar someone has to justify.
3. **Reclassify** — "this is an organization, not a person," bulk-selectable,
   with `no-reply|noreply|automated|info@|support@` local-parts pre-flagged.
4. **Nickname / relationship / alias UI.** 0037 calls an alias *"the record of a
   human decision"* and nothing lets anyone make the decision: 3 of 573 people
   have an alias, 0 have a nickname, 1 has a relationship category. Editable
   fields differ per table and the UI must not assume otherwise — only
   `wiki_people` has `nickname` and `relationship_category`; `wiki_orgs` has
   `relationship_type`/`role_title`; `wiki_places` has neither. `aliases` is the
   one field all three share (0037).

**Merge is not part of this**, though the duplicates make the case for it. It is
the one operation here that can corrupt the record, and nothing else waits on
it — so it gets its own pass (Phase 10). Ranking makes the duplicates tolerable
in the meantime: both Gustos simply sit low.

This is unglamorous and it is the highest-value work in the arc, because
articles, notes, history and the lifeline all sit *behind* the People index.

### 4. There is a person row for you.

The wiki has no self node today, so every relationship is one-sided —
`relationship_category` on a person says "sister" without saying *to whom* — and
"who is my family" has nowhere to live.

**BUILT — migration 0080.** The pointer went on the profile, not as an `is_self`
flag on `wiki_people`:

```sql
ALTER TABLE app_user_profile ADD COLUMN self_person_id TEXT;  -- soft ref, no FK
```

Three reasons the direction reversed once the code was read.
`app_user_profile` is already the singleton answering "who owns this
appliance", and it already soft-references the graph exactly this way —
`home_place_id` points into `wiki_places` with no FK (0003). Singularity then
becomes **structural**: one column on a one-row table cannot hold two selves,
where a flag on the many-side needs a partial unique index to say the same
thing and can be violated in between. And it settles the ownership question by
the direction of the arrow.

No FK, matching `home_place_id`: the wiki tables are rebuilt by resolution
passes, and a hard FK from the profile would make an ordinary entity cleanup
fail against the one row on the box that must never fail.

The migration backfills **under the 0037 rule** — link only when the surface
matches exactly one person. On the real box that resolves `Adam Jace` and
correctly declines `adam` (a separate row, 25 refs). Getting this wrong would
attribute the owner's entire message history to a stranger, so ambiguity must
not be allowed to resolve itself.

It is the **graph node**: the anchor every `relationship_category` is relative
to, and the owner of your own messages (`is_from_me` already rides on message
metadata). It gets an article like any other person.

**There are four "who am I" constructs, not two, and only one owns each fact.**
`app_user_profile` (0003) already holds `full_name`, `preferred_name`,
`birth_date`, `occupation`, `employer`, `home_place_id` — and `build_user_context`
([chat.rs:558](../virtues-core/src/api/chat.rs:558)) already injects it into
every prompt. The self row must not duplicate it.

| Construct | Owns | Consumer |
|---|---|---|
| `app_user_profile` (0003) | account facts — name, birth date, occupation | already in every prompt |
| `wiki_people` self row (§4) | the graph node: relationships, your own messages | the wiki, the graph |
| `wiki_narrative_identity` (§11) | the telos document — values, character | already in every prompt |
| `wiki_telos` (0006) | which era a day belongs to (acts → chapters) | out of scope |

`app_user_profile` is authoritative for the name; the self row references it
rather than restating it.

### 5. Articles link to subjects. That is what makes it a wiki.

Everything else here would give a set of generated report pages with a shared
editor. The motion that makes a wikipedia a wikipedia is *link → link → link*:
Sarah's article names the day you met, the day names the place, the place lists
who else was there.

**The link target is a subject, never an article.** An earlier draft said
article↔article, which cannot work: §2 makes articles opt-in, so most subjects
have no prose — and there is no article route and no article id in any link
anyway. Production ref-routes name subjects: `/person/person_ab12`,
`/day/day_2026-03-03` (`get_entity_url`,
[pages.rs:540](../virtues-core/src/api/pages.rs:540)). A backlink whose target
has no prose is still meaningful, because it renders on the **subject view**,
which always exists.

So the edge is `(source_page_id, target_route)` — keyed by **route identity, not
an FK**. `/day/day_2026-03-03` may have no `wiki_days` row at all (42 rows
across 155 days), so an FK is not merely inconvenient, it is wrong.

**Forward links are authored during generation, not matched afterwards.** These
are two different mechanisms and an earlier draft conflated them:

- **What ships today:** the writer is handed an explicit *"Entities you may
  link"* list and copies the exact markdown — `[Maya](/person/person_ab12)`, one
  per entity on first mention, never invented
  ([day_summary.rs:103](../virtues-core/src/api/day_summary.rs:103),
  `entity_article_gen.rs:53`). Recall is bounded by the dossier, and the writer
  already knows which entity it meant.
- **What we are NOT building:** a post-hoc surface matcher that rewrites prose.
  That would be a rewriting pass over a CRDT **that also contains human text** —
  it could relink a sentence the user typed. 0037's exactly-one rule governs
  mentions extracted from *records*; here it belongs one step earlier, choosing
  what goes on the candidate list.

Auto-linking bare surfaces is a separate, later feature, and it must never
rewrite user-typed text.

**Backlinks already have a precedent — check it before building a table.**
`get_page_backlinks` ([pages.rs:262](../virtues-core/src/api/pages.rs:262))
derives page→page backlinks at read time with `LIKE '%/page/{id})%'` over
`app_pages.content`. At this corpus size that may simply be enough; an on-save
edge table is an optimization that should be justified by a measurement, not
assumed. If one is built, the only correct hook is `save_and_materialize`
([yjs.rs:387](../virtues-core/src/server/yjs.rs:387)) — the sole place `content`
is materialized, and the same site where `on_content_updated` is already a stub.

**Coverage caveat for §15's "13 articles on day one":** `NARRATE_PROMPT`'s
allowlist is **people and places only** — no organizations, no day→day. Those 13
day articles yield day→person and day→place edges and nothing else.

### 6. Wiki search: the pieces exist; the ranking does not.

A wikipedia has two search motions, and we have most of both.

**"Go to"** — type *Sarah*, land on Sarah's article. `search_refs`
([pages.rs:567](../virtues-core/src/api/pages.rs:567)) already does this: a
prefix/contains `ILIKE` UNION over people, places, orgs, files and pages,
ranked prefix-before-contains. It powers @-mentions, the RefPicker and Desk
pins. Two real gaps:

- **It never consults `aliases`.** 0037 added `aliases JSONB` to all three
  entity tables specifically so one human decision resolves a name forever — and
  the navigator does not read the column. The obvious patch is wrong: `$1` is
  bound to the *contains* pattern `%q%`, so `aliases ? lower($1)` can never
  match. It needs a **new bind of the raw lowercased query** — aliases are
  already stored lowercased with a GIN index (0037), so `aliases ? $4` works.
  Note the UNION has **seven** branches (people, places, orgs, files, pages,
  chats, notebooks), not the five an earlier draft listed.
- **Articles will arrive typed as `page`.** With the wrapper, an article must
  resolve as *the person*, not as a page — a `kind` filter plus subject typing.

**"Search within"** — full text over article prose. **Free.** Articles enter
`search_embeddings` through the ontology and are served by the existing hybrid
stack (dense + BM25, z-fusion, rerank). Nothing to build.

**What is missing is smaller than it looks, because the doctrine already
exists.** `search_local.rs` states it outright: objects and content are
**grouped, never interleaved by score**, precisely because merging them is the
score-scale schism from [ir-notes.md](./ir-notes.md) — and `SearchModal.svelte`
already renders "In your records" as its own group. §6's "names before passages"
**is** that grouping, so it is nearly free, and the reranker problem touches the
passage leg only.

Two real pieces remain. The palette gets its objects from **client stores**, and
entities are in no client store — so the wiki box needs the server `search_refs`
leg the palette does not use. And scoping: `SearchFilters.ontologies` already
exists, but `LocalSearchRequest` carries only `{q, limit}` and hardcodes
`SearchFilters::default()`, so the wiki-scoped variant needs the field threaded
through.

(`POST` for the content leg is deliberate — a query about your own life must
never land in a URL or an access log. Keep it. Also note the passage leg
requires the embedder, so on `dev-real` only the `search_refs` leg is
testable.)

### 7. Two things, two names: `app_marginalia` and `wiki_notes`.

An earlier draft unified these into one table. That was wrong. They differ in
every dimension that matters:

| | `app_marginalia` (was `app_annotations`, 0057) | `wiki_notes` (was `wiki_marginalia`, 0033) |
|---|---|---|
| what | a note beside a passage in a document you are reading | an editorial note about a subject in your record |
| anchor | file + page + quote (+ rects) | subject only — never a passage |
| author | you | you or the machine |
| cites | no | machine notes: required |
| lifecycle | permanent; a highlight is never "resolved" | open → accepted / dismissed / absorbed |
| built | fully: create/list/patch, markdown export per file and per notebook, its own indexed ontology | schema only, zero producers |
| rows on the real box | **1** | **0** |

**Marginalia is the literal thing** — a note written in the margin of a page you
are reading — so the word goes to the document reader, where it is exact. The
wiki's editorial notes are plainer and get the plainer word.

The unification argument was that wiki notes would eventually need quote
anchoring and would rebuild what 0057 already solved. Splitting them retires
that argument rather than answering it: **`wiki_notes` are subject-scoped and
never get quote anchoring**, so there is nothing to rebuild. A machine note that
disputes a sentence names the sentence in its prose and cites the source; it
does not attach to a character range.

Both renames are safe now — 1 row and 0 rows — and will never be cheaper.
`ALTER TABLE … RENAME` is catalog-only: no rewrite, no copy, instant, inside the
transaction sqlx wraps every migration in. Add-new + backfill + drop would be
*more* dangerous here, doubling the migration count and adding a real drop for
no benefit at one row.

**But a rename renames only the table.** Left behind with their old names:
`idx_wiki_marginalia_subject`, the auto-named `wiki_marginalia_subject_type_check`
/ `_kind_check` / `_author_check`, and the identity sequence
`wiki_marginalia_id_seq`; likewise `idx_app_annotations_file`. Consequence for
this plan specifically: **"the `subject_type` CHECK gains `narrative_identity`"
cannot be written the obvious way** — `DROP CONSTRAINT wiki_notes_subject_type_check`
fails and aborts the migration mid-upgrade. The rename migration must carry
`ALTER INDEX … RENAME`, `ALTER TABLE … RENAME CONSTRAINT`, and
`ALTER SEQUENCE … RENAME` first. 0033 is the in-repo precedent — it renamed four
indexes after the notebook rename.

**And the index rows must be re-pointed in the same migration.** There is no GC
path in `search/` at all: nothing ever deletes rows whose ontology no longer
exists. Renaming the `document_annotation` ontology strands its rows forever.
Subtler and worse: renaming the *table* while keeping the ontology name leaves
`source_table` stale **permanently**, because the upsert only fires when
`doc_hash` changes and a rename does not change `md5(embed_text)` — and
`search/query.rs` joins entity filters on `source_table`. So:
`UPDATE search_embeddings SET ontology = …, source_table = … WHERE ontology = <old>`
(or `DELETE` and let the 15-minute indexer re-embed — one row, free).

**The SQL is not the cost; the fan-out is.** Thirteen sites, and **none of them
is compile-time checked** — the six annotation SQL sites are runtime
`sqlx::query`, and the frontend is seven string URLs: the ontology `name` and
`table_name`, `api/annotations.rs`, `search/query.rs`, `tools/semantic_search.rs`,
five routes in `server/mod.rs`, five handlers in `server/api.rs`,
`apps/web/src/lib/api/client.ts`. A missed site is a runtime 500, not a build
error.

**If the API path moves, alias it for one release.** The iOS app bundles
`apps/web` as a Tauri SPA. Web ships in the slot atomically with the box; the
**installed** iOS build does not — so `/api/annotations` → `/api/marginalia`
404s every older iOS install until the user updates.

`wiki_people.notes` is dropped in the same move: a user's prose about a person
is a note on that person, and should be a `wiki_notes` row rather than sit
beside one. That column, and `content` on people, places and orgs, are **empty
on the real box** (0 / 0 / 0 / 0), so it costs nothing.

#### The vocabulary

- **In a document:** *marginalia*. The rail is the margin; a marked passage is a
  **highlight**; a highlight with text, or text with no passage, is a note in
  the marginalia.
- **In the wiki:** *notes*. The rail is **Notes**; the unit is a **note** —
  *Add a note*, *3 open notes*. It has a singular, which "marginalia" does not:
  *"add a marginalia"* is not a sentence.
- **"Annotation" retires from the screen and the API.** It was only ever the
  implementation word.
- **Three columns already spell `note` and are not renamed:**
  `wiki_bookmarks.note` (0073 — whose own comment calls it "user-authored
  marginalia"), `wiki_events.user_notes` (0006 — and §14 makes an event a note
  subject, so an event carries both), and `app_marginalia.note_md`. The last is
  fine and even correct: a note *in the marginalia* is exactly what it holds.
  The word is shared vocabulary, not an owned namespace — say so rather than
  claim exclusivity the schema does not support.

### 8. The note covenant.

> **A machine note may only be written by a pass that held a complete session in
> context** — a finished day, an event, a chat thread. Never a sweep over
> isolated rows.

This is the fix for what actually killed semantic ER (0061, 0062): the model saw
a fragment and could not tell sarcasm from statement, or when a thing was said
from when it happened, because the unit of reading was a row.

Three rules hold it:

1. **Point, don't decide.** *"Sarah may have moved to Denver — group thread,
   Jul 12, tone ambiguous; the article doesn't mention it"* is useful **even
   when wrong**, because the citation makes it checkable in seconds.
   `sarah → denver, confidence 0.6` is worthless when wrong, because there is
   nothing to check. A note shortens the path to evidence; it does not
   replace judgment. Sarcasm detection is not required for that to pay.
2. **Cite or reject** — `source_refs` enforced at write time, not requested in a
   prompt.
3. **Silence is the default.** The bar is *the article is contradicted, or
   materially incomplete on something this session settles* — never "Sarah was
   mentioned."

**The writer may never write `wiki_entity_refs`.** Not at confidence 0.5, not
flagged, not ever. Promotion is a human click or an editor pass gated on
`auto_update`. The graph stays deterministic and user-authored.

```sql
-- Rename the table, THEN its index, constraints and sequence — Postgres
-- renames none of those for you (§7).
ALTER TABLE wiki_marginalia RENAME TO wiki_notes;
ALTER INDEX idx_wiki_marginalia_subject RENAME TO idx_wiki_notes_subject;
ALTER SEQUENCE wiki_marginalia_id_seq  RENAME TO wiki_notes_id_seq;
ALTER TABLE wiki_notes RENAME CONSTRAINT wiki_marginalia_subject_type_check
                                      TO wiki_notes_subject_type_check;
-- …same for _kind_check and _author_check. Only now can the CHECK be widened:
ALTER TABLE wiki_notes DROP CONSTRAINT wiki_notes_subject_type_check;
ALTER TABLE wiki_notes ADD  CONSTRAINT wiki_notes_subject_type_check
    CHECK (subject_type IN ('event','day','story','person','place','organization',
                            'chat','page','narrative_identity'));
    -- 'organization' not 'org'; 'telos' dropped — §11 forbids the merge it invites.

ALTER TABLE wiki_notes ADD COLUMN source_refs JSONB NOT NULL DEFAULT '[]'::jsonb;
ALTER TABLE wiki_notes ADD COLUMN resolved_at TIMESTAMPTZ;
ALTER TABLE wiki_notes ADD COLUMN resolution  TEXT
    CHECK (resolution IN ('accepted','dismissed','absorbed'));
ALTER TABLE wiki_notes ADD COLUMN resolved_by TEXT
    CHECK (resolved_by IN ('ai','human'));
-- The two resolution columns cannot disagree.
ALTER TABLE wiki_notes ADD CONSTRAINT wiki_notes_resolution_pair
    CHECK ((resolved_at IS NULL) = (resolution IS NULL));
-- "Cite or reject" is an invariant, so the DB holds it — not just Rust.
ALTER TABLE wiki_notes ADD CONSTRAINT wiki_notes_machine_must_cite
    CHECK (author = 'human' OR jsonb_array_length(source_refs) > 0);
-- Replaces idx_wiki_notes_subject, which this is a prefix of. Drop that one.
CREATE INDEX idx_wiki_notes_open ON wiki_notes (subject_type, subject_id)
    WHERE resolved_at IS NULL;
```

**`kind` is inherited and must be assigned.** The surviving CHECK is
`('observation','style_note','correction','appraisal','memo','provenance')`.
Map, do not extend: a note that **disputes the article** is `correction`; a note
about **the subject** that the article lacks is `observation`; an NI proposal is
`observation`. Nothing else writes.

That mapping also does the work quote anchoring would have: *accept* on a
`correction` means "edit that sentence," on an `observation` means "append."
One field to branch on, no character ranges. A `correction` also wants a
nullable `written_against_version` (an `app_page_versions.version_number`), or a
note contesting a sentence the user has since deleted is permanently
unresolvable.

#### What a note can point at

`subject_type` already covers event, day, story, person, place, organization,
chat, page and narrative_identity. Two additions settle the rest.

**Time is a RANGE, not a date with a precision flag.** Language is vague about
time and a `DATE` is not: "March", "the spring", "sometime after June" all need
somewhere to go. A range expresses granularity by its own width and needs no
enum to drift out of step —

| said | stored |
|---|---|
| "the 14th" | `[2026-03-14, 2026-03-15)` |
| "March" | `[2026-03-01, 2026-04-01)` |
| "the spring" | `[2026-03-01, 2026-06-01)` |
| "sometime after June" | `[2026-06-01, NULL)` |

so `refers_to_start` / `refers_to_end`, both nullable, both usually null. This
is what turns a note into something that can **come due**: "what is waiting on
this week" is an overlap test rather than a switch on precision, and an
open-ended reference is expressible at all, which an enum could not manage.

**One note, one subject.** A margin note lives in one margin; three rows with
the same body is noise. The writer picks the primary subject — usually the
durable one, the person — and names the others as ref-route links in the prose.
Extending the backlink scan to `wiki_notes.body` then surfaces that note on
their pages for free, using machinery that already exists. Multiplicity is a
rendering question, not a schema one.

**And a floating subject stays floating.** *"He recommended a book called The
Peregrine"* has no entity, and the covenant forbids inventing one — that is
precisely what 0071 deleted `wiki_things` to stop. It is filed on the person
with the title in the prose, and search finds it. That is the correct answer
rather than a gap, but it is worth naming the ceiling: **a note can only point
at what the record already names.**

**No dedupe key.** An earlier draft gave each note a machine-comparable key with
a unique index behind it. That presumes a note has structured identity, and it
does not — **a note is prose with citations**, written for later, to be folded
into the subject's story by whoever edits next. Prose has no key, and a key would
not survive rephrasing anyway (*"Sarah may have moved"* / *"Sarah might be in
Denver now"* — two keys, one observation). Instead:

- **The writer reads the subject's open notes first** — the read-before-write
  0033 already specifies. It sees "there is already a note about Denver" as
  *context*, and declines in prose, the only register that can tell those two
  sentences apart.
- **Recurrence is signal.** Five notes circling Denver over three weeks *is* the
  "this article is out of date" trigger. Deduping would delete the evidence that
  the article needs work. Count them; surface the count.

**Notes never age out.** A note whose purpose is "prose for later" that deletes
itself before later arrives has defeated itself — silently, which is worse: the
record quietly forgets something it chose to write down. Three exits, all events
rather than timers:

| Resolution | `resolved_by` | Who |
|---|---|---|
| `accepted` | `human` | a human folds it into the article |
| `dismissed` | `human` | a human says no |
| `absorbed` | `ai` | an article rewrite incorporated it |

**`absorbed` must be reported, not inferred.** An earlier draft left it as "an
article rewrite incorporates it," with no evaluator — and there can be none:
§10's maintenance edit knows which notes it was *given*, never whether the text
it emitted reflects any one of them. So the edit **takes explicit `note_ids` and
stamps exactly the ids it reports using.** Without that, absorption never fires,
the backlog drains only by hand, and §8's diagnosis of unbounded growth ("the
writer's bar is too low") becomes unfalsifiable — growth would be equally
explained by the missing exit.

With it, absorption is the exit most notes take and the reason the backlog
drains without a clock.

### 9. Notes are a THIRD pass, and they look outward.

An earlier draft made the day-summary narration the note writer, on the grounds
that it already held the day's context. Measurement killed that: the day dossier
carries **message counts, not message text** —
`SELECT canonical_name AS who, COUNT(DISTINCT m.id) ... GROUP BY who`
([day_summary.rs:1208](../virtues-core/src/api/day_summary.rs:1208)). The pass
sees *"Sarah: 14 messages"* and never a word of what was said. A note is made
entirely of what was said.

Worse, the two jobs pull against each other. The chain is a compression funnel —
~1,000 rows → dossier → ~14 events → four sentences — and `NARRATE_PROMPT` says
so outright: only what distinguished the day survives. **Notes are made of
exactly what compression discards.** "Sarah moved to Denver" is never a day's
headline.

#### The question a note pass asks

Not *what was this day* — the chain already answered that. Instead:

> With the context of the day, **find what this day reveals about things that
> are not this day** — other days, people, places, the future, the past.

That is a different question of the same material, which is why it stops
fighting compression. It also yields the bar, stated plainly in the prompt:

> **If the note is about today, it isn't a note.**
>
> *"You had coffee with Maya"* is the day. *"Maya mentioned she's leaving in
> March"* is a note, because March isn't today.

Necessary but not sufficient. The second half is significance:

> **If the article wouldn't change, it isn't a note.**

*"He recommended a book called The Peregrine"* passes the first test and fails
this one — Sarah's article does not change, so it is not a note. This is
mechanically checkable **only if the pass is handed the current articles** of
the subjects in play: *here is what the record says about Sarah; does this
change it?* For a subject with no article yet: *would this be in one?*

#### Where it runs

A **third call at the end of the same nightly run**, after NARRATE. Not folded
into SEGMENT (its JSON is the fragile part, and it runs before scoring) and not
into NARRATE (which cannot see content and exists to compress). The "no new
process" economy is real — it was attached to the wrong stage. The dossier is
rebuilt there (~40 ms of SQL) rather than threaded through six scoring steps.

#### What it reads

Only **discrete, document-shaped** sources. The registry already draws this line
and its own test states it: a signal annotates an event, it is never a row in
the corpus. A note can never come from a heart-rate series.

| source | median/day | verdict |
|---|---|---|
| calendar | **43 chars** | wholesale — nearly free, and titles are dense with the future |
| messages | 2.5k | wholesale, each body capped ~320 chars (p99) |
| own chats | 19k | capped — the least ambiguous evidence on the box; it is where you *say* the thing |
| transcriptions | 65k | all of it, after dedupe; cap any single row |
| email | 90k | **filtered** — 74% is machine mail |

No selection by novelty. Choosing what is interesting before the model reads it
pre-empts the only judgment the pass exists to make.

**Email filtering is reciprocity, not vendor labels.** Gmail's
`CATEGORY_PERSONAL` is stored and free to use where present (102 of 313 rows),
but it does not exist on IMAP or Outlook. The universal signal is that
**newsletters are one-way**: has the user ever sent mail to this address? That,
plus sender-resolves-to-a-person (82 of 313), needs no vendor taxonomy.
`List-Unsubscribe` is a third layer if headers are retained.

#### Before any of this: fix the silent failure

`parse_events_salvaging` is right — a retry costs another premium call and would
truncate in the same place. But `.unwrap_or_default()` turns "nothing
salvageable" into zero events **and `segmented_at` is stamped anyway**, so a
garbage response is indistinguishable from a quiet day, permanently, and the
catch-up queue never revisits it. Do not stamp when zero were salvaged. This
matters more once the dossier carries text, because a larger prompt truncates
more often. (The only retry anywhere is on a 402 wallet-empty; a 5xx loses the
day.)

#### Volume is capped in code, not in the prompt

**≤3 notes per run**, enforced in the applet and logged when it binds — the log
line is the measurement that makes "the writer's bar is too low" falsifiable.
Restraint is what models comply with least, and a note per subject on a rich day
is ~4,000 a year: the rail becomes a wall nobody reads.

#### Known coverage gap, accepted

A day-scoped writer notices only what surfaces on a day it processes: never a
pattern ("Sarah has cancelled four times in three months"), never anything
before it started running. Do not build a second pass. It means the rewrite
trigger is two-sourced — **novelty** from day notes, **accumulation** from
`refresh_after_new_refs`.


### 10. Articles are edited, never regenerated — and the edit runs in the agent phase.

**Where the writer lives.** An applet run has two phases
([applet_runner/mod.rs:364](../virtues-core/src/applet_runner/mod.rs:364)): the
subprocess, and then — if the applet declares an `agent` prompt — an agent loop
that runs **in-process with `deps.yjs`**, handed the subprocess summary as
context. All 22 applets are `runtime = "function"` today and none use the second
phase.

That boundary is not cosmetic. A `function` subprocess holds a bare pool from
`connect_from_env` and has no `YjsState`, so an article edit made there is
**silently lost**: `get_or_create` ignores `content` whenever `yjs_state` is
non-null, and the next save rewrites `content` from the CRDT. §1's "both halves
of the write path already exist" holds only for the *first* write.

So `day_summary_eod` stays a cron applet and gains an agent prompt, splitting
along the line the covenant already draws:

| | phase | why |
|---|---|---|
| write notes (§9) | subprocess | plain `wiki_notes` rows, no CRDT — and the day's dossier is already assembled there |
| edit articles (this §) | agent | needs `YjsState`; `AgentLoop::new_with_yjs` makes `edit_page` correct |

**The subprocess observes and proposes; the agent edits prose.** The permission
boundary and the process boundary are the same line.

**What it writes.** The first write is a full generation; **every subsequent write is a targeted
edit** through the same find/replace path the AI already uses on pages.

Whole-document replacement is wrong twice: in a CRDT it is delete-all +
insert-all, discarding concurrent human edits **by construction**; and every
revision then diffs at 100%, so History shows "everything changed" on every
entry, which is the same as showing nothing.

So maintenance reads the current article, reads what is new (open notes + refs
since `source_ref_count`), and emits surgical replacements. Human edits survive
naturally, because the machine patches *around* them. This demotes
`wiki_days.last_edited_by` from a hard freeze to belt-and-braces.

### 11. Narrative identity: the telos document.

**What it is:** values and aspirations, character and temperament, virtues and
vices, addictions and trauma, the bucket list, what you want, who you are. The
things you would want an assistant to understand about your character — a
*telos* document, not a biography. §4's self person-row is who you are **in the
record**; this is who you are **to yourself**, and the two are never merged.

> **Naming collision, deliberate:** `wiki_telos` already exists (0006) as the
> parent of `wiki_acts` → `wiki_chapters` → days. That is a *structural* node —
> which era a day belongs to — not a values document. Out of scope for this arc:
> build nothing on it, and do not let the NI surface adopt the word in a way
> that invites someone to merge the two later. Note `wiki_marginalia.subject_type`
> already permits `'telos'` (0033) — §8's rewritten CHECK drops it, so the note
> table stops accepting the merge this paragraph forbids.

**No headings, no sections.** One document, raw prose — only the higher-order
salient pieces, in any order. Sections would mean four editors and a schema
argument the first time a fifth thing matters.

NI is already injected into every chat prompt
([prompt.rs:34](../virtues-core/src/agent/prompt.rs:34)) via
`build_narrative_identity()` ([chat.rs:539](../virtues-core/src/api/chat.rs:539)).

**Live bug, and it has never fired.** That function does
`content.chars().take(800)` — but `wiki_narrative_identity` has **zero rows**, so
`fetch_one` errors and NI resolves to `""` in every prompt today. Truncate at a
paragraph boundary and surface "over budget" in the editor rather than cutting
silently — but understand this as a build, not a repair.

There is a second, independent cause of the table being empty, and Phase 0 fixes
it for free: `narrative_identity_gen.rs:112` gates its dossier on
`wiki_people WHERE interaction_count >= 3`, and that column is 0 on all 573
people (§3). The NI draft applet's "people who matter" block has always been
empty. Deriving `interaction_count` silently changes what that applet produces —
worth knowing before it does.

**Budget: 2k tokens, for behavioral reasons, not economic ones.** ~1,300 words is
three dense pages about what a person values. Cost is not the constraint — NI
sits in the *system* prompt, so it is cached and the marginal cost is a cache
read. What 3–5k costs is precision: the prompt already spends four paragraphs
saying hold this lightly (*"the fastest way to lose trust is to psychoanalyze a
shopping list"*), and every extra paragraph is more surface for a spurious
connection to a routine question. A longer NI does not make the assistant
understand you better; it makes it perform understanding more often. Wanting more
is a signal the detail belongs in a person's article or a story.

**Propose, never write.** `propose_narrative_identity_edit(text, why)` — no
`section` param — writes a `wiki_notes` row on
`subject_type='narrative_identity'` and renders in chat as a card with **Add** /
**Dismiss**. Accepting appends a paragraph; the user edits freely afterward.

**Pruning is never automatic.** Deciding what has stopped being true about a
person is a value judgment, and it is the one place in the system most obviously
theirs.

### 12. Lifeline: the query contract now, the LOD later.

The console is a query problem before it is a chart problem, so the endpoint
shape is the decision that matters:

```
GET /api/wiki/lifeline?from=&to=&buckets=N&lanes=health,comms,location,…
→ { lanes: [ { id, buckets: [{ t0, t1, density, top: [span…] }] } ] }
```

Per-lane, per-bucket density plus a few representative spans; raw rows only past
a zoom threshold. Same shape as the `server` pagination prop the wiki redesign
added to `UniversalDataGrid`.

**Lanes** map to the existing `data_*` spine — the taxonomy is already there:

| Lane | Tables |
|---|---|
| Health | `data_health_sleep`, `_heart_rate`, `_hrv`, `_steps`, `_workout`, `_active_energy`, `_distance` |
| Location | `data_location_visit`, `data_location_point` |
| Communication | `data_communication_message`, `_email`, `_transcription` |
| Audio | `data_audio_session`, `data_audio_recording` |
| Activity | `data_activity_app_usage`, `_web_browsing`, `_listening` |
| Calendar | `data_calendar_event` |
| Finance | `data_financial_transaction`, `_account`, `_asset`, `_liability` |
| Content | `data_content_document`, `_bookmark`, `_conversation` |

**The table above is aspirational; the registry is the source of truth.** Six of
those tables have no descriptor at all (`data_health_active_energy`,
`_distance`, `data_audio_recording`, `data_financial_asset`, `_liability`,
`data_content_conversation`), and `data_activity_app_usage` does not exist — the
registry calls it `data_activity_app_session`. Derive lanes from
`crates/virtues-registry/src/ontologies.rs`, which already carries per-table
day-source config, and treat any lane member without a descriptor as work to add
one. Note a hand-written map already exists beside the registry on the frontend
(`apps/web/src/lib/wiki/ontology.ts`) — do not add a third.

**Canvas, not DOM.** Thousands of spans across lanes will kill the reconciler;
`DaylineChart` and `EventTimeline` are SVG and correct at day scale, wrong at
life scale.

**Ship the simple renderer.** The measured corpus is ~330k rows over 155 days —
a few MB, and the whole thing fits in memory. Multi-level LOD caching, pinch
debouncing and rescale-on-zoom are engineering for a scale no box has yet. Build
the endpoint (right shape, cheap) and one zoom level that works. When a box has
years, add, in this order: keep the previous LOD on screen rescaled while the
next loads (the map-tile pattern — never a skeleton mid-navigation); debounce
~120 ms after the gesture settles; LRU by (lane, level, range). Note then that
Postgres is on the same box, so an eager loading state flickers on queries that
were going to beat it.

### 13. Entities get real CRUD.

The schema is fine; the UI is missing. `wiki_places` already has `name`,
`category`, `address`, `google_place_id` — a cluster showing as
`37.7749,-122.4194` is purely an absent rename affordance, and there is no
create-a-person affordance at all.

Needed: **create / rename / delete** for person, place and org, plus editing
`nickname`, `relationship_category` and `aliases` — the human decisions 0037
built a column for and nothing has ever written.

**Merge is not here.** It is the one operation that can corrupt the record, it
depends on nothing else in this plan, and nothing else depends on it — so it
gets its own pass (Phase 10) rather than riding along with the easy verbs.

No `thing` type — `wiki_things` was dropped in 0071.

### 14. Events have summaries, not articles.

`wiki_events` carries its own summary, has `dirty_at`, and is a valid note
subject. It stays that way: **events are evidence, articles are for subjects.**
An event gets notes and a summary, never a page, a revision history or a
maintenance toggle. This keeps the article population bounded by things a person
would name.

### 15. The day is an article, and the recap is its first paragraph.

`wiki_days.autobiography` is the only prose in the six-way table with real data
— 13 of 42 days — so it is also the only part of the consolidation that has to
*move* rather than simply appear. It becomes a `subject_type='day'` article
page like any other.

But the prose itself needs to change shape, and the current prompt is not
lazy about its brevity — it is arguing:

> LENGTH FOLLOWS THE DAY … you cannot fit a day's fourteen events in four
> sentences, so only the parts that actually distinguished this day from every
> other one survive — the routine falls away, the distinctive thing remains.
> That is correct, not a loss.
> — `NARRATE_PROMPT`, [day_summary.rs:89](../virtues-core/src/api/day_summary.rs:89)

That discipline is load-bearing. This pass has a history of manufacturing
sensory detail it did not observe, and *"write more"* is the single instruction
most likely to bring it back.

> **Revised 2026-08-03** (user direction: the day is an article now, so the
> prompt should read like one, not like a "4 sentences max" summary). The
> sentence quota is gone. What replaced it as the guard against invention is
> the **evidence ceiling** — every sentence must trace to the dossier; an
> article stretched past its evidence is worse than a short one, because the
> stretching is where invention lives — plus the unchanged anti-transcription
> bar and OBSERVE-NEVER-INFER. Structure became lede + body sections expected
> where the evidence supports them, rather than "optional and usually absent."

- **The lede opens the article** — one short unheaded paragraph carrying the
  shape of the day. It sets the register for everything beneath it.
- **Sections are generated under a different rule.** Not *be brief* but **a
  section may only exist if it says something the timeline does not already
  say.** The lede's failure mode is padding; a section's is transcription —
  re-narrating the event list in paragraphs. Different guard for a different
  risk.
- **OBSERVE, NEVER INFER carries over verbatim** to every section. It is the
  most important paragraph in that prompt and the only reason the day page is
  trustworthy.
- `autobiography_sections JSONB` exists on `wiki_days` and has always been
  written as `None` — designed for this and never used. It retires with
  `autobiography`; sections are just headings in the article's markdown, so
  they diff, revert and get linked like all other prose.

**Reflections fold into the day article.** `app_pages.date` is a second,
undocumented discriminator today: `list_pages` filters `WHERE date IS NULL` to
hide "reflections", and `create_reflection` mints day-linked pages
([pages.rs:494](../virtues-core/src/api/pages.rs:494)). Left alone, a day would
carry both a reflection page and a `subject_type='day'` article, and the Pages
exclusion would need `kind='page' AND date IS NULL`.

They collapse instead of competing. **An article is editable**, so the place to
write your own account of March 3rd *is* March 3rd's article — two authors, one
page, which is already the model for every other subject. The 4 existing
reflections (measured) append into their day's article, `create_reflection`
retires, `date` stops discriminating, and the Pages exclusion stays plain
`kind='page'`.

This is a **fold, not a delete** — reflections are live in `HomeView`,
`JournalCard`, `YearPage` and the agent prompt, and hold real user writing. No
reflection content is dropped; the migration moves it.

**Already working, and worth spending:** the day prompt emits real ref-route
entity links today — `[Maya](/person/person_ab12)`, one per entity on first
mention, never invented. That is §5's forward-link mechanism running in
production. So the backlink table can be built from **13 day articles on day
one**, before a single entity article exists.

**The move is pure SQL, with three constraints.** `get_or_create` seeds `Y.Text`
from `content` when `yjs_state` is NULL, so inserting `app_pages` rows with
`content = autobiography` and `yjs_state = NULL` produces a correct CRDT on
first open — no application code needed for the content itself. But:

- **Derive the id deterministically.** `create_page` hashes a timestamp, which
  SQL cannot reproduce: use `'page_' || md5('day-article:' || d.id)` with
  `ON CONFLICT (id) DO NOTHING`.
- **Do not set `app_pages.date`.** The day-source config uses `use_date_filter:
  true` on `t.date`, so setting it makes every day article appear inside its own
  day view as "you wrote a page today" — the §1 failure, landing even before the
  descriptor split matters. The day linkage lives on `wiki_articles.subject_id`
  only.
- **Never `UPDATE app_pages.content` in a migration.** The Yjs `DocCache` is
  in-memory; a debounced save would overwrite it. Inserting new rows is safe.

Beneath the article: the existing charts, cleaned up. The article answers *what
was this day*; the charts answer *what does the record hold about it*. Neither
should try to be the other.

### 16. The room, and the empty box.

`WIKI_MODE` ([modes.ts:89](../apps/web/src/lib/sidebar/modes.ts:89)) becomes:

```
Overview · Lifeline · Narrative Identity · People · Places · Orgs · Days · History
```

**History** is a feed of `app_page_versions` where the page is an article — "the
AI rewrote *Sarah* at 03:12, diff, revert."

It is **not** free from the wrapper, and the naive reading is off by one. Version
rows are written as a **pre-edit snapshot** with `created_by: "ai"` and a preview
of "Auto-saved before AI edit"
([page_editor.rs:255](../virtues-core/src/tools/page_editor.rs:255)). So
`created_by` names the editor **about to write**, not the author of that
version's content — and because no row is written *after* an edit, the current
article text is never in the versions table at all. Either state that semantics
explicitly ("a version is the state immediately *before* the write named by
`created_by`") or snapshot after the edit as well. Phase 3 was already right that
the diff is most of the work.

**Day one is 573 people and no prose, and that has to read as complete.** Opt-in
articles mean the wiki must be worth visiting before a single model call: the
entity indexes, the days, the lifeline, and search all work on the raw record.
An entity with no article shows its records with *Write the article* as an
offer, never as an empty state or a missing-data warning. The record is the
product; the prose is an addition to it.

### 17. Overview: a front page, not a dashboard.

A dashboard monitors a system, and a life is not one. The front page of a
wikipedia is a **newspaper whose only subject is you.**

1. **The lifeline strip** — full width, one row, the whole span at maximum
   zoom-out, clickable into the console. You see the shape of the thing before
   reading a word, and it is the same endpoint at `buckets ≈ viewport width`.
2. **The masthead** — the short line that tells you what this record is before
   you read any of it: *"1,847 days recorded. 312 people,
   96 places. Since February 2026."*

   **It is computed, not written.** An earlier draft made it an article whose
   `auto_update` defaulted ON — the one exception to paradigm rule 2, and an
   exception the plan did not need: every number in that sentence is SQL. A
   deterministic line is always current, costs nothing, never goes stale, and
   keeps *nothing is written that you did not ask for* true without a footnote.
   So there is no `subject_type='overview'` and no Overview article.
   (Not "summary" — nothing is summarized, and the word is already spoken for by
   the day. Not "abstract" — it is not a precis of what sits below it.)
3. **On this day** — already shipped, and the highest-value module here: the
   only one that surfaces what you would never have thought to search for.
4. **What changed** — abbreviated History plus the open-note count. This is the
   **front door to the review loop**; without it, §8 and §9 write notes into a
   room nobody visits.
5. **Where it's thin** — *"No location data since Jul 12. 142 days have no
   article."* A record that says where it is incomplete is more trustworthy than
   one presenting itself as finished. Pieces exist (`wiki_days.data_quality`,
   `DataQualityCoverage.svelte`).

**Cut the entity index** currently at the bottom — it duplicates the sidebar and
turns the front page into a table of contents.

Register, per PR #28: serif prose in the column, sans numerals in the side
rail, charts crisp.

### 18. What this replaces, and when it is safe to drop.

Every column below is superseded by `wiki_articles` + `app_pages`. **All of them
are empty on the real box except `wiki_days.autobiography` (13 rows)** — which
is the only prose in the wiki that has to be *moved* rather than simply dropped.

| Column(s) | From | Superseded by | Data |
|---|---|---|---|
| `article`, `article_updated_at`, `article_ref_count` | `wiki_people`, `wiki_places`, `wiki_orgs` (0072) | the article page + `wiki_articles.source_ref_count` / `last_written_at` | 0 |
| `content` | `wiki_people`, `wiki_places`, `wiki_orgs` | the article page | 0 |
| `notes` | `wiki_people` | `wiki_notes` (§7) | 0 |
| `autobiography`, `autobiography_sections` | `wiki_days` | the day article (§15) | **13 / 0** |
| `content` | `wiki_narrative_identity` | the NI article (§11) | 0 rows in the table at all |

`wiki_stories.content` stays. Stories are hidden from the room, not built, and
`subject_type='story'` is reserved in `wiki_articles` for whenever they return —
dropping the only prose store for a subject nobody is working on buys nothing.

**`wiki_narrative_identity` has zero rows.** `build_narrative_identity()`
([chat.rs:539](../virtues-core/src/api/chat.rs:539)) `fetch_one`s an empty
table, so NI resolves to `""` in every prompt today. The 800-character
truncation has never truncated anything — the feature is unbuilt in practice,
not merely under-budgeted, which makes §11 a build rather than a repair.

#### The drop that passes CI and 500s on a real box

**This is the single most dangerous thing in the plan.** `sqlx::query!` is
compile-time checked, and three of those macros select exactly the columns above
— `api/wiki.rs:311` pulls `content, article, article_updated_at, … notes` from
`wiki_people`, and :440 / :540 do the same for places and orgs. There are 57
macro sites in the workspace.

Every build path sets `SQLX_OFFLINE=true` — `Makefile:72`, `ci.yml:62`,
`release-linux.yml`. Offline mode validates against the recorded JSON in
`.sqlx/`, **not the live schema**. So: drop the columns → `.sqlx/` still says
they exist → `cargo check` passes → CI passes → the release builds →
`GET /api/wiki/people/:id` 500s on a real box. Invisible until someone's box
runs it. Same shape as migration 52 and the pdfium 404.

Two mitigations, and the second is not optional:

1. Every drop migration ships in the same commit as the edited `query!` bodies
   **and** a regenerated `.sqlx/` (`cargo sqlx prepare --workspace` against a
   migrated DB).
2. **Add `cargo sqlx prepare --check --workspace` to CI.** The Postgres service
   container is already there (`ci.yml:40`). Without it, `.sqlx` drift is
   undetectable by every gate this repo has, and mitigation 1 is discipline
   rather than a guarantee.

#### Expand, migrate, contract — and the reason is rollback, not concurrency

A drop belongs in a **later migration than the one that adds its replacement,
and a later release.** An earlier draft justified this with a concurrent-reader
window that does not exist: `cli/upgrade.rs` stops the service (:280), flips the
symlink (:313), migrates (:318), then starts (:334) — the old binary is already
stopped. The conclusion survives; the reason was wrong, and the correct reason
makes the rule **one release stricter**.

The real windows are the automatic ones:

- **`flip_back` (upgrade.rs:294).** If migrations succeed but `service_start`
  fails for any unrelated reason, it flips back to the prior slot **and starts
  it** — old binary, new schema, no operator choice. Its own comment admits "the
  schema is still forward of it."
- **`virtues rollback`.** Flips the binary only; schema stays forward.
- **Boot migrations run with `set_ignore_missing(true)`**, so an older binary
  boots *cleanly* against a newer schema and then fails per-query — the worst
  failure shape there is.

> **The rule:** a column may be dropped only once **the release that `rollback`
> would land on** has already stopped reading it.

And grep more than Rust. `applets/morning_examen/manifest.toml` says, in its
agent **prompt**, "reads yesterday's autobiography from `wiki_days`" — a string
no compiler inspects, which after the drop writes erroring SQL nightly. Worse,
**authored applets live in the state root** (`/var/lib/virtues/applets`), are
per-box, and may hold SQL against any renamed table: no migration can rewrite
them and no CI run will ever see them. Name the old identifiers in the release
notes and let the `did_you_mean` hint surface rather than a bare 42703.

#### Two writers, not one

`autobiography` is not merely *read* in several places — `day_summary_eod` keeps
**writing** it nightly (`day_summary.rs:465`). If §15's move ships without
cutting the writer in the same release, migrated day articles go stale the first
night and the two copies diverge with no reconciliation. Readers to retire with
it: `api/wiki.rs` (day CRUD, day list, the `narrated` flag),
`entity_article_gen.rs:343`, `narrative_identity_gen.rs`, `cli/mod.rs:544`, and
`tools/sql_query.rs:206` — which advertises the column to the model in its
schema hint.

## Phases

0. **Hygiene** (§3, §4) — **backend done 2026-07-31; two UI affordances left.**
   Derived `ref_count` (a new uniform measure, computed per query at 11 ms, not
   materialized) is now the default sort of all three entity indexes, with a
   Mentions column; the self person is `app_user_profile.self_person_id`
   (migration 0080, backfilled under the 0037 exactly-one rule);
   reclassify-as-org moves refs, aliases, and stored routes in one transaction
   and refuses the self person; aliases are editable on people and orgs and
   normalized to lowercase on the way in, because 0037 matches on
   `aliases ? lower(surface)` and a mixed-case alias would look saved and
   resolve nothing. `nickname` / `relationship_category` were already editable.
   **Still to build: the reclassify button and the alias editor in the UI.**
   All deterministic, no model calls. **First, because every other phase sits
   behind the People index.** Merge is deliberately not here — see Phase 10.
1. **Wrapper** — **backend done 2026-07-31; chrome left.** Migration 0081 adds
   `app_pages.kind` + `wiki_articles`; `api/wiki_articles.rs` holds the single
   creation path (pool-only, so applets can call it); the Pages list and
   `search_refs` exclude articles; `MIN_REFS_TO_WRITE`/`MIN_NEW_REFS` are gone
   and the sweep is `JOIN wiki_articles WHERE auto_update` with a per-article
   threshold; `POST /api/wiki/articles/:type/:id` writes one on request and
   `PUT …/auto-update` toggles maintenance.

   **The ontology split needed a registry change, not a config edit** (as the
   audit predicted): `EmbeddingConfig` gained `embed_where`, threaded into the
   indexer *inside parentheses* — the staleness test is a disjunction, so an
   unparenthesised scope would have parsed as
   `se.id IS NULL OR (stale AND kind='page')` and indexed the other ontology's
   rows under the wrong name while reporting success. Two registry tests now
   pin it. The split also broke `attach_record_refs`, which counted ontologies
   rather than tables and so silently stripped the citation ref from every
   SQL-tool row touching `app_pages`; fixed and tested.

   **Still to build: the article chrome** (byline, *Write the article*,
   *Keep this updated*), and maintenance rewriting — which is Phase 5 work by
   construction, since a pool-only write to a CRDT-backed page is discarded on
   the next save.
2. **Links + search** (§5, §6) — **done 2026-07-31.** `get_subject_backlinks`
   derives mentions at read time (no edge table) and targets **subjects, not
   articles**, so a mention of someone with no prose still surfaces — the case
   that matters most under opt-in. `search_refs` now resolves canonical name,
   nickname **and aliases**, which 0037 built a column for and nothing had ever
   read; it needed a fourth bind, since `$1` is a LIKE pattern and containment
   on `%sarah%` matches nothing. `LocalSearchRequest.ontologies` threads room
   scoping through to the `SearchFilters` field that already existed.
3. **History** — **done 2026-07-31.** `get_article_history` + `get_history_feed`,
   a `similar`-based line diff, and the History room. The version table is **off
   by one** and reading it naively gets authorship backwards: a row is a
   snapshot taken *before* an edit, stamped with the editor about to write, and
   nothing is written after an edit — so the current text is in no version row.
   Recovered at read time by pairing `version[n]` against `version[n+1]` (or the
   live page), which fixes the feed without invalidating rows already on disk.
   Diff text comes from `yjs_snapshot`, not `content_preview` — that column
   holds a label ("Auto-saved before AI edit"), never prose.
4. **Notes** — **done 2026-07-31** (migration 0082). Both renames, with every
   artifact the rename does *not* carry: indexes, three CHECK constraints and
   the identity sequence — the CHECK widen names the constraint, so doing it
   before the rename would abort mid-upgrade. `search_embeddings` rows are
   re-pointed in the same migration, because there is no GC path and
   `source_table` is only rewritten when `doc_hash` changes, which a rename does
   not do. `/api/annotations` routes stay as-is: the installed iOS build does
   not ship with the box, so moving the path would 404 it. Cite-or-reject is a
   DB constraint, not a prompt. `wiki_people.notes` is **not** dropped here —
   the code stops reading it in this phase and a later migration drops it, per
   the rule.

   **Phase 5 remains gated**: the writer must not be built against a guess. The
   20 hand-written notes come first (see the unproven premise), and they need a
   human reading real days.
5. **The writer** — **plumbing done 2026-07-31; the prompt is deliberately not
   written.** `write_machine_notes` enforces the ≤3 cap in code (and logs when
   it binds, which is the measurement that makes "the bar is too low"
   falsifiable), skips uncited notes by name rather than failing the pass, and
   stamps `wiki_articles.dirty_at` so maintenance has a queue. Cite-or-reject is
   a DB constraint.

   **What is missing is the prompt, and that is the point.** The plan's one
   unproven premise is that an AI reading a finished day can leave notes a
   person is glad to find, and the spike — twenty notes hand-written against
   real days — is what produces both the few-shot examples and the acceptance
   test. Writing a prompt first would be building against a guess and calling it
   done. The plumbing is inert until someone writes that prompt; the applet
   ships disabled.

   Maintenance *edits* (§10) also remain unbuilt: they need the agent phase,
   because a pool-only write to a CRDT-backed page is silently discarded.
6. **NI** — **done 2026-07-31.** Budget raised to ~2k tokens with truncation at
   a paragraph (then sentence, then word) boundary — never mid-word, since a
   document that stops mid-sentence reads to the model as a thought to finish.
   `propose_narrative_identity_edit` writes a `wiki_notes` row and nothing else;
   the tool's own description tells the model it has NOT been added. The NI page
   shows an over-budget readout and carries the Notes rail where proposals wait.
7. **Entities CRUD** — **done 2026-07-31.** Create person/org by hand (they
   could only ever be discovered before), delete with `purge_subject` taking
   refs, article, notes and stored urls — `delete_place` had leaked all four
   since it was written. The self person refuses deletion. No merge.
8. **Day page** (§15) — **migration + prompt done 2026-07-31** (0083); chart UI
   pass outstanding. 13 autobiographies moved to day articles in pure SQL:
   derived ids so a replayed backup cannot double-create, `date` left NULL so a
   day article does not appear inside its own day as authorship, and no UPDATE
   so the in-memory Yjs cache cannot clobber it. `NARRATE_PROMPT` was
   re-oriented as an article brief on 2026-08-03: lede + body sections, no
   sentence quota, the evidence ceiling as the guard against invention, and
   the same section bar — a section may only exist if it says something the
   timeline does not.
9. **Lifeline** — **done 2026-07-31.** `GET /api/wiki/lifeline` returns per-lane,
   per-bucket density; lanes are **derived from the registry `domain`** rather
   than hand-written (which folds audio into communication — if that is wrong the
   fix belongs in the registry, where every consumer sees it). Bucketing is
   `width_bucket` in Postgres: one round trip per lane, ~44 ms over 169k
   messages. Canvas, one zoom level, each lane scaled to its OWN peak and
   square-rooted — lanes are not comparable in absolute terms, and one loud day
   would otherwise flatten a year.
10. **Merge**, standalone. Deliberately last and deliberately alone: it rewrites
   rows in a 131k-row table under a `NULLS NOT DISTINCT` unique index, unions
   `emails`/`phones`/`handles`/`aliases`, re-points articles, notes, pins and
   notebook items, and is effectively irreversible — so the loser is
   soft-deleted (marked merged-into), not dropped. It needs its own design pass;
   nothing else in the plan waits on it.

Phases 0–4 are additive and independently shippable. **Phase 5 is the first that
lets an AI touch the record** and should not start until 4 is in daily use.

**Before any of it: add `cargo sqlx prepare --check --workspace` to CI** (§18).
It is one job against a Postgres service container that already exists, and
without it the most dangerous class of failure in this plan is undetectable.

**Drops trail their phase by one** (§18). Each phase adds its replacement and
leaves the old column in place; the migration that drops it ships in the next
one — after the release `rollback` would land on has stopped reading it.

**This is 8–11 migrations, not a handful.** Claim every number with
`make migration NAME=` *before* writing SQL, and re-run the duplicate check from
CLAUDE.md before any PR carrying more than one — this plan puts three or four in
flight across phases that will interleave with other agents on `wave`. Three
ordering hazards are not obvious from the phase list:

- the day **move** must precede the `autobiography` **drop**, and by a release,
  not merely a number — same file, adjacent numbers, easy to collapse under
  review pressure;
- the `wiki_notes` rename must precede any code naming `wiki_notes`, and inside
  that migration the CHECK widen must follow the constraint *rename*;
- the drop of `article`/`content`/`notes` must never be batched with anything
  else — its blast radius is three compile-time-checked queries and its symptom
  is a green build.

Idempotency needs no special effort: sqlx wraps each migration in a transaction,
so partial application is impossible, and `_sqlx_migrations` rides inside the
`pg_dump`. The one exception is the day move — give it a deterministic id and
`ON CONFLICT DO NOTHING`, so a restore replayed across the boundary cannot
double-create day articles.

Phase 0 is the one that changes how the wiki *feels* on day one, and it needs no
model, no migration risk, and no new paradigm — only work nobody has done yet.

## The one unproven premise

Everything here is plumbing now confirmed to exist — the CRDT write path, the
seeding, the versions table, the freshness check, the name navigator. **One
thing has never been tested: that an AI reading a finished day can leave notes a
person is glad to find.** The last adjacent attempt was deleted in 0061.

So before Phase 1: take 3–5 dense days off the boxcopy and **hand-write twenty
notes as they ought to read.** Human gold standard first, so there is a
target before anything tries to hit it. Judge each against the covenant — does it
cite, does the article want it, would you click Accept. What survives becomes
both the few-shot examples and the acceptance test for Phase 5.

If twenty hand-written notes against real days read as noise, that is not a
Phase 5 problem: the writer should not be built, and the plan collapses to
articles + links + search + history + CRUD + lifeline — all of which remain worth
doing. Better to learn that from a day of writing than after building the rail to
display them.

*(`dev-real` has no model gateway on :9002, so the gold standard is hand-authored
anyway; checking whether a model can reach it comes second and needs
`virtues-api` up.)*
