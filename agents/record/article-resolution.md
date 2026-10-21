# Article resolution

**Built 2026-09-15/16, unreleased.** This document describes the wiki's second
architecture: every subject gets an article, one editor writes and maintains
all of them, and the person can always override the editor without having to
maintain the article themselves.

Supersedes `agents/plan/article-resolution-plan.md` (deleted). It replaces
the wiki plan's one-pen rule ([wiki-consolidation.md](wiki-consolidation.md)),
which is the main thing to know if you read that document first.

---

## The wiki as a set of resolution processes

The wiki is a set of **resolution processes**, each of which takes the owner's
data and resolves it into something with a name. It is not a pipeline that
compresses days into longer periods:

| resolution | what it resolves | state |
|---|---|---|
| **entity** | records → people, places, orgs | built, deterministic, no model |
| **event** | a day's rows → a gapless timeline | built, nightly |
| **topic** | an event → 2–4 tags | built, free, rides inside event resolution |
| **day** | a day's dossier → the day's article | built, nightly |
| **article** | a subject + the owner's data → that subject's article, maintained over time | this document |
| **semantic** | a remark here → the thing it refers to there | not scoped |

An earlier model folded day summaries upward into weeks, months and years.
That model was dropped because **a year article should not be a summary of
its days**. The days are already there, each with
a page a reader can open. A year article should say what is only true at the scale
of a year, and folding day summaries together cannot produce that.

## The first draft is one model call; revision is an agent loop

This split keeps the cost down:

| | how | why |
|---|---|---|
| **first draft** | one model call, everything in the prompt | there is no existing text to respect and nothing to go looking for |
| **revision** | an agent loop with tools | it must read the current article, work out what is genuinely new, find the evidence, decide whether it changes anything, and make a surgical edit — none of which is knowable in advance |

**Stories show why revision needs an agent.** Every other page is built on
existing material: a day has
its events, a year its days, a person the records that mention them. A story —
"Sailing", "How I learned to cook" — has no such material. Nobody has
gathered it and no list can be handed to the model. The only way to
write it is to search the owner's data, so revision *has* to be an agent loop;
and once it is, years and entities are other subjects for the same loop.

Designing for the hardest kind (stories) first is why there is one editor
instead of five.

## Where the editor runs

The applet runner has always had two phases: a subprocess, then — if the
manifest declares an `agent` prompt — an in-process agent loop holding
`YjsState`. **No applet had ever used it.** The `wiki_editor` applet is its
first caller.

The subprocess owns the gates and the hand-over; the agent phase does the
research and the writing. The toolset is `semantic_search`, `sql_query`,
`get_page_content`, `think`, and `revise_article`.

### Why the editor uses `revise_article` instead of `edit_page`

Chat edits pages through `edit_page`'s find/replace, and an early draft said
the editor should too — an agent loop can retry a failed anchor where a batch
job cannot. That reasoning is correct but incomplete.
Find/replace allows the retry but loses two things this design needs: a
guarantee the diff is small, and any enforcement of the invariant.
`edit_page` will happily apply an edit that paraphrases a sentence the person
wrote.

So the agent returns **the whole revised article**; the server diffs it against
the live text, checks it, and applies only what changed. A rejected edit comes back
as a tool error naming what failed, which the agent can act on in the same
turn. That keeps the retry loop the find/replace argument wanted, and the
server enforces the invariant instead of the prompt only asking for it.

`edit_page` is unchanged for chat, where the situation is the opposite: a
person is watching, and a failed anchor costs a sentence of conversation
instead of an hour of unnoticed failure.

## What the editor is told

System prompt = **one constitution** (`constitution.md`, shared by every article) + **one brief**
per subject kind + the active `wiki_rules`. The prompts are `.md` files under
`virtues-core/prompts/wiki/`, compiled in with `include_str!` so the text that
is reviewed is the same text that runs. See
[agents/build/wiki-editor.md](../build/wiki-editor.md).

The constitution's main rules: state only what is observed, never infer; length
follows the evidence, with no quota; **the person's own sentences take priority
over the source data** — placed, never paraphrased, never re-explained; **their removals are final**; edit rather than
rewrite; link by exact id only; never write the graph; never create a subject.

