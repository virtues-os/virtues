# Getting started: the founder's letter and the Home page

> Cut the four-step onboarding down to the founder's letter, and rebuild Home so
> that "getting started" is what Home looks like before its first-run sections
> retire. Home is one page, designed once. Written 2026-08-31.
>
> This is the layer *above* [onboarding-plan.md](onboarding-plan.md), which
> covers setup — pairing, accounts, the airlock. Setup ends when the SPA opens;
> this plan is everything after.

## Why

Three facts, all discovered by reading the code, drive the shape:

**The reveal step runs before there is any data to show.** The "Meet
yourself" step (`RevealSection.svelte`) renders `/api/census` counts at the one
moment they are guaranteed to be thin. Nothing kicks when a source is
connected — a fresh OAuth source produces its first rows on the next cron slot,
15–30 minutes later; people and places resolve on a 15-minute tick; and the day
narration (SEGMENT → NARRATE) only runs on *completed* days at the box's
maintenance hour, draining backlog at about one day per hour. The payoff of
connecting your life is inherently asynchronous. A page the user passes through
once cannot show it; a page the user returns to can.

**We have three partial getting-started surfaces and no complete one.** The
onboarding sources step (`ConnectWorld.svelte`), `MobileOnboarding.svelte`
mounted separately in the app shell, and `setupState.svelte.ts` — a store that
polls `GET /api/setup/state` every 60s, tracks `first_source` / `first_device`
/ `first_sync` / `remote_access`, and is rendered *nowhere* (only the
remote-access toast consumes it). The store was clearly built for the page this
plan describes.

**Home is weak, and this work rebuilds it.** Instead of adding a first-run
layer to a page we don't like, the getting-started work is the Home rebuild:
the lifeline is the main element of Home from the first minute, and the
getting-started material is a set of sections that each retire on their own.
The page never switches modes; sections disappear one at a time.

## Decisions (settled in discussion, 2026-08-31)

- **Onboarding = the founder's letter, full page, Continue at the end.**
  Introductions, sources, and the reveal steps are demolished. The letter still
  states the commitments, and nobody has to get through steps they may want to
  skip before reaching their data.
- **Nobody signs anything.** Introductions is not folded into the letter as a
  signature — it becomes the top section of the getting-started page.
- **No gating on sources.** Sources was always skippable; the empty-box path
  must remain first-class (DIY, chat-import-only).
- **No trait inference, anywhere, including here.** No Big 5, no
  Myers-Briggs, no machine-written portrait. The observed-data portrait was
  deleted on purpose (2026-08-26): the machine writes narrative identity only
  while it is empty, and the document is the user's own words thereafter.
  The significance rule says the user decides what is significant; the system
  never infers it. What the page may say about the *person* is limited to
  deterministic observations about their *data* — earliest name, the span, which lane runs deepest. The portrait
  comes out of the interview, in the user's words.
- **One dismissibility rule for every section:** sections that *ask*
  retire when answered (or are dismissible); sections that *show* retire on
  their own when they have nothing left to say. This is Home's existing rule —
  "a section with nothing to say does not render" — run forward in time.
- **There is no "finished getting started" state.** There is no
  `getting_started_complete` flag, no mode switch, nothing to migrate later.
  When the asking sections have retired, what remains is Home: the lifeline,
  the day, and the user's data.
- **"Onboarding" left the visible vocabulary (2026-08-31).** The letter lives
  at `/founders-letter`; `/onboarding` (and its old step URLs) and `/setup`
  redirect there. Wire names — `onboarding_status`, `skip-onboarding`, the
  `(onboarding)` route group, the components directory — keep their spelling
  until a protocol/rename sweep is worth doing on its own.
- **While any section remains, getting started IS the page.** Home's
  furniture — subtitle, day stepper, deck, recents, the keep — exists only
  after every section has retired. A middle state that rendered the remaining
  sections quietly on top of Home was built and removed the same day: it looked
  like Home with getting started bolted on, which is the problem this plan
  exists to prevent.

## The page, top to bottom

Each section names its retire/dismiss rule. Order is also priority. The page should mostly be prose with a few questions
in it; it should not look like a checklist.

**1. Introductions.** Who are you — the content of today's onboarding
`Introductions.svelte`, ported. Retires the moment it's answered. Not
dismissible, but it is one small question, not a form.

**2. Connect your world.** `ConnectWorld.svelte` ports over nearly intact —
sources, devices, the BLE choreography. Retires on the first source or device
win (`setupState`). Dismissible: "not now" removes it and it never comes back as a reminder; the same
sources stay reachable in Settings. (`MobileOnboarding.svelte` stays: on
reading it, it is the phone's *native OS-permission* flow — Tauri plugin
invokes — not a duplicate of this section.)

**3. The lifeline.** Never retires — it is the spine of the new Home. Drawn
from `GET /api/wiki/lifeline`, which already buckets every timestamped
`data_*` table into lanes server-side (indexed, `width_bucket`, 2,000-bucket
cap) and reports **`first_seen` per lane** — which is the distinction the
drawing needs: "nothing happened" is drawn differently from "nothing was
collecting yet." Art direction: hand-drawn / notebook, a line that visibly *accretes* —
sparse left edge, thickening right edge, the user's own dates and earliest
names as marginalia. The census observations (oldest thing, span, deepest
lane) live as annotations on this drawing, not as a stats table. Load the
`frontend-design` skill before building this; serif never bold.

**4. What arrives when.** The real schedule, replacing the old "What happens
next" prose with what the pipeline actually delivers and when: transcripts within minutes
(2-minute cron), people and places within the quarter hour (entity resolver
tick), search shortly after (embedding cron), your first day written up
tomorrow morning (maintenance hour, default 4am local; backlog drains ~one day
per hour). Self-retiring by design: each line disappears when that data
arrives, and the section is gone within a day on its own. Not dismissible
because it dismisses itself. This section also gives the user a reason to come back
tomorrow, which matters more than any single impressive moment.

