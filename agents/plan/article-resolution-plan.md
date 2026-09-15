# Article resolution

Status: **DRAFTED 2026-09-15**, unbuilt. Supersedes `wiki-rungs-plan.md`
(deleted, never committed), which modelled the wiki as a compression
pipeline folding day summaries upward. That framing was wrong: the wiki is
a set of **resolution processes**, and writing an article is one of them.
Extends [wiki-plan.md](./wiki-plan.md) (the article/page mechanism) and
adopts [attention-plan.md](./attention-plan.md)'s revision doctrine. Leaves
the open-question queue to [narrative-resolution-plan.md](./narrative-resolution-plan.md).

Citations are file:line as of 2026-09-15, checked by three review passes
(devil's advocate, code audit, simplify-and-push).

**Death condition:** delete when article resolution runs for years, stories
and entities. What survives is `agents/record/article-resolution.md` and a
manual page.

---

## 1. The resolutions

The wiki is produced by resolution processes. Each takes the record and
resolves it into something with a name.

| resolution | what it resolves | state |
|---|---|---|
| **entity** | records → people, places, orgs (`wiki_refs`) | built, deterministic, no model ([entity_resolution/mod.rs:62](../../virtues-core/src/entity_resolution/mod.rs:62)) |
| **event** | a day's rows → a gapless event timeline | built, nightly |
| **topic** | an event → 2–4 topical tags | built, free, rides inside event resolution ([day_summary.rs:33](../../virtues-core/src/api/day_summary.rs:33)) |
| **day** | a day's dossier → the day's article | built, nightly |
| **article** | a subject + the record → that subject's article, maintained over time | **this plan** |
| **semantic** | a remark here → the thing it refers to there | not scoped; needs the structure settled first |

**Every subject has an article**, and article resolution writes and
maintains all of them: day, year, chapter, life, person, place, org, story.
Day resolution is the special case that produces a *first draft* of one
subject kind as a by-product of event resolution. Everything above and
beside it is the general case.

**Topics stay inside the event.** A topic is a tag on an event, not a
subject with a page — a topic page would compete with a story page, and
stories are the person's own version of exactly that ("Piano & Composition"
is a topic they decided matters). Topics earn their keep as a **retrieval
affordance**: the article resolver can ask for every day tagged `piano`.
Nothing more is built for them.

---

## 2. Article resolution

**One process, one toolset, one brief per subject kind.**

An article is like a page on wikipedia with a contributor who watches it.
When enough new information about the subject has accumulated, the article
is queued; the editor reads what is there, researches the record, thinks,
and edits the living document — adding the salient, true, high-fidelity
things at that level, and nothing else.

### 2.1 Draft is one-shot; revision is agentic

The split that keeps this affordable, and the one the day already
demonstrates:

| | how | why |
|---|---|---|
| **first draft** | one call, everything in the prompt | there is no existing text to respect and nothing to go looking for. The day narrator is this, and stays exactly as it is |
| **revision** | an agent loop with tools | it must read the current article, work out what is genuinely new, find the evidence, decide whether it changes anything, and make a surgical edit. None of that is knowable in advance |

A story like "Piano & Composition" has **no rung beneath it** — no day
list, no ref set. The only way to write it is to search the record. That is
the proof that revision has to be agentic, and once it is, years and
entities are simply other subjects for the same loop.

### 2.2 The loop already exists

The applet runner has two phases: a subprocess, then — if the applet
declares an `agent` prompt — an **in-process agent loop holding
`YjsState`** ([applet_runner/mod.rs:452](../../virtues-core/src/applet_runner/mod.rs:452),
[agent/applet_runner.rs:121](../../virtues-core/src/agent/applet_runner.rs:121)).
**No applet uses it.** Article resolution is its first caller, which is
what wiki-plan §10 predicted it was for.

The **wiki editor toolset** is one set for every subject, and every tool in
it ships today ([tools/executor.rs](../../virtues-core/src/tools/executor.rs)):

| tool | use |
|---|---|
| `semantic_search` | find the evidence, scoped to the subject |
| `sql_query` | counts, spans, "which days carry this topic" |
| `get_page_content` | read this article and its neighbours |
| `edit_page` | the surgical edit, through the CRDT |
| `think` | deliberate before editing |

