# The wiki editor — constitution and briefs

This doc describes how the prompts that write the wiki's prose are organized.
The "constitution" is the shared prompt file every article uses
(`virtues-core/prompts/wiki/constitution.md`, loaded as `CONSTITUTION` in
`virtues-core/src/api/wiki_editor.rs`); a "brief" is the per-subject-kind
prompt added after it. The design is
[agents/record/article-resolution.md](../record/article-resolution.md); this
doc lists what must stay true when you change the prompts.

## Where the prompt files live

The prompts live beside the code and are compiled in:

| file | what it is |
|---|---|
| [`virtues-core/prompts/wiki/constitution.md`](../../virtues-core/prompts/wiki/constitution.md) | the rules that never vary, shared by every article |
| [`virtues-core/prompts/wiki/year.md`](../../virtues-core/prompts/wiki/year.md) | the brief for a year |
| [`virtues-core/prompts/wiki/chapter.md`](../../virtues-core/prompts/wiki/chapter.md) | the brief for a chapter — an era the person drew |
| [`virtues-core/prompts/wiki/story.md`](../../virtues-core/prompts/wiki/story.md) | the brief for a story — a subject they named, with nothing beneath it |
| [`virtues-core/prompts/wiki/entity.md`](../../virtues-core/prompts/wiki/entity.md) | the brief for a person, place or organization |

Two subject kinds have **no** brief, and in both cases that is intentional.
The life page belongs to the person, is written in the first person, and the
editor may not touch it. The day has its own released narrate prompt and will
move to this system when that prompt is next revised. `brief_for` returning
`None` enforces both exclusions in code: a model weighs a rule in a prompt
against its other rules, but it cannot work around a brief that does not
exist.

A system prompt is **constitution + brief**, concatenated, plus the active
`wiki_rules`. The turn's opening message carries the subject, its authored
fields, the person's sentences and removals, what changed since the last
edition, and the tool budget.

They are `.md` files rather than Rust string constants so that the text people
review is exactly the text that runs. `include_str!` from outside the crate is an
established pattern here (`applets/sources.toml`). **Never paraphrase a prompt
into a doc** — that is how the two prompts we already had drifted apart.

## Why there is one constitution and many briefs

Before this there were two editors, the day narrator and the entity article
writer, with two prompts restating the same policy in different words. Each
difference between them went unnoticed until the generated prose came out
wrong. Adding years, stories and chapters the same way would have made five.

So one constitution holds the shared policy, and one brief per subject kind
describes what *that kind of page* is. A rule that belongs to every article goes in the
constitution. A rule about what a year looks like goes in the year brief. If
you find yourself writing "don't infer" into a brief, it belongs in the
constitution instead.

## Decisions already made

- **Second person, past tense.** This is decided, and the day prompt already
  writes that way. The wiki has its own style; no house style governs it. One
  objection is worth knowing: an article about you, addressed to you, can sound
  like a report on a subject. The answer is the observe-never-infer rule and
  avoiding flattery, not switching to the third person, which would give the
  wiki a third point of view.
- **The three points of view are intentional.** A day is second person
  (something you read back at night); an entity is third person about the
  entity and second person about the owner; the life document is first person,
  written by the owner, and is **never** edited by the machine.
- **The editor edits; it does not rewrite.** The entity prompt's "this is an
  edition, not an append: rewrite the whole article" is superseded. A
  whole-document rewrite in a CRDT always discards concurrent edits and makes
  every revision a 100% diff, which tells the person nothing about what
  changed.

## Changing a prompt

1. Edit the `.md`. The constants pick it up at build time.
2. **Read the diff the way the model will.** These are instructions: the
   model will follow a sentence written as advice as if it were a rule.
3. A changed constitution does not retroactively fix written articles. Use the
   bulk `update_requested_at` path to re-edit one level of the hierarchy on
   purpose, or leave them; an article is not wrong just because the rules
   improved.
4. Prompts are not migrations, but they behave similarly: every box gets the
   new text on upgrade and applies it to pages that already exist.

## Articles stored as block pages

An article becomes a block page when the server first builds its live
document (`api/block_move.rs`, `first_open`, behind `VIRTUES_TREE_ARTICLES=1`
with block pages on), or in the conversion run once opened. The editor does
not change for it: it still reads the article as markdown and returns the
whole revised article as markdown. What changes is the server's half of
`revise_article`:

- **The page is read first, the article's row after.** Building the document
  can move an article nobody had opened, and the move rewrites its edition
  (`block_move::rebase`). Reading the row before the page would diff against
  an edition that no longer exists.