**5. Your first day page.** Appears only once the first narrated day exists
(`narrated_at`): "your first day is written up," pointing at the day page.
Retires once opened — after that, days are just Home's normal content.
Self-retiring; it confirms the last line of section 4.

**6. Your first conversation.** A deep link into the narrative interview chat
(`chat_narrative_interview`). This link does not exist today: the old
reveal *mentions* the waiting conversation but never points at it. Retires
once the user has said anything in the interview (the same signal ChatView
uses to grow "Write it up").

**7. Enrichment.** Create your first applet; learn about notebooks; whatever
we add next. Individually dismissible, never gating, and open-ended: this
is where new getting-started items get added. Start with two or three rows,
not ten.

Sections 3 plus whatever Home already does well (the day components —
`DayDeck`, `DayGround`, `DayNovelty` — rehomed under the lifeline spine) are
the permanent page. Deep redesign of the day sections themselves is *out of
scope* here; this plan rebuilds Home's frame and first-run life, and the day
content iterates inside it afterwards.

## Demolition and rewiring

- `steps.ts`: `STEPS`/`VIEW_ORDER` collapse to the letter. `Introductions`,
  `ConnectWorld` move to `lib/components/home/` (or a `getting-started/`
  sibling); `RevealSection.svelte` and the reveal branch of
  `(onboarding)/onboarding/[[view]]/+page.svelte` are deleted, along with the
  `AccountGate` fallback that rode on the reveal step (real account/pairing
  gating lives in setup, not here).
- `OnboardingHeader.svelte`: the four-icon step strip goes; the letter needs at
  most a wordmark.
- The letter's Continue calls the existing `enterApp()` path
  (`skipOnboarding(true)` → `goto("/")`). `onboarding_complete` now means "has
  passed the letter" — the `(app)/+layout.ts` redirect keeps working unchanged.
  Boxes mid-onboarding when this ships simply find themselves past it; nothing
  to migrate.
- `setupState.svelte.ts` becomes load-bearing: the retire signals for sections
  2 and 4 read from it (plus a lightweight "interview has a user message"
  signal for section 5). Extend `GET /api/setup/state` if a needed win isn't
  tracked; do not invent a parallel store.
- `/api/census` survives as the source of the lifeline's marginalia numbers,
  but see fixes below.

## Fixes to make along the way

These are real defects the redesign will otherwise inherit:

- **"Days written up" overstates.** `census.rs` counts rows in `wiki_days`,
  and `get_or_create_day` inserts a stub row from merely *viewing* a day page.
  Count `narrated_at IS NOT NULL` instead.
- **Census swallows errors into zeros.** Missing table (`42P01`) → 0 silently;
  any other error → warn and report 0 anyway. That is the swallowed-query
  bug in another form. On the page that introduces a person to their own
  data, a broken query must not look like "you have nothing."
- **Census cost.** ~27 sequential uncached `count(*)` queries per load. Fine
  for a page seen once; not fine for the homepage. Either cache briefly or
  derive the marginalia from the lifeline response, which is one query per
  lane and already consumed by Home.

## Build order

1. **Letter-only onboarding.** Collapse `steps.ts`, rewire Continue, delete
   the step strip. Onboarding is done shrinking before anything new exists —
   this slice is shippable by itself and the old reveal simply never renders.
2. **Home frame.** Rebuild `HomeView` as spine + sections with the retire rule
   as an actual mechanism (each section declares its own "nothing left to say"
   predicate). Rehome the existing day components inside it.
3. **Lifeline drawing.** The only new component. `getLifeline()` is
   already consumed by Home, so this is rendering work: lanes, `first_seen`
   edges, accreting ink, marginalia from census. Design-heavy; prototype
   before polishing.
4. **Port the asking sections.** Introductions and ConnectWorld into their sections;
   wire retire/dismiss to `setupState`. (`MobileOnboarding` stays — it is the
   phone's native OS-permission flow, not a duplicate.)
5. **Schedule and interview link.** Section 4's lines keyed to real
   signals (first transcription row, first `wiki_people` row, first
   `narrated_at`), section 5's deep link and retire signal.
6. **Census fixes + enrichment rows.** The three fixes above; then the first
   two enrichment items.

Slices 1 and 2–3 can proceed in parallel; 4–5 depend on 2.

## Dismissibility (settled 2026-08-31)

Every asking section (introductions, connect-your-world, interview, enrichment) is
individually dismissible, including introductions — a checklist item you can't
say no to is just a nag. Showing sections (schedule, first-day page) are
self-retiring and need no dismissal. The lifeline is not dismissible because
it is a permanent part of Home. Two rules stayed fixed:

- **Dismissed sections are removed entirely.** No residue rows; the features stay
  findable where they permanently live (Settings → Sources, the interview in
  the sidebar).
- **No visible "skip getting started."** A user who dismisses the asking sections has
  dismissed everything that can be dismissed; everything else leaves on its own. The one escape
  hatch is the same *hidden* dangerously-skip door onboarding has today —
  tucked away, for people who know to look.

Dismissal state lives server-side (per box), so sections disappear identically on
every device.

## Open questions

- Whether the letter also carries the "what is this thing" privacy posture
  currently spread across onboarding copy, or whether that's a docs link.

## When to delete this plan

When the letter-only onboarding and the new Home ship, delete this plan.
What remains: a record of the rebuild (why the reveal was removed, the retire rule),
and manual pages for the getting-started sections users actually see.
