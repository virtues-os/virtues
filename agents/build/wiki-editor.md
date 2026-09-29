# The wiki editor — constitution and briefs

**How the wiki's prose is instructed.** The design is
[agents/record/article-resolution.md](../record/article-resolution.md); this says
what must be true when you touch the prompts.

## The text is not in this file

The prompts live beside the code and are compiled in:

| file | what it is |
|---|---|
| [`virtues-core/prompts/wiki/constitution.md`](../../virtues-core/prompts/wiki/constitution.md) | the rules that never vary, shared by every article |
| [`virtues-core/prompts/wiki/year.md`](../../virtues-core/prompts/wiki/year.md) | the brief for a year |
| [`virtues-core/prompts/wiki/chapter.md`](../../virtues-core/prompts/wiki/chapter.md) | the brief for a chapter — an era the person drew |
| [`virtues-core/prompts/wiki/story.md`](../../virtues-core/prompts/wiki/story.md) | the brief for a story — a subject they named, with nothing beneath it |
| [`virtues-core/prompts/wiki/entity.md`](../../virtues-core/prompts/wiki/entity.md) | the brief for a person, place or organization |

Two subject kinds have **no** brief, and both absences are refusals rather
than gaps. The life page is the person's, in the first person, and the editor
may not touch it. The day has its own released narrate prompt and joins this
door when its revision does. `brief_for` returning `None` is what makes each
refusal structural — a rule in a prompt is something a model weighs against
its other rules, and a missing brief is not.

A system prompt is **constitution + brief**, concatenated, plus the active
`wiki_rules`. The turn's opening message carries the subject, its authored
fields, the person's sentences and removals, what changed since the last
edition, and the tool budget.

They are `.md` files rather than Rust string constants so that the thing
reviewed is the thing that runs. `include_str!` from outside the crate is an
established pattern here (`applets/sources.toml`). **Never paraphrase a prompt
into a doc** — that is how the two prompts we already had drifted apart.

## Why one constitution and many briefs

There were two editors before this — the day narrator and the entity article
writer — with two prompts restating the same policy in different words, and
each drift between them was invisible until prose came out wrong. Years,
stories and chapters would have made five.

So: one constitution carries the policy, one brief per subject kind carries
what *that page* is. A rule that belongs to every article goes in the
constitution. A rule about what a year looks like goes in the year brief. If
you find yourself writing "observe, never infer" into a brief, it belongs one
level up.

## What is settled

- **Second person, past tense.** Not an open question: it is settled here,
  and the day prompt already writes that way. (It used to cite
  [voice.md](voice.md), which claimed one voice for every surface; that claim
  was cut 2026-09-21, and the wiki's voice is the wiki's own — the record read
  back, not a house style.) The objection worth knowing — that an article
  about you, addressed to you, can read as a report on a subject — is answered
  by the observe-never-infer rule and by refusing to flatter, not by switching
  to the third person and giving the wiki a third voice.
- **The three voices are deliberate.** A day is second person (a mirror you
  read at night); an entity is third person about the entity and second about
  the owner; the life document is first person, theirs, and is **never**
  machine-edited.
- **Editing, not rewriting.** The entity prompt's "this is an edition, not an
  append: rewrite the whole article" is superseded. A whole-document rewrite in
  a CRDT discards concurrent edits by construction and makes every revision
  diff at 100%, which shows the person nothing.

## Changing a prompt

1. Edit the `.md`. The constants pick it up at build time.
2. **Read the diff as the model will.** These are instructions, not prose;
   a sentence that reads as advice will be followed as a rule.
3. A changed constitution does not retroactively fix written articles. Use the
   bulk `update_requested_at` path to re-edit a rung deliberately, or leave
   them — an article is not wrong because the rules improved.
4. Prompts are not migrations, but they are close: every box gets the new text
   on upgrade and applies it to pages that already exist.

## Notes, and what may not be one

A `wiki_notes` row is the machine's only channel into the record: a cited
proposal about a subject, which a person accepts or dismisses. The writer that
leaves them nightly is not built yet ([wiki-notes-plan.md](../plan/wiki-notes-plan.md));
these rules hold for it and for anything else that writes a note.

- **Cite or reject.** A machine note without `source_refs` cannot exist —
  `wiki_notes_machine_must_cite` is a CHECK, and `write_machine_notes` refuses
  it by name first. The asymmetry is the design: a cited note is useful even
  when wrong, because it is checkable in seconds; a bare claim with a
  confidence is worthless when wrong. A citation is a record route, not the
  model's own reasoning restated.
- **Only a pass that held a whole session writes one** — a finished day, a
  chat thread. Never a sweep over isolated rows: a fragment cannot tell sarcasm
  from statement, or when a thing was said from when it happened. That is what
  killed semantic entity resolution.
- **If it's about today, it isn't a note.** *"You had coffee with Maya"* is the
  day. *"Maya mentioned she's leaving in March"* is a note, because March isn't
  today.
- **If the article wouldn't change, it isn't a note.** So the writer is handed
  the current article of each subject in play. Silence is the default.
- **The writer never writes `wiki_refs`.** Not at low confidence, not flagged.
  The graph stays deterministic and user-authored.
- **Notes never age out.** They leave by `accepted`, `dismissed` or `absorbed`
  — events, never timers.
- **Volume is capped in code** (`MAX_NOTES_PER_RUN`), and the cap logs when it
  binds. Raising it is the wrong fix for a bar that is too low.

## Events are evidence, not article subjects

`wiki_events` carries its own summary and may be a note's subject. It never
gets an article, a revision history or a maintenance setting. Articles are for
things a person would name; that is what keeps their number bounded. Do not
add `event` to `wiki_articles.subject_type` or give it a brief.

## The trap that produced this doc

`refresh_due_entity_articles` documents, in its own comment, that maintenance
"belongs in an applet's AGENT phase… and edits through the same find/replace
path the assistant already uses on pages — which is also the only way to get
reviewable diffs instead of a 100% rewrite every edition." That was written,
and then the function returned `Ok(0)` for months while its applet shipped
disabled. **A prompt with no caller is not a plan, it is a comment.** Do not
add a brief for a subject kind until something actually runs it.
