# Person pages

Proposed 2026-10-10. How a person's page gets a description of who they are to you at low
cost, and how that description stays accurate over years.

## What is wrong today

The failure classes, read against the code and the schema:

- **The data already describes behaviour, and reading it costs nothing.** From
  messages alone, deterministic SQL can tell apart the shapes that matter:
  who writes more and longer, who starts conversations, how much is group
  versus one-to-one, how fast replies come, when contact began, peaked and
  stopped. None of it needs a model.
- **Nothing records who someone is to you.** `relationship_category`, `nickname`, notes and
  `bond` have almost no code that writes them, so they are almost always empty.
  The page badge defaults to "Contact" for everyone, however close. The
  only deterministic relationship label that will exist is the iOS contact
  card's related names, which is in flight on wave.
- **The articles are guessed and go stale.** Person articles are written for
  the top people by all-time volume, so someone central now can have none.
  Their prose carries relationship words (kinship, romance, "likely") that
  no row stores. `input_fingerprint` is never set, so no article can be
  found stale, and none is ever revised. An article can be written before
  most of the records about its subject exist.
- **Revision gets almost no input and is disabled by default.** The editor applet ships
  `default_enabled = false`. When it runs, an entity revision is handed one
  line ("New records reference this subject. Search for them.") and an agent
  on the Standard slot with `web_search`, capped at $0.60 a run. Any new
  message moves the fingerprint, so the people you talk to most would get the
  most expensive treatment, monthly. The colophon says "keeps it current"
  while nothing does.
- **Identity resolution has errors, and every later layer inherits them.** A
  handle on two contact cards resolves to nobody, so a frequent
  correspondent's messages can go unattributed. `handles` is written only
  when a contact card *creates* a person. The owner's group messages carry
  no person ref. One-way email senders, many of them bots, are minted as
  people with no message ref.
- **Per-person stats are computed on the fly in several queries**, each with
  different filters (tapbacks in or out, roles, UTC or local dates). Each
  prose consumer sees a different subset of the facts about a person. The chat
  @-mention fetch reads `content`, a column with no writer.
- **Every history has one trap:** a long stretch with no messages from
  anyone, from an import gap or a lost device. A naive "longest silence"
  reports it for every long-standing person. The ledger counts silence only in
  days when the box recorded other activity.

## Design

A person page is four layers, and each layer has exactly one kind of author.
Most of what has gone wrong came from a layer being written by the wrong one:
relationship labels guessed in prose, a ranking by message volume used as a measure of
importance, a revision agent asked to rediscover what SQL already knows.

| Layer | Says | Author | Tokens |
|---|---|---|---|
| Identity | which handles are this person | deterministic, or you | 0 |
| The ledger | the shape in time: first, last, rhythm, who starts, silences | SQL | 0 |
| Meaning | who they are to you, and when | you, or your contact card | 0 |
| The account | what happened, in order, in words | the model, when you ask | paid once per closed month |

Two rules keep it cheap and accurate.

**Time-dependent facts render at read time.** Generated prose never describes
the present. "Last message
<date> — <how long ago>" is rendered on the page from the
ledger at read time, like a name resolving live behind a pill
(`agents/record/names.md`). The account covers only *closed* periods. An
article written this way cannot go stale by the clock passing; it can only be
incomplete, and it is incomplete only when a new period closes. That is the
only event that triggers maintenance. It is also how "a constant presence" stops being
written about someone you last heard from over a year ago: the prompt has no slot
for the present, instead of an instruction forbidding it (memory: prompt slots,
not prohibitions).

**Each message is read by a model once.** The model reads each closed month of a person's
messages once and leaves a short dated account of it, the way a day is narrated
once and never re-narrated. Everything after that — the article, its
revisions, a chat question about the person — reads those accounts, not the
messages again.

### 1. Identity

This is `identity-plan.md`, and it comes first: a ledger built over the wrong handles
produces a wrong page that looks confident. The parts that matter here:

- Write `handles` on contact merge, on `update_person`, and for every minted
  person (`contacts.rs:613-622` never touches it today).
