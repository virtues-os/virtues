# Resolution — an audit across six areas

**Status:** Findings only, 2026-08-17. Nothing here is fixed. Six read-only
agents swept entity resolution, the narrative identity stack, day/event
summarization, retrieval, the wiki schema, and the docs. Line numbers were
current at HEAD `c8889713` plus this session's uncommitted work; verify before
acting.

The sweep was prompted by finding that `wiki_rules` had been written since
migration 0101 and read by nothing. That turned out to be one case of **a
wider pattern**, and that pattern is the most important finding in this
document.

---

## 1. Data written and never read, or read and never written

Roughly twenty-five columns and five whole tables. Grouped by how they fail.

### 1a. Read but never written — these produce false facts

This is the worst class, because the value is present, **wrong, and treated as fact**.

| column | read at | reality |
|---|---|---|
| `wiki_places.visit_count`, `first_visit`, `last_visit` | `api/wiki.rs:557` (UI), `api/entity_article_gen.rs:247-263` (**fed to the article LLM**), `tools/sql_query.rs:186,192` (offered to the agent) | no writer anywhere |
| `wiki_people/orgs.interaction_count`, `first_interaction`, `last_interaction` | same | no writer anywhere |
| `wiki_days.morning_baseline` | `api/wiki.rs:1147` → UI | no writer; 83/127 rows hold stale data from a removed job |
| `wiki_days.battery_curve`, `readiness_score`, `readiness_details` | `api/wiki.rs:1147-48`, `converters.ts:224` | no writer; UI renders permanently-null fields |
| `wiki_days.snapshot` | `api/wiki.rs:1148` | no producer |

`entity_article_gen.rs` passing `- Visits on record: 0` into a prompt is the
clearest example: the model is being told, as fact, something no code has ever
computed, and it will write prose around it.

### 1b. Written but never read

`wiki_rules.subject_type`/`subject_id` · `wiki_refs.confidence`, `resolved_by`,
`metadata` · `wiki_orgs.metadata.original_name` · `wiki_days.segmented_at`,
`illustration` · `wiki_articles.last_written_at` ·
`wiki_narrative_identity.document`, `drafted_at`, `active` ·
`wiki_narrative_interview.word_count` · `wiki_events.local_novelty_z`,
`lof_raw`, `confidence`, `hrv_z` · `search_embeddings.text_hash` ·
`search_index_meta.fingerprint` · `wiki_people/orgs/places.article_ref_count`

Two are worth calling out:

- **`wiki_narrative_identity.document`** is the prose a person is told to "read
  and correct." It is written once and returned only in a transient POST
  response. **No view, endpoint or component reads it.** Once the onboarding
  screen closes it is unreachable.
- **`wiki_events.local_novelty_z` / `lof_raw`** are computed nightly for 629 of
  744 events and read by nobody, while the visible "Most Novel" badge uses the
  worse statistic (§2).

### 1c. Five tables with zero rows and zero writers, still offered to the agent

`wiki_acts`, `wiki_chapters`, `wiki_stories`, `wiki_telos`, `wiki_years`.

`tools/sql_query.rs:211-251` offers all five to the SQL agent with join hints.
Ask the box "what act of my life am I in" and it will query an empty table and
answer nothing, forever. `wiki_days.act_id`/`chapter_id` are 0/127 populated.

### Why this keeps happening

Nothing fails when a column stops being written. There is no test, no
constraint, and no report that says "this column has no producer." The schema is
the union of every idea anyone has had, and reading it gives no signal about
which ideas are live. **A schema-coverage check in CI — every column, does any
code write it, does any code read it — is the single highest-leverage fix in
this document**, because it converts a recurring silent failure into a build
error.

---

## 2. Rule violations: model output fed back in as evidence

The `OBSERVE, NEVER INFER` rule holds at the graph boundary; we verified it. Every
`INSERT INTO wiki_refs` is deterministic, retrieval never writes edges, and
`api/wiki_notes.rs:19` explicitly forbids the writer touching refs. **The
violations are all in the day pipeline, where model prose is fed back in as
fact.**

