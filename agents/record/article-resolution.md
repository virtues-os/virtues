# Article resolution

**Built 2026-09-15/16, unreleased.** The design record for the wiki's second
architecture: every subject gets an article, one editor writes and maintains
all of them, and the person can always overrule it without ever being made
responsible for it.

Supersedes `agents/plan/article-resolution-plan.md` (deleted). It overrules
[wiki-plan](../plan/wiki-plan.md) §10, the one-pen rule, which is the main
thing to know if you read that document first.

---

## The thesis

The wiki is not a compression pipeline. It is a set of **resolution
processes**, each of which takes the record and resolves it into something
with a name:

| resolution | what it resolves | state |
|---|---|---|
| **entity** | records → people, places, orgs | built, deterministic, no model |
| **event** | a day's rows → a gapless timeline | built, nightly |
| **topic** | an event → 2–4 tags | built, free, rides inside event resolution |
| **day** | a day's dossier → the day's article | built, nightly |
| **article** | a subject + the record → that subject's article, maintained over time | this record |
| **semantic** | a remark here → the thing it refers to there | not scoped |

An earlier model folded day summaries upward into weeks, months and years.
That framing was wrong, and the sentence that killed it is that **a year
article is not a summary of its days**. The days are already there, each with
a page a reader can open. A year exists to say what is only true at the scale
of a year, which no amount of folding produces.

## Draft is one-shot; revision is agentic

The split that makes it affordable:

| | how | why |
|---|---|---|
| **first draft** | one model call, everything in the prompt | there is no existing text to respect and nothing to go looking for |
| **revision** | an agent loop with tools | it must read the current article, work out what is genuinely new, find the evidence, decide whether it changes anything, and make a surgical edit — none of which is knowable in advance |

**The story rung is the proof.** Every other page sits on something: a day has
its events, a year its days, a person the records that mention them. A story —
"Piano & Composition", "How I learned to pray" — sits on nothing. Nobody has
gathered its material and no list can be handed to the model. The only way to
write it is to go and search the record, so revision *has* to be agentic; and
once it is, years and entities are simply other subjects for the same loop.

Picking the hardest rung first is what produced one editor instead of five.

## The vehicle

The applet runner has always had two phases: a subprocess, then — if the
manifest declares an `agent` prompt — an in-process agent loop holding
`YjsState`. **No applet had ever used it.** The `wiki_editor` applet is its
first caller.

The subprocess owns the gates and the hand-over; the agent phase does the
research and the writing. The toolset is `semantic_search`, `sql_query`,
`get_page_content`, `think`, and `revise_article`.

### Why `revise_article` and not `edit_page`

Chat edits pages through `edit_page`'s find/replace, and an early draft said
the editor should too — an agent loop can retry a failed anchor where a batch
job cannot. That reasoning is correct and is not the whole question.
Find/replace gives back the retry and gives up two things this design needs: a
guarantee the diff is small, and any enforcement of the invariant.
`edit_page` will happily apply an edit that paraphrases a sentence the person
wrote.

So the agent returns **the whole revised article**; the server diffs it against
the live text, checks it, and applies only what changed. A refusal comes back
as a tool error naming what failed, which the agent can act on in the same
turn — the retry loop the find/replace argument was defending, with the
invariant as something the server refuses rather than something the prompt
asks for.

`edit_page` is unchanged for chat, whose situation is the opposite: a person is
watching, and a failed anchor costs a sentence of conversation rather than a
silent hour.

## What the editor is told

System prompt = **one constitution** (shared by every article) + **one brief**
per subject kind + the active `wiki_rules`. The prompts are `.md` files under
`virtues-core/prompts/wiki/`, compiled in with `include_str!` so the text that
is reviewed is the text that runs. See
[agents/build/wiki-editor.md](../build/wiki-editor.md).

The constitution's load-bearing rules: observe never infer; length follows the
evidence, not a quota; **their lines outrank the record** — placed, never
paraphrased, never re-explained; **their removals are final**; edit rather than
rewrite; link by exact id only; never write the graph; never create a subject.

The register is **second person**, settled on specimens of one real dense year.

## Ownership never flips

Nobody wants to maintain their own record. They want to leave marginalia and
touch a sentence without becoming responsible for the page. The previous
design took the opposite view — `auto_update` was a boolean the Yjs layer
flipped false on the first real edit, which froze the article for good.

What replaces it is not a lock but a **server-enforced invariant**, computed
from the machine's own last output:

```
diff(live_text, machine_text) = exactly what the person did since
```

Inserted sentences join `theirs`; deleted sentences join `removed`; both are
pruned against the live text so they cannot grow without bound. Before any
machine edit is applied, `check_edit` refuses it unless every sentence in
`theirs` survives verbatim and nothing in `removed` has reappeared.

The invariant protects bytes. **Placement is the constitution's job**, and the
person's answer to a bad placement is revert — which restores a chosen version
as a new version, so history is linear and nothing is ever lost. A standing
"don't write that again" is a `wiki_rules` row scoped to the page.

An earlier draft derived provenance from version history instead. The audit
killed it: the wire values are `user`/`ai`/`auto`, no wiki component cut
versions at all, a row's author names the *next* edit, and the 50-version cap
prunes the oldest human sentence first.