Articles are written in the **second person**. This was chosen after comparing
sample drafts of one dense year.

## Owner edits never turn off machine maintenance

Nobody wants to maintain their own wiki. They want to leave marginalia and
touch a sentence without becoming responsible for the page. The previous
design took the opposite view — `auto_update` was a boolean the Yjs layer
flipped false on the first real edit, which froze the article for good.

The replacement is a **server-enforced invariant** instead of a lock, computed
from the machine's own last output:

```
diff(live_text, machine_text) = exactly what the person did since
```

Inserted sentences join `theirs`; deleted sentences join `removed`; both are
pruned against the live text so they cannot grow without bound. Before any
machine edit is applied, `check_edit` refuses it unless every sentence in
`theirs` survives verbatim and nothing in `removed` has reappeared.

The invariant protects the exact text. **Where the text is placed is governed by
the constitution**, and the
person's answer to a bad placement is revert — which restores a chosen version
as a new version, so history is linear and nothing is ever lost. A standing
"don't write that again" is a `wiki_rules` row scoped to the page.

An earlier draft derived provenance from version history instead. The audit
ruled it out: the wire values are `user`/`ai`/`auto`, no wiki component cut
versions at all, a row's author names the *next* edit, and the 50-version cap
prunes the oldest human sentence first.

## Three checks before any model call

In order: **eligible** (`maintenance` is not `never`, and no human edit inside
six hours) → **due** (`update_requested_at`, or never written, or past its
interval — 7 days for years and stories, 30 for everything else) → **drifted**
(the evidence fingerprint moved).

The fingerprint follows the attention plan's rule and is not a counter. It is a
sha256 over labelled counts and max-timestamps of what the subject rests on,
which differs by kind. An entity rests on its refs; a year on its narrated
days; a chapter on the narrated days inside its span. **A story rests on
nothing measurable** — finding out whether its material grew would mean running
the agent's own searches, which is the expensive thing the gate exists to
avoid — so a story is revised when the person asks or accepts a note, and not
because the underlying data changed.

A pass that changes nothing still stores the new fingerprint. Without that, a
broken article would be re-run every hour.

## Subject kinds as built

| subject | brief | first version | maintained |
|---|---|---|---|
| **day** | its own released narrate prompt | nightly, one-shot | **no**, but rewritten when the owner asks — see below |
| **year** | `year.md` | editor, agentic | yes |
| **story** | `story.md` | the person's own sentence, never the model's | yes, on request |
| **chapter** | `chapter.md` | seeded from the interview | yes |
| **person / place / org** | `entity.md` | editor, agentic | yes, within a budget |
| **life / the self** | — | the interview write-up | **never** — the only channel is a note |

Two of those rows are intentional exclusions, not missing work.

**The life page is never machine-edited.** It is the one place in the wiki
that is not generated from the owner's data: written by the person, in the first
person, and the editor may not touch it. `brief_for` returns `None` for
`narrative_identity` so the exclusion is enforced in code instead of by a rule in a
prompt that a model might weigh against another rule.

**The day is not maintained by the editor yet.** Its narrator is released and tuned, and the
first draft stays exactly as it is. Whether the day's *revision* becomes just
another article is the attention plan's call, not this one — but it is the
largest remaining gap, because until it happens "one editor for every
subject" is true of six kinds out of seven.