**This reverses the earlier design of the write path, for a stated
reason.** The deleted plan had the model return a whole article for the
server to diff, because a batch job cannot retry a failed find/replace. An
agent loop *can* — it sees the failure and tries again, which is how coding
agents edit files. So the editor uses `edit_page` as chat does, and the
whole-article-plus-server-diff machinery is dropped. `page_editor` already
reads via `get_page_content`, snapshots a version, and edits via
`apply_text_edit` — the three things a maintenance pass needs.

### 2.3 What the agent is told

System prompt = **the constitution** (shared) + **the brief** (per subject
kind). The constitution holds the rules that never vary:

- a private wiki about one person, written from the record, read only by them;
- **observe, never infer** — no emotion, motive, verdict or invented detail;
- every sentence traces to evidence; length follows the evidence, not a quota;
- link subjects by exact id only; never invent a link;
- **their lines outrank the record** — placed, never paraphrased, never
  re-explained; **their removals are final** — never restored;
- edit surgically; never reorder sections; never rewrite what is still true;
- the only channels are the edit and a cited note; never write the graph,
  never create a subject unasked;
- **the active `wiki_rules` ride here**, exactly as they ride in chat
  ([chat.rs:713](../../virtues-core/src/api/chat.rs:713)) — without them the
  year article names what chat is told never to raise.

The turn's opening message carries: the subject and its authored fields,
`THEIRS` (§2.6), `REMOVED`, what has changed since the last edition, and the
budget (how many tool calls it may spend).

The briefs are prose and live in `agents/build/wiki-briefs.md`, included by
the Rust constants at compile time so the doc that is reviewed and the
prompt the box runs cannot drift.

### 2.4 When: eligibility, then drift, then interval

Three gates, in order.

**Eligible** — is this subject allowed an article at all?

| subject kind | eligible when |
|---|---|
| day, year, chapter, life, story | always; these are the person's own structure |
| person, place, org | the person has touched it (opened, named, bonded, noted, linked) **or** it is in the **top 20 by recency-weighted refs** |

Recency weight is one SQL expression over `wiki_refs.occurred_at`:
`sum(exp(-ln(2) * age_days / 30))` — an exponential half-life of 30 days,
so last week outweighs week twelve without a hand-made curve. **A budget,
not a threshold**: "the top 20" is bounded on any box, where a threshold
matches five on one box and five hundred on another. Significance is
user-sourced first and measured second, which is the standing doctrine.

Per article, a **maintenance** setting overrides the budget:
`always | auto | never`, default `auto`, set on the page. This generalizes
the `auto_update` boolean and is the person's steering wheel.

**Due** — the fingerprint of the subject's evidence has drifted. The
existing helper generalizes ([day_summary.rs:533](../../virtues-core/src/api/day_summary.rs:533)):
a sha256 over labelled counts and max-timestamps of what the subject rests
on. This is the attention plan's doctrine — the trigger is drift, not a
counter — and it means an article with nothing new is never touched.

**Ready** — `last_written_at` is older than the minimum interval: 7 days
for the current year and open stories, 30 days for everything else. Plus
`update_requested_at` for "update now", settable in bulk from the CLI so a
changed constitution can re-edit a whole rung.

Columns on `wiki_articles` (one migration; replaces `dirty_at`,
`refresh_after_new_refs`, `source_ref_count`, `auto_update`):

```sql
maintenance          text NOT NULL DEFAULT 'auto'   -- always | auto | never
input_fingerprint    text
update_requested_at  timestamptz
last_human_edit_at   timestamptz
machine_text         text          -- exactly what the editor last left (§2.6)
theirs               jsonb NOT NULL DEFAULT '[]'
removed              jsonb NOT NULL DEFAULT '[]'
```

**Convergence**, from the attention plan's preconditions: a run that
changes nothing still stores the new fingerprint (else a broken article is
an hourly oscillator); a ceiling of **3 articles per run**, hourly, prose on
the quiet hour; a failed run is logged and the article's colophon says the
record could not update it.

### 2.5 The edit

Inside the agent loop, per article:

1. Skip if `last_human_edit_at` is inside six hours — safe under the CRDT,
   still jarring under a cursor.