- When a handle is on two cards, ask you about it instead of silently
  attributing it to nobody. Show the question on both pages: "This number is on two of your contacts."
- Stop minting people from one-way email. A person is created on two-way
  contact or from a contact card.
- Attribute your group messages through `thread_id`, so "you wrote to them" also
  counts group messages.

### 2. The ledger

One table with one writer, read by every consumer. A row per person per month:

- messages each way, 1:1 and group separately, characters each way
- conversations started by each side (first message after a 6-hour gap)
- reactions given and received, by kind; attachments
- messages between 22:00 and 04:00 local time
- first and last message time in the month
- calendar co-attendance (past events only — recurrences can run decades ahead)

Plus one box-wide row per month (the denominator: was anything recorded at
all). Built incrementally on the resolver tick for months whose refs moved;
full rebuild is seconds. It replaces the ad hoc queries (gloss, firsts,
`arc`, `wiki_first`, circumstances, day memory, list, novelty) and is the
`<ledger>` every prose consumer is handed.

The page shows facts from it and no scores: the span, the last
message, a quiet rhythm of months. No rank, no tie strength, no "inner
circle" — message volume does not measure importance (`circumstances.rs` already orders
by recency for this reason). Derived states such as "quiet for eight times its usual
gap" exist only to trigger work, and are never displayed as labels.

### 3. Meaning

Only you, or your own contact card, set who someone is. The software's job is
to ask a good question.

- **The contact card's related names** fill the relationship deterministically
  when the in-flight capture lands. Those are your words, typed into your
  phone.
- **The page asks a question based on the ledger, in place of the "Contact" badge.**
  "You and Nick wrote to each other most days for two years, until last
  spring. Who is Nick to you?" Then plain choices (family, partner, former partner,
  friend, work, someone else) and a line for the bond. For partner and work, an
  optional *from* and *to*. A model may phrase the question; it never supplies
  the answer, and no choice is preselected.
- **It asks only on the page you opened.** There is no queue and no digest of
  questions: the asks system was rejected, and opening someone's page is the
  moment you are thinking about them.
- **The answer is a fact with a time range.** "Partner, from one month to
  another" stays true forever. A relationship that changes gets a second range;
  the first is closed, never overwritten. This is the one schema addition the
  plan needs, and it replaces `relationship_category` (read in eight places,
  written by nothing).
- **Editing meaning revises the article.** `subjects.rs` has
  `authored_fields: false` for people today, so a bond edit never reaches the
  prose.

Every other surface takes "who someone is" from this layer only. The day
writer currently reads 900 characters of the machine-written article as "From
their wiki page" and treats it as established fact. That is how one guessed word in an
article would reach every later day page.

### 4. The account

Written only when you ask for the article. Your request stays the only trigger.

**Month notes.** For each month with contact, one Lite call reads that month's
messages with the person, in order, both directions, including your group
messages in threads they are in. It writes a short dated account: what you
talked about, plans made, things that happened, and your own words where you
said how you felt or what you wanted. It cites the dates it draws from, and a
month that was only logistics says so in a line. A month is closed on the 1st
of the next. Its note is final unless late-arriving records move that month's
fingerprint (an import of old history). Months should fit whole on Lite almost always; only a peak
month is trimmed, by keeping conversation openers and the longest messages.

This differs from the moments pass that was rejected for day pages: it has no types,
scores or entity extraction. It is prose a model writes reading the actual
words, then hands to another model, the same shape as day prose.

**The article** is one call that reads the month notes in order, the ledger,
and the meaning facts. It is written in chronological order: how it began, what it was at its
height, how it changed or ended, and what you said about it. Why it worked or
didn't goes in only in words one of you said, with the date. It ends at the
last closed month. It does not describe the present, because the page renders
the present above it.

**Revision** is the same one call, handed the current article, your own
sentences (`theirs`, enforced by `check_edit` as now), the month notes since
the last edition, and the ledger. No agent loop, no `web_search`, no "search
for them." It runs when one of these happens, at most once a month:

- a month with contact closed since the last edition
- the relationship went quiet or resumed (a ledger state change)
- you edited the meaning or the bond

