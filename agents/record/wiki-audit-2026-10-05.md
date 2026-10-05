# State of the wiki, 2026-10-05

Four read-only sweeps on 2026-10-05: the live data on a real box, the backend
pipeline, the web UI, and the docs against the code. The three findings most
likely to be wrong were re-checked by hand. Line numbers are as of `wave` that
day. Counts are from one box and describe failure classes, not a person.

## Verdict

The bones are right and the day is healthy. Everything around the day is thin,
and the machinery that should thicken it is quietly broken.

- **Days work.** Every recent day has a full article (mean 2.5k characters),
  nothing is orphaned, every link resolves, and the logs show no wiki errors.
- **The rest of the wiki barely exists.** Day pages cover the last 3.5 months
  of an 8.8-year record. 21 people have articles; no place, organization or
  year does. The one story and the chapters are stubs that have not moved
  since September.
- **The editor that should grow it runs without its rules and starves.** The
  revision agent never sees the constitution, and day articles block its
  queue.
- **Chat could not reach any of it** until 2026-10-02, and the fix is not on a
  box yet.

## The good

- **One subject registry** (`api/subjects.rs`) drives briefs, routes, tables
  and link checks. Adding a kind is a row.
- **One constitution plus a brief per kind**, compiled in
  (`wiki_editor.rs:17-24`). Every brief is used.
- **Ownership is enforced by the server, not the prompt.** A machine edit that
  drops a sentence the person wrote is refused (`wiki_editor.rs:215-320`), and
  this is well tested.
- **Links are checked against the name behind the id**, not just existence
  (`wiki_editor.rs:494`, `entity_article_gen.rs:450`). On the box, 0 of 313
  subject links were dead.
- **Cost gates exist:** first drafts only on request, a pause after a human
  edit, rest intervals, an evidence fingerprint before the agent runs.
- **The UI has one detail router.** Day, year, person, place and org all render
  through `WikiContent.svelte` with one loading and error shell; articles use
  `Markdown variant="article"` everywhere; one `Ref` component gives every
  link a peek. No dead components in `components/wiki/`.
- **Retrieval steps 1–2 match their plan** (`search/wiki_first.rs`,
  `query.rs:524`, `indexer.rs:355`).

## The bad

### Coverage (live box)

- **The time axis is short.** Day pages begin 2026-06-17. 92% of person refs
  and 166k messages are older, back to 2017. No year rows exist.
- **13 days inside that range are missing** (06-22 to 07-07), each with 26–112
  messages but no `wiki_days` row. Two empty days were retried 6–7 times.
- **Only people get articles:** 0 of 79 places, 0 of 255 organizations.
- **Corrections were never used:** 0 notes, 0 rules.
- **Staleness is invisible:** `input_fingerprint` is NULL on all 95 day
  articles; 28 days have events newer than their article; 14 day articles are
  set to never update with no human edit.

### Subjects

- **The people table is noisy:**
  - 225 of 730 people have no refs.
  - 45 are companies or services.
  - 20 groups of exact duplicate names cover 48 rows, usually one row per
    address with the refs split between them.
  - `relationship_category` is empty on every row.
- **Organizations are mostly merchants:** 243 of 255.
- **No merge exists** anywhere in the code.
- **Reclassifying a person as an organization** leaves the person's article
  behind.

### Writing

- **Articles describe the evidence:** every person article says "the record",
  messages come up 69 times across them, and nearly half the day articles talk
  about messages or data. The 09-24 entity brief rewrite targets this. Unverified.
- **Few cross-links:** person articles link only to days, never to people,
  places or organizations. The draft's link allowlist pairs subjects that
  share a record, which almost never happens, and takes `LIMIT 12` with no
  order (`entity_article_gen.rs`).

### Backend

- **Concurrent first drafts can corrupt the record of what was written.** Both
  first-draft paths check and then create with no lock
  (`entity_article_gen.rs:73`, `years.rs:293`). `create_article` hands back an
  existing article silently, and `record_edition` then stores text that never
  reached the page.
- **Chapter seeds are not marked as the person's** (`narrative_draft.rs:610`),
  unlike stories (`stories.rs:170-183`).
- **Swallowed DB errors:** `entity_article_gen.rs:302,310,339`,
  `years.rs:227`.
- **Three link validators** (`sanitize_links`, `dead_links`,
  `day_article.rs:737`), and year drafts run none of them.
- **Notes have no machine writer.** The constitution calls them the editor's
  channel; `write_machine_notes` is called only from tests.

### UI

- **Server errors read as "not found":** `wiki/api.ts` returns null on any
  non-OK response (about 62 raw `fetch` calls, none through `client.ts`).
- **Editing differs by kind:**
  - Years have no edit, no maintenance control and no notes rail.
  - The editor opens beside the page for some kinds and in a new tab for
    others.
  - Person, place and org pages call `location.reload()` after a write.
