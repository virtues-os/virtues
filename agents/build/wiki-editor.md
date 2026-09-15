# The wiki editor — constitution and briefs

**How the wiki's prose is instructed.** The design is
[article-resolution-plan.md](../plan/article-resolution-plan.md); this says
what must be true when you touch the prompts.

## The text is not in this file

The prompts live beside the code and are compiled in:

| file | what it is |
|---|---|
| [`virtues-core/prompts/wiki/constitution.md`](../../virtues-core/prompts/wiki/constitution.md) | the rules that never vary, shared by every article |
| [`virtues-core/prompts/wiki/year.md`](../../virtues-core/prompts/wiki/year.md) | the brief for a year |

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

- **Second person, past tense.** Not an open question: [voice.md](voice.md)
  settles it for every surface where Virtues speaks, and the day prompt
  already writes that way. The objection worth knowing — that an article about
  you, addressed to you, can read as a report on a subject — is answered by
  the observe-never-infer rule and the "refuses to flatter" register, not by
  switching to the third person and giving the wiki a third voice.
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

## The trap that produced this doc

`refresh_due_entity_articles` documents, in its own comment, that maintenance
"belongs in an applet's AGENT phase… and edits through the same find/replace
path the assistant already uses on pages — which is also the only way to get
reviewable diffs instead of a 100% rewrite every edition." That was written,
and then the function returned `Ok(0)` for months while its applet shipped
disabled. **A prompt with no caller is not a plan, it is a comment.** Do not
add a brief for a subject kind until something actually runs it.