1. **The anti-correlation failure `attention-plan.md` describes is fully live.**
   `dayline/embedding_ops.rs:79` embeds `summary.trim()`;
   `dayline/novelty.rs:96-99` embeds `e.event_summary` verbatim;
   `day_summary.rs:424-430` picks `max(novelty_z)` and `:448` injects "(the most
   unusual beat of the day)" back into the narration prompt. **Model output is
   used to rank model output, and that ranking then steers the article.**
   `EventTimeline.svelte:167` still renders the badge.
2. **`day_summary.rs:1371-1400`** — `data_audio_session.content` (model-stitched
   summaries) enters the dossier as evidence with no provenance tag, while
   `SEGMENT_PROMPT:39` tells the detective audio content "tells you what a
   stretch actually was." `attention-plan.md` lists this field as one that must not be used as
   evidence.
3. **`day_summary.rs:1578`** — `recent_event_case_file` feeds 14 days of prior
   model prose into the narration prompt, while `NARRATE_PROMPT` claims "every
   sentence must trace to the dossier." The narrate call never sees a dossier.

The two largest sources of inferred content — audio content and the case
file — are exactly the two that the prompt gives no caveat for.

---

## 3. Correctness and privacy

**Deleted content stays searchable and citable.** `search/indexer.rs:390-403`:
when a record's text becomes empty, the branch writes a placeholder and
`continue`s, so it never enters `docs` and the stale-chunk deletion at `:452-483`
never runs. Chunk 0 keeps its old `content` and `search_vectors` row; chunks
1..N are untouched. **Delete the body of a message and retrieval still returns
and cites it.** Severity: high, because this breaks a privacy claim the
product makes.

**Merchant names collapse into one org.** `entity_resolution/places.rs:363` —
`normalize_merchant_name`'s suffix list contains `"*"` and `" CO"`, truncating at
the *first* match: `"SQ *BLUE BOTTLE"` → `"Sq"`. Every Square/Toast/PayPal
merchant collapses into one shared `wiki_orgs` row that then accumulates
unrelated refs. This is silent, irreversible graph corruption from a
*deterministic* writer, and `original_name`, the only record of the pre-normalization string,
is write-only (§1b), so it cannot be undone.

**Retrieval recall is silently capped at 40.** `search/query.rs:507,540` sets
`CANDIDATE_POOL = 200`, but pgvector's `hnsw.ef_search` defaults to 40 and
nothing in the repo ever sets it — measured on the live DB: `LIMIT 200` returns
40 rows; `SET hnsw.ef_search=250` returns 200. The dense arm contributes 40
candidates, the lexical arm 200, and fusion never accounts for the 5×
asymmetry. The fix is one `SET LOCAL`. The cap also brings back the
starvation bug that `ir-notes.md` §2.2 claims was fixed.

**The score-scale mismatch is still present**, in a different place. `query.rs:758-783`: candidates
with empty content are excluded from reranking but stay in the result set with
their fused z-score, while reranked peers are overwritten with raw reranker
output; `:779` sorts the mixed set. It affects only 3 rows today, and it is
not fixed.

**Other correctness:** DST-skipped local midnight silently falls through to a
UTC day (`day_summary.rs:126-158`); visits spanning midnight are invisible to
the second day, a structural cause of morning "Unknown" blocks (`:1197-1204`);
inclusive upper bounds double-count a visit at exactly midnight (`:1203,1292,2032`);
`n_docs` undercounts permanently and is the *N* in the IDF
(`indexer.rs:502-509`); nondeterministic `LIMIT 1` without `ORDER BY` picks a
person when two share an email (`people.rs:585,782`).

---

## 4. Cost

**`narrate_day` has no idempotence guard.** No fingerprint, no "prose already
exists" check, while the catch-up queue selects on `narrated_at IS NULL` — and
125 of 127 days on this box have prose but no `narrated_at`. Every such day
inside the 14-day horizon is **re-narrated once per hour, forever**, each a
paid Chat call. `segment_day_events:277-286` has the guard; narrate does not.

**~58k queries to rescore a box.** `dayline/annotate.rs:167-193` issues one
`SELECT 1 … LIMIT 1` per registered ontology (27 of them) per event, plus two
more — ~29 round-trips × ~16 events × 127 days, unbatched, with no LIMIT on the
outer date list.

---

## 5. Regressions from this session

The session that wrote this audit introduced these, and they would have
shipped.