## Three gates before any money is spent

In order: **eligible** (`maintenance` is not `never`, and no human edit inside
six hours) → **due** (`update_requested_at`, or never written, or past its
interval — 7 days for years and stories, 30 for everything else) → **drifted**
(the evidence fingerprint moved).

The fingerprint is the attention plan's doctrine rather than a counter: a
sha256 over labelled counts and max-timestamps of what the subject rests on,
which differs by kind. An entity rests on its refs; a year on its narrated
days; a chapter on the narrated days inside its span. **A story rests on
nothing measurable** — finding out whether its material grew would mean running
the agent's own searches, which is the expensive thing the gate exists to
avoid — so a story is revised when the person asks or accepts a note, and not
because the record moved underneath it.

A pass that changes nothing still stores the new fingerprint. Without that, a
broken article is an hourly oscillator.

## The rungs, as built

| subject | brief | first version | maintained |
|---|---|---|---|
| **day** | its own released narrate prompt | nightly, one-shot | **no** — see below |
| **year** | `year.md` | editor, agentic | yes |
| **story** | `story.md` | the person's own sentence, never the model's | yes, on request |
| **chapter** | `chapter.md` | seeded from the interview | yes |
| **person / place / org** | `entity.md` | editor, agentic | yes, within a budget |
| **life / the self** | — | the interview write-up | **never** — the only channel is a note |

Two of those rows are deliberate refusals rather than gaps.

**The life page is never machine-edited.** It is the one place in the wiki
where the record is not the author: written by the person, in the first
person, and the editor may not touch it. `brief_for` returns `None` for
`narrative_identity` so that the refusal is structural rather than a rule in a
prompt that a model might weigh against another rule.

**The day does not join yet.** Its narrator is released and tuned, and the
first draft stays exactly as it is. Whether the day's *revision* becomes just
another article is the attention plan's call, not this one — but it is the
largest remaining seam, because until it happens "one editor for every
subject" is true of six kinds out of seven.

**The self is still keyed `narrative_identity`, not the self person row.** The
plan called for re-pointing it; chat reads it on every turn, so that is a
migration with a fallback rather than a rename, and it was left. What did ship
is `ensure_self_person`: nothing had ever created the owner's own row, so on
most boxes the owner was the single human their wiki had no page for, and
every feature keyed to "the self" silently did nothing.

## Failure classes found along the way

Each of these cost real time, and each is a class rather than an incident.

**A column with a reader and no writer does not read as missing — it reads as
wrong.** Migration 0025 dropped every wiki column in that state, and the
evidence for why is the symptom list: "Total visits: 0" on a place visited
weekly, "Interactions on record: 0" fed to the article model as fact, an empty
Last Interaction column down the whole people table, a readiness diamond that
never drew. A `NULL` surfaces as a hole someone notices. A zero surfaces as a
number someone believes.

**A write that is queued is a write that can be discarded.** `apply_text_diff`
originally left the save to the debounce. `get_or_create` then reseeded the
doc from `content` — correct, because `yjs_state` was still NULL — and silently
ate the first edit to every article. The fix is to persist immediately. The
class: any cache whose "empty" state falls back to another source must be
written through, not behind.

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

**A prompt's own example is a thing the model will reach for.** Asked to link
a person, the model emitted `/person/person_abc1` — the id from the example in
its own prompt, which exists nowhere. `check_links` now resolves every link
before an edit is applied.

**Third person crept into a second-person article**, and the cause was the fold
input, not the prompt: material describing the owner as "they" teaches the
voice more effectively than a rule forbidding it. Both were fixed, and the
constitution now says explicitly that briefing material written in third
person is the briefing's voice, not the article's.

**An unbounded research budget is spent on rediscovery.** A year revision hit
its ceiling having written nothing, across ten tool calls re-finding the days
it had already been handed. The subprocess now hands over what changed.

**Editing an applied migration breaks every box's boot.** A one-word comment
fix to migration 0015 changed its checksum, which `sqlx::migrate!` verifies —
every box would have refused to start on upgrade. Restored byte-for-byte; the
correction moved into a doc comment. Applied migrations are immutable, comments
included.

## What shipped

Migrations `0022` (article resolution columns, subject-type vocabulary aligned
across four disagreeing CHECK lists, `wiki_years.summary`), `0023` (the story
subject), `0024` and `0025` (the drops).

**0024 and 0025 are a one-way door.** A dropped column means the previous
binary cannot boot, which takes `virtues upgrade`'s rollback with it for the
release that carries them. Both were written only after their readers were
removed in code — the ordering rule the
[2026-08-28 schema audit](schema-audit-2026-08-28.md) exists to enforce — and
anything whose sweep was unfinished was deliberately left in place, however
dead it looked.

**The applet ships disabled** (`default_enabled = false`). Everything above was
proven on the dev box against a real record; none of it has run on a box.
Enabling it should be a deliberate act, and the first week of it wants
watching: the ceiling of three articles per hourly run is a guess, and the
per-run cost ceiling was already raised once under pressure.