- **The model's text becomes a draft** (`YjsState::draft`): on a block page,
  its markdown converted as an article (`parse_article_markdown`: footnotes,
  figures and veils become the contract's own nodes), and the draft's text is
  the blocks' export. Markdown the contract refuses is an `InvalidInput` back
  to the agent, with nothing written.
- **The invariant runs on the draft, never the raw text**, and compares
  sentences by key (`sentence_key`: escapes and the character references an
  export writes read as their characters, whitespace collapsed). The export
  writes `R\&B` where the model wrote `R&B`; compared raw, the owner's own
  sentence would read as lost, or a sentence of the machine's as theirs.
- **The edition is the draft's text, recorded before the write**
  (`record_edition`), and the write is `YjsState::write_draft`: block by
  block (`virtues_document::revise_ops`), so a block the draft shows as the
  page has it keeps its items and any caret in it, and a block changed a
  little is patched in place, keeping its id. Should the write leave other
  text than the draft, what it left is recorded instead, and logged.
- **"Has the owner changed it?" re-projects both sides**
  (`block_move::as_compared`: the edition and the live text each as this
  server's export would write them). A release that writes an escape or a
  list marker differently then changes no answer. The day page's
  `has_your_edits` and the picture's `edition_follows` compare the same way.
  A pending suggestion of the owner's reads as one, `{--…--}{++…++}`, never
  as decided, so a sentence under it is theirs and the invariant keeps it.
- **The owner's pending suggestion is theirs to decide.** The draft keeps
  a suggestion the model carried back as the page has it, and a revision
  that would change or drop a block holding one is refused with nothing
  written (`suggestions_taken`); the model is told to keep that paragraph
  as it is. A suggestion the model writes of its own comes in accepted.
  The owner's rewrite of a day page, a new draft from the record that never
  carries a suggestion back, is refused before its writer runs while the
  page holds one (`pending_suggestion`).
- **A revert of a block article goes by blocks** (`pages::restore_version`):
  a version from before the article held blocks is converted as an article,
  its pending suggestions kept, and `machine_text` is left alone, as on a
  markdown article. The restore stamps `last_human_edit_at` itself, as
  every version put back from History does.

Find/replace (`apply_text_edit`) is the one text path a block page refuses:
its anchors are markdown the page does not hold.

## Notes, and what may not be a note

A `wiki_notes` row is the only way the machine can propose a change to the
person's wiki: a cited proposal about a subject, which a person accepts or
dismisses. The writer that
leaves them nightly is not built yet ([wiki-notes-plan.md](../plan/wiki-notes-plan.md));
these rules hold for it and for anything else that writes a note.

- **Cite or reject.** A machine note without `source_refs` cannot exist —
  `wiki_notes_machine_must_cite` is a CHECK, and `write_machine_notes` refuses
  it by name first. The reason: a cited note is useful even when wrong,
  because it can be checked in seconds, while an uncited claim with a
  confidence score is worthless when wrong. A citation points to a source row
  in the data; restating the model's own reasoning is not a citation.
- **Only a pass that read a whole session writes one** — a finished day, a
  chat thread. Never a sweep over isolated rows: a fragment cannot distinguish
  sarcasm from a statement, or when something was said from when it happened.
  That is the problem that caused semantic entity resolution to be removed.
- **If it's about today, it isn't a note.** *"You had coffee with Maya"* is the
  day. *"Maya mentioned she's leaving in March"* is a note, because March isn't
  today.
- **If the article wouldn't change, it isn't a note.** So the writer is handed
  the current article of each subject in play. Silence is the default.
- **The writer never writes `wiki_refs`.** Not at low confidence, not flagged.
  The graph stays deterministic and written by the user.
- **Notes never expire.** They are removed only when `accepted`, `dismissed`
  or `absorbed` — by an event, never by a timer.
- **Volume is capped in code** (`MAX_NOTES_PER_RUN`), and the cap logs when it
  is reached. If the cap is being hit because the quality threshold is too low,
  raise the threshold rather than the cap.

## Events are not article subjects

`wiki_events` carries its own summary and may be a note's subject. It never
gets an article, a revision history or a maintenance setting. Articles are only
for things a person would name, which keeps the number of articles bounded. Do not
add `event` to `wiki_articles.subject_type` or give it a brief.

## Do not add prompts that nothing calls

The failure this doc exists to prevent: a brief or a maintenance function that
describes the right design (edit through the same find/replace path the
assistant uses on pages, in an applet's AGENT phase, so edits are reviewable
diffs rather than rewrites) while nothing calls it. It looks like working code,
does nothing, and its applet ships disabled for months. **Do not add
a brief for a subject kind until something actually runs it.**