- **The sidebar lights no row** on `/person/…`, `/place/…`, `/org/…`, `/year/…`
  or past days.
- **Two list views** for people, places and orgs; `WikiListView` and its three
  tables are reachable only by URL.
- **Three link parsers** with different strictness (`refRoutes.ts:102,120`,
  `wiki/links.ts:48`).

### Docs

- **The published manual misdefines Article** (`docs/understand/glossary.md:51-53,79`):
  - It says an article is never about a span of time, but days, years and
    chapters all have articles.
  - It promises every subject an article.
  - It describes chapter pages that depend on an applet shipped off.
- **`narrative-resolution-plan.md`** rests on widening `wiki_refs`; migration
  0026 narrowed it. It also cites dropped tables.
- **Chat sees three names for a day's text:** "article", "prose" and
  "narrated" (`sql_catalog.rs:451`, `circumstances.rs:459`).
- **Smaller stale lines:**
  - `attention-plan.md:52` names a column that does not exist.
  - A dead `"always"` arm sits at `server/api/wiki.rs:881`.
  - `circumstances.rs:442` points at a block that does not exist.

## The ugly

1. **Revisions are written without the rules.** The `wiki_editor` applet's
   agent is told "the rules for writing it are in your instructions above".
   They are not. `build_applet_system_prompt` (`agent/applet_runner.rs:375`)
   carries no constitution, no brief and no standing rules;
   `wiki_editor::system_prompt` is called only by the two first-draft paths.
   So every revision, and every chapter or story first write, ignores the
   house style and the person's "never write about X". Verified by hand.
2. **Day articles starve the editor.** `due_articles` (`wiki_editor.rs:586`)
   selects day and life articles. The applet skips kinds without a brief and
   records no pass for them (`applets/wiki_editor/main.rs:40`). Every day older
   than 30 days stays due and sorts first, so the three slots per run fill with
   days. Only articles someone pressed "update now" on ever get revised.
   Verified by hand.
3. **Search filtered by date returned nothing, everywhere.** `occurred_at` is
   NULL on all 250,902 embeddings on the box. Fixed on `wave` 2026-10-02
   (`3fc8725b`, with a backfill), not yet released.
4. **A large entity can panic its draft.** `p.truncate(MAX_TOTAL_CHARS)`
   (`entity_article_gen.rs:361`) cuts at a byte index, and every dossier line
   carries an em dash. Truncation also drops the link list, which comes last.
   Verified by hand.
5. **One dead link can lock a page forever.** `check_edit` keeps the person's
   sentences verbatim, while `check_links` checks the whole text
   (`wiki_editor.rs:993-994`). A link inside a sentence of theirs whose target
   later disappears refuses every revision after.
6. **Deleted stories and chapters stay searchable.** Their deletes bypass
   `delete_article` (`stories.rs:132`, `narrative_draft.rs:789`, `let _ =`).
7. **The only chat tool that writes to the wiki cites a string, not a record**
   (`tools/executor.rs:815`), against the editor's own citation rule.
8. **Day and year tabs likely fail to restore from a URL.** The id gains a
   second prefix on the way back (`day_day_…`; `tabs/registry.ts:448,486`).
   Read from code, not reproduced.

## Fix order

Ranked by impact over effort. The first four are small.

1. Hand `system_prompt(kind, standing_rules)` to the editor agent, with a test
   that the run's prompt contains the constitution. (Ugly 1)
2. Limit `due_articles` to kinds with a brief, in SQL. (Ugly 2)
3. Release the date fix and the wiki lead; re-run step 0 of
   `wiki-retrieval-plan.md`. (Ugly 3)
4. Make the dossier truncation char-safe and put the link list first. (Ugly 4)
5. Back-fill days before 2026-06-17 and fill the 13 holes. This is the largest
   gain in what the wiki knows. It needs a cost estimate first.
6. Clean the subject tables: merge duplicates, move services to
   organizations, and drop people with no refs from lists. Build merge first.
7. `create_article` says whether it created; `record_edition` only then.
   Seed chapters like stories. Route story and chapter deletes through
   `delete_article`.
8. Check links only in machine-written sentences on a revision. Keep one link
   validator and run it on years and days.
9. UI: errors distinct from not-found through `client.ts`; a refetch instead of
   reload; year article controls; sidebar `activeWhen` on detail pages; one
   link parser.
10. Docs: fix the manual's Article entry first, then the narrative-resolution
    premise, then one name for a day's text.

## Not measured

- **Whether the 09-24 entity brief cuts the "the record" writing.** No person
  article has been redrafted since.
- **Why 14 day articles are set to never update.**
- **Whether the 28 changed days changed in substance.**
- **The `handle_owner` query** that logged 83 slow-query warnings in three days.
- **The day/year tab restore bug,** which is read from code and not reproduced.