2. If the live text differs from `machine_text`, cut a `user` version first
   so history shows the person's edit before the machine's. **No wiki
   component cuts versions today** — only the generic page view's autosave
   does — so this is new code.
3. The agent researches and edits via `edit_page`, which snapshots a
   version and applies through the CRDT.
4. **Checks after the loop**: every link resolves to a real id; the lede is
   still a paragraph; **every sentence in `theirs` survives verbatim** and
   **nothing in `removed` has reappeared**. On failure, revert to the
   pre-run version and log — the agent had its retries inside the loop.
5. Cut the closing version with `created_by='ai'`, the agent's **edit
   summary** (the *why*: "added the spring recital, three new lessons"), and
   a **mechanical line that cannot lie** (+2 paragraphs, −1 sentence, 4 new
   links). Store `machine_text` and the fingerprint **after** the save
   debounce has flushed ([yjs.rs:196](../../virtues-core/src/server/yjs.rs:196)),
   so a restart cannot leave a version whose text is not the page.

**Revert** restores a chosen version as a new version. Versions are capped
at 50 per page ([pages.rs:679](../../virtues-core/src/api/pages.rs:679));
the cap must never prune a `user` version.

### 2.6 Provenance, and why ownership never flips

Nobody wants to maintain their own record. They want to leave marginalia and
touch a sentence, without taking ownership of the page. So the claim flip
goes ([yjs.rs:301](../../virtues-core/src/server/yjs.rs:301) is repurposed
to stamp `last_human_edit_at`), and what protects their words is that the
editor can see them.

Computed from **the machine's own last output**, which the server knows
exactly: `diff(live_text, machine_text)` is precisely what the person did
since. Inserted sentences join `theirs`; deleted sentences join `removed`;
both are pruned against the live text so they cannot grow without bound.

(The earlier draft derived this from version history. The audit killed it:
the wire values are `user`/`ai`/`auto`, not `human`
([versions.ts:21](../../apps/web/src/lib/yjs/versions.ts:21)); no wiki
component cuts a version; a row's author names the *next* edit; and the
50-cap prunes the oldest human sentence first.)

The invariant protects bytes. **Placement is the constitution's job** —
never re-explain, never contradict, never move their sentence — and the
person's answer to a bad placement is revert. A standing "don't write that
again" is a `wiki_rules` row scoped by `subject_type`/`subject_id` to the
page, which is why those columns are kept.

### 2.7 Notes

An accepted note changes the fingerprint, so the next run places it.
`write_machine_notes` ([wiki_notes.rs:224](../../virtues-core/src/api/wiki_notes.rs:224))
gains its first non-test caller: what the agent found but may not assert
becomes a cited note. The chat tool that proposes a narrative-identity
observation ([executor.rs:599](../../virtues-core/src/tools/executor.rs:599))
generalizes to `propose_note(subject, body, refs)`; only the person accepts.
An unresolved `author='ai'` note **is** an open question of the "meaning"
kind, so the resolution queue reads these rather than building a second one.

---

## 3. The subjects

What each brief is about, and what each subject still needs.

| subject | the brief, in a line | missing today |
|---|---|---|
| **day** | the article of the day: lede, then threads the timeline does not already say | nothing — released and tuned, **not refactored** |
| **year** | the article of the year, as wikipedia gives "2026": lede, then the threads that ran through it. Researched, not folded — the days are where it starts, not where it ends | the whole rung (§3.1) |
| **chapter** | the person's era; their title, changepoint and summary are the spine | `PUT`/`DELETE`, a page, a backlinks arm |
| **story** | a catch-all subject the person names — "Piano & Composition", "Books Written", "How I learned to pray". A title and nothing else; the record is researched for it | reshape + a room (§3.2) |
| **person / place / org** | the existing brief | eligibility and the budget |
| **life** | first person, theirs. **Never machine-edited** — the only channel is a note | the self row (§3.3) |

### 3.1 Years

Every year from birth has a page; lite ones are expected. **Materialized
lazily** — the partition is computed by range like the day's holes, and a
row and page appear on first visit, edit or draft. Not forty empty rows at
seeding, which would enter the search index as blank articles and write
what nobody asked for on first boot. Sections are **threads, never
months**; there is no `wiki_months`.

