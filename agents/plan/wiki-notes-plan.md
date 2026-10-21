# Wiki notes and merge

**What is left of the wiki consolidation.** The rest shipped and is recorded in
[wiki-consolidation.md](../record/wiki-consolidation.md); the note rules are in
[wiki-editor.md](../build/wiki-editor.md). This plan is the note writer, the
two columns it needs, and entity merge. The attention side of notes — what
automated passes may *not* write as one — is
[attention-plan.md](attention-plan.md) §Noting; not repeated here.

## Prerequisite: show that notes are useful

Nothing has shown that an AI reading a finished day can leave notes a person
is glad to find. The last adjacent attempt, semantic entity resolution, was
deleted.

So before writing any prompt, take 3–5 dense days, **hand-write twenty notes
the way they should read**, and judge each against the note rules: does it
cite, would the article change, would you click Accept. The notes that pass
become the few-shot examples and the acceptance test for the writer. If the
twenty hand-written notes turn out to be noise, we do not build the writer. It needs a person reading real days,
so it runs on a box with real data, not the demo copy.

## The writer

`api::wiki_notes::write_machine_notes` exists — cap, cite check, log when the
cap binds — and has **no caller outside its tests**. There is no prompt, and
no production writer of machine notes: the one there was, the chat tool
`propose_narrative_identity_edit`, was removed on 2026-10-10.

Design:

- **A third pass at the end of the nightly day run**, after narration. It is
  not folded into segmentation (fragile JSON, runs before scoring) or narration
  (which compresses away the details notes are made of).
- **It asks a different question of the same material:** what does this day
  reveal about things that are not this day?
- **It reads the subjects' current articles and open notes first**, so it can
  skip a note and say why in prose. Prose is the only form that can tell two
  phrasings of one observation apart.
- **Inputs are document-shaped only:** calendar, messages, the owner's chats,
  transcriptions, and email filtered by reciprocity (has the owner ever written
  to this address?) rather than vendor labels. Signal series are never inputs.
- **`kind`:** a note disputing the article is `correction`; one about something
  the article lacks is `observation`.

Recurrence carries information — five notes about one fact mean the article
is out of date — so count repeats instead of deduplicating them. That contradicts attention-plan's
uniqueness constraint; settle it with the gold notes in hand.

## The `absorbed` exit

Nothing stamps `absorbed`. The editor must take explicit note ids and stamp
exactly those it reports using; absorption cannot be inferred from the text it
emitted. Without it, notes drain only by hand and "the bar is too low" is
unfalsifiable.

## Two columns

- **`refers_to_start` / `refers_to_end`**, both nullable: time as a range, not
  a date with a precision flag. "March" is `[03-01, 04-01)`; "after June" is
  `[06-01, NULL)`. This lets a note come due — "what is
  waiting on this week" is an overlap test.
- **`written_against_version`**, nullable, on a `correction`: the
  `app_page_versions.version_number` it contests, or a note disputing a
  sentence since deleted is unresolvable.

Claim the number with `make migration` first.

## Merge

Duplicate entities — one sender under two addresses — sort low but stay
duplicated. Merge is the one wiki operation that can corrupt stored data, so it
gets its own design pass: rewrite rows in `wiki_refs` under its
`NULLS NOT DISTINCT` unique index, union `emails`/`phones`/`handles`/`aliases`,
re-point articles, notes, pins and project items, and soft-delete the loser as
merged-into rather than dropping it. The self person can never be the loser.
Candidates can arrive through [narrative-resolution-plan.md](narrative-resolution-plan.md)'s
queue (`answer_shape = 'merge'`); the operation itself is here.