1. **Rule capture was removed at the moment enforcement landed.** Moving `rules`
   to `stage: 'queue'` means no question ever asks for a rule
   (`questions.ts:286`), so `build_rules` reads a table that stays empty.
   `narrative_draft.rs:52` still tells the model rules come from "usually the
   last answer" — now `belief`.
2. **The draft screen wipes rules on a second visit.** `saveNarrativeRules` is a
   global DELETE-and-reinsert (`narrative_draft.rs:318`) and `DraftReview` never
   loads existing rules. The new URL routing made `draft` re-enterable from the
   strip, widening a bug that already existed.
3. **The reveal overwrites the person's own words.** `RevealSection` triggers
   `applet_narrative_identity_draft`, which overwrites
   `wiki_narrative_identity.content` — distilled from the interview minutes
   earlier — with a paragraph guessed from observed data. Three writers own that
   column, and nothing decides which write wins.
4. **The census module declaration was committed without its file.** `pub mod census;` was added to `api/mod.rs`, a hot
   shared file, while `api/census.rs` stayed untracked. It rode into another
   agent's commit on a file-level pathspec and broke CI, which they spent three
   runs backing out (`92c493b4`). `cargo check` stayed green throughout because
   it compiles the working tree. The declaration, the route and the file must
   land together — and `client.ts` plus `RevealSection.svelte` currently call an
   endpoint that no longer exists.

---

## 6. Docs

- **`0099` does not exist.** It was reverted; the wiki-refs migration is
  **`0105`**. `attention-plan.md` cites 0099 five times,
  `narrative-resolution-plan.md` twice.
- **The name collision is confirmed.** `onboarding.md`, `onboarding-plan.md` and
  `onboarding-paradigm.md` are 100% box setup — zero hits for
  `interview|letter|reveal`. The app's `/onboarding` is the narrative flow.
  Rename the docs to `setup-*.md`.
- **Status headers are wrong in both directions:** `wiki-plan.md` (since recorded as [wiki-consolidation.md](wiki-consolidation.md)) says "unbuilt" (0081–0083
  shipped); `references.md` says "no implementation yet" (four components exist).
- **`the-day.md`'s whole Autobiography layer** rests on `wiki_days.autobiography`,
  dropped today in 0106.
- **`README.md` claims "every doc is listed here"**; nine are not, including the
  largest doc in the repo.
- **`write_machine_notes` still has zero production callers** — the note contract
  that `wiki-plan.md` builds on (now [wiki-notes-plan.md](../plan/wiki-notes-plan.md)) has no producer, and the uniqueness constraint it
  requires does not exist.
- **Gap:** the six-step post-setup onboarding flow has no doc at all.

### Vocabulary collisions

| concept | names in flight |
|---|---|
| the citation edge | `wiki_entity_refs` / `wiki_refs` |
| a user-authored constraint | "standing orders" / "rules" |
| the identity artifacts | "portrait" / "narrative identity" / `document` / `content` |
| a day's prose | "autobiography" / "day article" / `wiki_day_prose` |
| **resolution** | entity resolution / Narrative Resolution / `wiki_notes.resolution` / `resolved_by` |
| the unit of a day | "story" / "event" / "day" |

"Resolution" is already the most overloaded word in the schema, in four
unrelated senses. Using it as the umbrella name should be an explicit
decision.

---

## 7. Suggested order

**Urgent fixes first (small, and each fixes something actively wrong):**

1. `hnsw.ef_search` — one `SET LOCAL`, recovers 5× dense recall.
2. Empty-text records must delete their stale chunks — privacy.
3. `narrate_day` idempotence guard — stops paying hourly for the same 125 days.
4. Land census as one commit (file + declaration + route), or delete the client calls.
5. Fix the three session regressions in §5.

**Then the systemic fix:**

6. A schema-coverage check in CI. Every column: written by anything? read by
   anything? This is what turns §1 from a recurring silent failure into a build
   error, and it is worth more than fixing any individual column on the list.

**Then the observe-never-infer violations:**

7. Stop embedding model summaries for novelty; stop feeding audio content and
   the case file into prompts as evidence.

**Then cleanup:** the dead columns, the five empty tables, the doc renames and
status corrections.