`wiki_years` has zero code references, so it is reshaped freely: add
`summary` (theirs, verbatim); stop reading `description`, `content`,
`cover_image`. Two endpoints, not four: `GET /api/wiki/year/:year` and a
`PUT` for `title`/`summary`; the index is derived; drafting is the generic
subject path with `year` allowed. `resolve_id` already accepts the prefix
([wiki.rs:1320](../../virtues-core/src/api/wiki.rs:1320)) and the route
exists; the page is dead behind `// TODO: Implement year API`
([WikiContent.svelte:111](../../apps/web/src/lib/components/views/WikiContent.svelte:111)).
Backlinks gain `year` and `day` arms.

Three page states, designed rather than left to chance: **before the
record** — the chapter dateline and their `summary`, with an invitation, no
draft offered; **thin** — dateline, days on record with first and last, the
days marked narrated / recorded / unknown; **dense** — the article first,
the rest as apparatus. The day page gains the breadcrumb back ("2026 · Out
on my own").

### 3.2 Stories

A story is a **catch-all subject**, not a span (correction, 2026-09-15). It
may have dates; it need not. For now they are **user-created only** — the
person makes the page and titles it; article resolution does the rest.
Reshape `wiki_stories`: keep `title`, `start_date`, `end_date`; add
`summary`; stop reading `subtitle`, `content`, `themes`, `metadata`,
`cover_image`, `sort_order`. A room, an index, and the dead `'stories'`
union member becomes live.

### 3.3 The life, and the self

The identity article **becomes the self person's article**: subject
`(person, self_person_id)`, retiring the `narrative_identity` type and its
singleton index. Two facts shape the order: `wiki_narrative_identity` the
*table* is dead cruft and was never the page (the page is the article keyed
`nar_identity_001`, [narrative_draft.rs:75](../../virtues-core/src/api/narrative_draft.rs:75));
and **no code creates a self person row** — `self_person_id` is optional and
the profile PUT only stores an id it is handed. So: the interview ensures a
self row; the migration re-points where the id is set and repairs later
where it is not; chat's loader reads by self id with a fallback; then the
type and index go. The page carries the apparatus — lifeline crop, chapters,
years, the record grid — and name and birth date are editable there, since
the year partition depends on the birth date.

---

## 4. Cruft

**Drops are a one-way door**: a dropped column means the previous binary
cannot boot, so `virtues upgrade`'s rollback dies for that release.
Therefore slice 1 **stops reading** and drops nothing; one terminal
migration drops, after boxes have rolled.

**A live bug to fix first, on its own.** `save_day_article` writes through
the pool only `WHERE yjs_state IS NULL` ([day_summary.rs:776](../../virtues-core/src/api/day_summary.rs:776)),
and nothing ever sets `yjs_state` back to NULL — so once a day page has been
opened in the editor, **narration can never land on it again**. That is on
boxes now.

| item | action |
|---|---|
| `wiki_narrative_identity` (table, zero non-comment refs) | drop |
| `seen_count`/`first_seen`/`last_seen` — never written, yet fed to the article model as "Interactions on record: 0" ([entity_article_gen.rs:241](../../virtues-core/src/api/entity_article_gen.rs:241)) and listed in the model-facing catalog | stop reading (prompt, `sql_query` catalog, TS) → drop |
| `article`/`article_updated_at`, `wiki_people.notes`, `google_place_id` | stop reading → drop |
| `wiki_days.epigraph`/`data_quality`/`cover_image`/`snapshot`/`last_edited_by` — **a live writer exists**: `update_day` behind `PUT /api/wiki/day/:date` ([wiki.rs:1110](../../virtues-core/src/api/wiki.rs:1110)) | delete the route, handler and CLI fields; `on_this_day` returns the lede → drop |
| `wiki_days.readiness_*` (no writer at all) | stop reading → drop |
| `wiki_articles.dirty_at`/`refresh_after_new_refs`/`source_ref_count` | replaced by §2.4 → drop |
| `refresh_due_entity_articles`, which returns `Ok(0)` by construction, and its disabled applet | delete, replaced |
| `wiki_events.agent_action` | **keep** — the nightly sleep resolver writes it ([sleep.rs:156](../../virtues-core/src/dayline/sleep.rs:156)) |
| `wiki_events.user_hidden` | **keep**, wire in the attention plan's order or "hide" ships as "destroy" |
| `wiki_rules.subject_type`/`subject_id` | **keep** — the per-page rule hook (§2.6) |
| **four** disagreeing subject-type CHECK lists (`wiki_articles`, `wiki_notes`, `wiki_refs`, `wiki_rules`) | align to one |
| `href="/wiki/{id}"` at **13 sites** — matches no route, opens a new chat | fix |
| ~10 entity-page sections that can never render (`converters.ts` hardcodes `[]`) | delete |
| `WikiRailAI`, `WikiRailHistory` (mock data), `WikiPage`, `WikiRightRail`, `WikiRailContents`, `WikiCitations`, `WikiLinkedPages`, `WikiRelatedPages` | delete |
| `PUT/DELETE /api/wiki/citations/:id` client fns with no route; `updateDay` with no callers; the `'stories'` dead union member; `location.reload()` after writes; `prompt()`/`confirm()`; the disabled "Page settings" button; legacy `page.content` Notes on Place and Org | delete / fix |
| docs: wiki-plan §10 (the one-pen rule, find/replace maintenance), §18, §11; narrative-identity.md and onboarding.md on "the table was deleted"; the schema-audit record on stories; 0015's `parse_entity_id` | correct; add README rows |

---

## 5. What this plan overrules

- **wiki-plan §10, the one-pen rule (2026-08-03)** — "an article is either
  the record's or yours, never both." Overruled: ownership never flips.
  Its reason (interleaved authorship is hard to follow) is answered by the
  edit summary, the mechanical diff line, unprunable `user` versions, and a
  machine forbidden to re-explain or move the person's sentences.
- **wiki-plan §17's "articles are opt-in"** for the time rungs — the day
  already breaks it nightly. Honestly stated: **the time rungs are a
  standing request** (asking for the wiki is asking for its timeline);
  subjects and stories stay opt-in via `maintenance`.
- **The deleted `wiki-rungs-plan.md`** in full: the fold-from-ledes model,
  the staged jsonb list, the whole-article server diff, blame from version
  history, eager year seeding, stories as authored spans.

---

## 6. Build order

| # | slice | gate |
|---|---|---|
| 0 | **the narration bug** (§4) — one commit, ahead of everything | a day whose page has been opened re-narrates |
| 1 | **stop reading** — prompt, catalog, TS; delete `update_day`; fix 13 links; delete dead components. No migration | no zero counts reach the model; every wiki link resolves; the previous binary still boots |
| 2 | **article resolution** — one migration; the constitution and the entity brief in `agents/build/wiki-briefs.md`; the `wiki_editor` applet with an `agent` prompt; eligibility, drift, interval; `theirs`/`removed`; version cutting, summaries, revert; `maintenance` on the page | an entity whose evidence drifted is revised as a small diff with a summary; a sentence the person wrote survives and one they removed stays gone; a failed check reverts |
| 3 | **years** — reshape, API, the year brief, three page states, day↔year links | a real year page reads well enough to keep |
| 4 | **stories** — reshape, room, the story brief | "Piano & Composition" with no dates produces a researched article |
| 5–6 | chapters; the life and the self page | each its own PR |
| last | **the drop migration** | boxes on the previous release have rolled |

Each slice is one PR to `staging`.

---

## 7. Open questions

1. **The register** — second person (the day's voice, "you") or third
   person with their name (a biography's voice, which is what wikipedia's
   year pages are when the subject is a person). Decided on two specimens
   of one real dense year on the dev box, eyes only, never committed. This
   is the one open question that changes what every page reads like.
2. **The per-run budget** — 3 articles hourly is a guess; it is the whole
   cost control and wants a month of evidence.
3. **Top 20 entities** — the right N, and whether places and orgs deserve a
   different budget than people.
4. **Does the day's *revision* join article resolution?** Its first draft
   stays nightly and one-shot. Re-narration is the attention plan's
   `refs_fingerprint` work; the natural answer is that a day due for
   revision is just another article, but that is that plan's call.
5. **Semantic resolution** — deliberately unscoped until the structure
   above is settled.
