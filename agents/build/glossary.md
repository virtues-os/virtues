# Glossary — the timeline vocabulary

This page defines the words we use for time in a person's life. Anything the
timeline drawing can show needs a name here, and each thing has exactly one
name, so schemas, prompts, UI copy, and docs use these words and no synonyms.
The reasoning behind these choices is in the timeline artifact ("The
Lifeline"); this page lists only the decisions.

## The three axes

Every item in the timeline can be placed on three axes.

| Axis | Question it answers |
|---|---|
| **Time** (granularity) | how long a span is — from a single timestamp to a whole life and beyond |
| **Tense** | what we do with it — record it, use it as context, or check it later |
| **Subject vs. attachment** | whether it has a place on the timeline at all |

## Temporal primitives

A **temporal primitive** is one value on the time axis: a kind of span that can
be a subject in its own right. The table lists them from shortest to longest
and says whether each gets its own page. The rule: **a primitive gets a page
only if it says something the primitive below it cannot.**

| Primitive | What it is | Gets a page? |
|---|---|---|
| infinitesimal | a record's timestamp | no — it is a raw data point |
| session | one continuous observation (a visit, a recording) | no — sessions are evidence used to build events |
| event | a bounded stretch of a day | no — an event is one line on a day page |
| **day** | the smallest span people tell stories about | **yes** — the basic unit of the day pages |
| week | a recurring routine; people rarely describe a week as a story | no |
| month · quarter | a calendar range used for filtering | no |
| **year** | the other span people commonly tell stories about | **yes** |
| **chapter** | a partition of the life — 5–10 in view at any age | **yes** — named by the person |
| **story** | a throughline at any scale — narrative or investigative | **yes** — declared by the person |
| **life** | who this person is | **yes** — exactly one |
| ∞ (telos) | the person's long-term purpose | no — it is a direction, so it has no page |

- **Chapter** is a *partition*: contiguous, exclusive, tiles the life. It
  answers "where am I right now." The system can detect likely boundaries
  from changes in the data; the person names the chapters. The partition is
  **normalized**: a life is shown as 5–10 chapters at any age. New chapters
  open at the front and older ones merge behind (university and the first job
  become "the twenties"), so older periods get coarser and the count stays
  small enough to read. The system offers a merge to the person; it never
  merges chapters silently.
- **Story** is a *throughline*: it can be non-contiguous, can overlap other
  stories, and can resume after a pause, at any scale. Examples: an afternoon
  (the recital), a year (learning to juggle), a trait (learning patience). A
  story can also be a *question*: an investigation through the person's own
  data, like the ones a project contains ("why do I get sad at work
  sometimes"). The person declares a story; the system never infers one.
- A day's page is written once on its own, when the day closes, and again
  only when the person asks (Rewrite this page). Years and chapters are
  revised by the wiki editor, when it is on, as the days beneath them are
  written, a rewritten day included. The live maintenance load is four
  time-articles at any moment, whether the person is twenty or eighty.

## Tense

Time says how long a thing is. **Tense** says what the system does with it.

| | Past | Present | Future |
|---|---|---|---|
| Goal | fidelity | context | commitment |
| The work | resolve what happened | ground what to say now | declare what should |
| Artifact | the record — events, days, chapters | the **brief** — assembled, never stored | plans and **commitments** |
| Can it be wrong? | yes — so it must cite | no — only stale | not yet — graded when now arrives |
| Fails by | confident wrong facts | a stale core | a promise never revisited |

- **Only claims about the past can be wrong.** A claim about what happened
  can be checked, so it must cite the evidence it rests on. Citations are how
  the past tense is built.
- **The present is assembled on demand and never stored.** A person can write
  about the past and declare plans for the future, but nobody writes the
  present. The **brief** is the one present-tense page: where the current
  chapter stands, what is open, what has stood out recently, and what is
  worth asking. It is derived from settled data, read during the day, and
  never revised.
- **Future items are grouped by what closes them, not by how long they
  last.** Their purpose is to allow a comparison later: when a planned date
  arrives, the system can check whether it happened. All four kinds reduce to
  one primitive, the **commitment**: a body, a horizon, and three ways to
  close (*done*, *abandoned*, *superseded*). An **aspiration** is not a
  future-tense item at all. It never starts or finishes; it is part of the
  life page, as a direction.
- Two rules. First, the **task** list (Home > Tasks, `app_tasks`) is where
  commitments become visible, and it never nags: no badges, no counts, no
  reminder the person did not ask for. A commitment that slips is just data
  about the person. Second, the system never offers a grade unprompted (it
  makes the comparison available and does not comment on it).

## Subjects and attachments

Some things with a page have no place on the timeline.

| Kind | What it is | Its tense |
|---|---|---|
| Subject · time-shaped | an interval that can hold a story | computed from the interval |
| Subject · entity-shaped | someone or somewhere in the record | none — a person is not past or future |
| Subject · thread-shaped | a throughline with a magnet — a story told, a question investigated, a commitment | open until closed |
| Attachment · claim | an unsettled observation, pending a person | whatever it cites |
| Attachment · citation | record R mentions subject S at time T | the referring record's own |
| Attachment · rule | a standing constraint that outranks context | none — rules do not expire |

- The temporal primitives are the time-shaped subjects, listed in order.
  Entities are not on the time axis at all. A day and a person are the same
  *kind* of thing: a subject that can have a page.
- **A subject counts only if other things can cite it.** Storing it is not
  enough. A story that cannot link to the events it is built from is only a
  caption; it makes no checkable claim.

**"Entity" is one of the three shapes. Never use it as a synonym for
"subject."** The two words both appear in the schema and mean different things
there:

| column | what it holds | why |
|---|---|---|
| `wiki_articles.subject_type` | all eight subjects | anything with a page |
| `wiki_refs.entity_type` | person, place, organization | a record can only POINT AT an entity |

A record cannot reference a year. A year is a partition we impose; it is not
something an email mentions. So the two lists are different on purpose, and
aligning them — which looks like obvious tidying — would be wrong.

The registry in
[`virtues-core/src/api/subjects.rs`](../../virtues-core/src/api/subjects.rs) is
this section in code: one row per subject, and everything that varies by kind
hangs off it.

## Project

A **project** is a workspace over the person's data: material gathered
around a question, with chat grounded in that material instead of in
everything. It is not a subject: it has no place on the timeline and gets no
article. In code it is `app_projects`, its contents are
`app_project_items`, a chat is bound to one by `app_chats.project_id`, the
API is `/api/projects`, and the routes are `/projects` and `/project/{id}`;
ids keep the `nb_` prefix. A project cannot contain a project. It was
**Notebook** until 2026-09-21, so an older commit message or record that
says notebook means this.

## Provenance

Every span of text or drawing carries exactly one provenance.

| Provenance | Meaning |
|---|---|
| **observed** | sensors recorded it; corroboration is countable |
| **authored** | a person named or wrote it |
| **generated** | a model wrote it — the system's own output, never sole warrant for a claim |
| **intended** | it has not happened yet — calendar, plan, commitment |

## Gaps

**Never write "gap" without a qualifier.** There are four kinds, they mean
different things, and different actors close them.

| Gap | What is missing | Closed by |
|---|---|---|
| **unobserved** | the record itself | a source — or acceptance |
| **unnamed** | a chapter over real record | the person — the drag-and-name gesture |
| **uncitable** | the join from a story to its events | the system — let a story cite what it recruited |
| **unplanned** | anything past the horizon | a commitment, or nobody if the person chooses; an unplanned gap is information and not a failure |

The timeline always draws gaps and never hides them. Each gap prompts the next
step: observation creates unnamed gaps, naming them creates chapters, chapters
provide context, and that context makes the brief useful.

## Prudential context

These terms describe what a person actually remembers, and they set the
budget for what the box keeps in a model's context.

| Term | Meaning |
|---|---|
| **base context** | who I am — loaded always, at no distance |
| **cached context** | the last hours and days — full resolution, no act of recall |
| **salience** | what stays visible as the view gets coarser with distance: surprising or costly events; computed from data |
| **significance** | what stays visible longest: weight the person assigns; declared by the person, never inferred |

The timeline has two named projections. **The mind** is logarithmic: space is
allocated by distance from now, the way memory treats a life. **The record**
is linear: every year gets the same width, the way an archive stores it. Both
are valid, and switching between them is a visible toggle on the timeline, not
a setting in preferences. The linear projection needs a **window**, a
resolution control that steps through life, decade, year, month, week, and
day. The logarithmic projection has no window, because it already shows a
whole life and a single afternoon in one frame; a linear scale cannot.

The same curve is used twice. Rendered, it gives the mind projection its
geometry (horizontal space shows attention, each band shows a resolution).
Written out as prose, it becomes the brief. Salience needs its own visual
encoding: we tried mapping it to height on the spine and it looked like noise,
so the spine is drawn at a uniform height and the encoding is still undecided.
The wording of a sentence must match its distance from now — "yesterday
you…", "last week you…", "in 2015 you…". A sentence at the wrong granularity
sounds wrong even when the facts are right.