What the day does have (2026-10-07) is the owner's **Rewrite this page**, at
the foot of a past day's page: a whole new first draft, written only when they
ask (`day_summary::rewrite_day_page`). It is not a revision, because nothing edits
around their sentences, so it asks first when the page may hold their words,
keeps the page as it was as a restore point, writes through the CRDT so open
editors receive it, and shows in History as one machine entry (History labels these as the record's) whose undo
puts the old page back. Its restamped `narrated_at` triggers one revision each of the day's year and
chapter at their next interval, when the wiki editor is on.

**The self is still keyed `narrative_identity` instead of the self person row.** The
plan called for re-pointing it; chat reads it on every turn, so that is a
migration with a fallback rather than a rename, and it was left. What did ship
is `ensure_self_person`: nothing had ever created the owner's own row, so on
most boxes the owner was the single human their wiki had no page for, and
every feature keyed to "the self" silently did nothing.

## Failure classes found during the build

Each of these cost real time. Each is described as a general class of bug.

**A column with a reader and no writer looks wrong instead of missing.** Migration 0025 dropped every wiki column in that state, and the
evidence for why is the symptom list: "Total visits: 0" on a place visited
weekly, "Interactions on record: 0" fed to the article model as fact, an empty
Last Interaction column down the whole people table, a readiness diamond that
never drew. A `NULL` shows up as a gap that someone notices. A zero shows up as a
number that someone believes.

**A queued write can be discarded.** `apply_text_diff`
originally left the save to the debounce. `get_or_create` then reseeded the
doc from `content` — correct, because `yjs_state` was still NULL — and silently
lost the first edit to every article. The fix is to persist immediately. The
class: any cache whose "empty" state falls back to another source must use
write-through, not write-behind.

**A TOML key after a table header belongs to that table.** `agent` sat below
`[config.limits]`, so it parsed as `config.limits.agent`, the column stayed
NULL, and the agent phase never ran. The same bug meant **Morning Examen had
never once run its agent prompt** — shipped, scheduled, and silently doing half
its job. Manifests now put scalar keys above any table header, with a comment
saying why.

**An allow-list is part of the tool's definition.** `revise_article` existed,
was documented, and was not in `APPLET_RUN_ALLOWED_TOOLS` — so the agent fell
back to `edit_page` and skipped every check this design exists to enforce.

**The first draft must claim its own output.** The first-draft path did not
call `record_edition`, so `machine_text` stayed NULL. The next revision would
have diffed against nothing, marked the entire article as `theirs`, and frozen
it permanently. Absent `machine_text` now means *unknown*, never *all theirs*.

**The model copies examples from its own prompt.** Asked to link
a person, the model emitted `/person/person_abc1` — the id from the example in
its own prompt, which exists nowhere. `check_links` now resolves every link
before an edit is applied.

**Third person crept into a second-person article**, and the cause was the fold
input, not the prompt: material describing the owner as "they" influences the
voice more strongly than a rule forbidding it. Both were fixed, and the
constitution now says explicitly that briefing material written in third
person uses the briefing's voice, which the article must not copy.

**An unbounded research budget gets spent re-finding known material.** A year revision hit
its ceiling having written nothing, across ten tool calls re-finding the days
it had already been handed. The subprocess now hands over what changed.

**Editing an applied migration breaks every box's boot.** A one-word comment
fix to migration 0015 changed its checksum, which `sqlx::migrate!` verifies —
every box would have refused to start on upgrade. Restored byte-for-byte; the
correction moved into a doc comment. Applied migrations are immutable, comments
included.

## Structural decisions

Three structural decisions came out of building the subject kinds. Each fixes
a problem that had already shipped twice.

### One registry for subject kinds

Every subject kind carries the same handful of facts: the word the schema
calls it, the prefix its ids start with, the table its row is in, the route the
client shows it at, the brief the editor writes it from. Those were spelled out
in ten places, each enumerating one facet, and the cost showed up as soon as a
subject kind was added. Chapters needed seven sites touched; three were missed, and all
three were defects — `check_links` skipped chapter and story links entirely,
`get_subject_backlinks` searched for an href nothing can write, and @-mention
autocomplete could not produce either.

`api/subjects.rs` is now the one table. **Adding a subject kind means adding a row.** Tests check it against both
sides: a test reads `pg_get_constraintdef` and compares the
registry to the schema's CHECK in both directions, and another reads the
client's `links.ts` and fails if its route table disagrees — the same lockstep
the plugin ACL uses, because there is no codegen between Rust and TypeScript.

The registry makes two distinctions explicit:

**A subject's `kind` and its id `prefix` are different values.** `organization` is the schema's
word and `org_ab12` is the id. They differ for exactly one kind, and a lookup
keyed on the wrong one returns zero rows instead of an error, which makes the
mismatch hard to notice.

**"Entity" names three of the subject shapes. It is not a synonym for "subject."**
`wiki_articles.subject_type` allows all eight; `wiki_refs.entity_type` allows
three, because a data row can only point AT an entity. A row cannot reference
a year — a year is a partition the app imposes, not something an email mentions. The
two lists are different on purpose. Aligning them looks like obvious tidying
but would be wrong. `agents/build/glossary.md` carries the three
shapes; the registry is that section in code.

### Components use the API types directly

Every wiki component typed against a converted "page type" — an API type
through a converter into a `PersonPage`. Counting references showed how little the
layer was used: thirteen of the fourteen types it exported had no consumer
outside its own directory.

This was a correctness problem, because
**optional fields type-check, compile, and render as nothing**. PersonPage read
`company`, `role`, `location` and `connectionTier` in nineteen places — a whole
Work row that could never draw. The day page gated an entire Entities section,
its heading and its table-of-contents entry on a structure the converter
hardcoded empty. A day full of people listed none, while the metadata block
below it reported how many were new.

1,256 lines became 171.

### When to keep a converter

One converter was kept, and it shows where to draw the line. `apiToDayEvent`
looks like the four that were deleted but works differently: it parses two strings into
dates, **computes** `durationMinutes` which is not on the wire at all, applies
defaults, distinguishes `|| undefined` from `?? undefined` so an empty string
and an absent value differ, and guards two jsonb columns that arrive untyped.
Delete it and every one of those moves into the eighty-eight call sites that
read the result.

The four that went did none of that. They renamed fields and filled in blanks
that stayed blank. **Keep a converter only if it does work the caller would otherwise
have to do.** A converter that only renames fields should be deleted.

## What shipped

Migrations `0022` (article resolution columns, subject-type vocabulary aligned
across four disagreeing CHECK lists, `wiki_years.summary`), `0023` (the story
subject), `0024` and `0025` (the drops).

**0024 and 0025 cannot be rolled back.** A dropped column means the previous
binary cannot boot, which takes `virtues upgrade`'s rollback with it for the
release that carries them. Both were written only after their readers were
removed in code — the ordering rule the
[2026-08-28 schema audit](schema-audit-2026-08-28.md) exists to enforce — and
anything whose sweep was unfinished was left in place on purpose, even if it
looked unused.

**The applet ships disabled** (`default_enabled = false`). Everything above was
tested on the dev box against real data; none of it has run on a box.
Enable it on purpose, and watch the first week closely: the ceiling of three
articles per hourly run is a guess, and the
per-run cost ceiling was already raised once under pressure.

## Editions record the block export (2026-10-11, unreleased)

Articles became block pages in slice 6 of
[document-contract-plan.md](../plan/document-contract-plan.md). Provenance
did not change its model (`diff(live, machine_text)` is what the person did),
but the text on both sides of that diff did, and three things had to follow.

**The edition is the page's exported text, not the text the writer sent.** On a block
article the page's text is the export of its tree, which escapes what the
writer did not (`R\&B`, `user\_name`) and writes footnote definitions in the
order the page cites them. A writer that recorded its own markdown as
`machine_text` would make every article read as owner-edited after one
revision, and the wiki editor would freeze them all. So every writer records
the export: the wiki editor its draft's text (`YjsState::draft`), the day's
rewrite and the picture what the write left.

**The move to blocks rebases the edition, in the same transaction as the switch**
(`block_move::rebase`, `record_rebase`). An article untouched since its
writers wrote it gets the export as its edition; an edited one, its edition
converted on its own. Each sentence of the owner's becomes the export's
sentence that says it, matched by its key and then by its plain words; one
the export cannot place keeps the article in markdown, because a sentence of
theirs that no longer matches is a sentence the invariant would stop
protecting. One that is no longer a whole sentence of the page (its words
now inside a longer sentence the owner wrote) is dropped, as the next
revision's `fold_provenance` drops it. The update is guarded on the edition
as read, so a writer that recorded one meanwhile makes the move read the
page again.

**Comparisons convert both sides each time; the export format is not pinned.** Pinning the export's
rules and re-recording every edition when they change was considered and
rejected: one missed pass and every article freezes. Instead each "has the
owner changed it?" converts both sides through this server's converter and
export (`block_move::as_compared`), so an edition recorded under an earlier
export compares equal to the same text written by this one. The cost is one
conversion per check, off the async workers, and only for block articles.

History reads the move's two versions, "Before blocks" and "Moved to blocks",
as restore points (`pages::is_move_version`): the move is nobody's edit, and
the first entry after it is diffed against the blocks.