A person you last heard from two years ago costs nothing until a new message
arrives.

## Cost

These are estimates. The gateway prices at runtime, so measure on an
invented corpus before quoting anything.

| Work | Tokens | Cost |
|---|---|---|
| Ledger, every person, every month | 0 | 0 |
| Month notes for a heavy correspondent, read once | a few hundred thousand in | about a cent on Lite, once |
| Article from those notes | tens of thousands in | well under a cent |
| Upkeep for one active maintained person | ≤2 Lite calls a month | under a tenth of a cent |
| Upkeep for a person gone quiet | 0 | 0 |
| Today's design, if enabled: one active person | a Standard agent run a month | up to $0.60 |

Reading everything a person ever sent, once, on Lite costs about a cent. In
today's design the cost comes from re-reading: an agent on Standard
rediscovering the same history every month, and chat retrieval spending many
tool calls on any question that names a person.

Chat retrieval is the larger saving. Embedded month notes make "what did Nick
and I talk about last spring" one search hit, not a dozen tool calls.

## Out of scope

- Infer a relationship. Not romance, not family from a shared surname, not
  an ending from a silence. The ledger informs the question, and you answer it.
- Display a score, rank or layer for a person, or order people by volume
  anywhere they can see it.
- Compute importance deterministically. The model judges what mattered from
  the words, with the ledger facts as input.
- Build a whole-wiki view of people: no cast list, relationship graph or
  co-occurrence figure. This is one person's page.
- Link transcript mentions to people by name. Many transcript mentions are
  a first name only. That waits on identity.
- Write an article nobody asked for.

## Slices

1. **Identity repairs. Built:** two-way contact
   makes a person (contact card, meeting, or you wrote to them); automated
   addresses never do; email you sent resolves its recipients and back-links
   what they sent before you answered; `handles` derived from addresses and
   numbers for everyone; `merge_people`, with "Same as…?" on the person page
   for anyone sharing a number or address, and old ids opening the survivor.
   Left:
   - removing the people the old rule minted: those with no messages, no
     meeting, nothing you wrote to them and no page edits (runs after the
     release that carries the rule);
   - the contact card's substring phone match in `contacts.rs`;
   - two merged cards both sync their photo, so the picture follows
     whichever synced last until one card goes or you choose one.
   Your group messages through `thread_id` moved to slice 2: the ledger needs
   them, and as refs they would put every group message you sent on every
   member's page.
2. **The ledger table** and one reader. Move the ad hoc queries onto it in
   the same change. Render the live line on the person page. Count your
   group messages per member through `thread_id`.
3. **Meaning**: the relationship-with-range table (claim a migration), contact
   card related names into it, the grounded question on the page,
   `authored_fields: true` for people, and every prose consumer reading meaning
   from this layer only. Delete `relationship_category` reads and the "Contact"
   default.
4. **Month notes** on Lite, backfilled when an article is requested. Store
   with a per-month fingerprint and embed for search.
5. **Article and revision** as one call over notes, ledger and meaning, with
   event triggers. Remove the entity handover's agent and `web_search` path;
   revision moves to Lite as it leaves the editor applet (whose Standard slot
   stays for years, chapters and stories). Fix the colophon so it claims only
   what runs. Re-measure variety by counting distinct opening sentences across
   all articles.
6. **Chat**: questions that name a person read meaning, the ledger line, the
   article and the relevant month notes first.

## Decided

- **First article on Standard, upkeep on Lite** (owner, 2026-10-10). First
  drafts are on Standard.
- **The existing person articles are regenerated** under the new first draft so
  the change can be read side by side (owner, 2026-10-10).
- **Resurfacing is out of scope.** What a page about a relationship that
  ended should do on day pages and "on this day" is its own
  design, later.

## Open for the owner

1. **Quoting the other person.** Day pages paraphrase others and never quote
   them in hard conversations. A person page about a relationship that ended
   is that kind of hard conversation. Proposal: quote your own words freely; paraphrase theirs
   unless one of their lines was the key moment.
