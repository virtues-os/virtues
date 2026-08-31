# Getting started — the letter, then the page

> Demolish the four-step onboarding down to the founder's letter, and rebuild
> Home so that "getting started" is what Home looks like before its scaffolding
> retires. One page, designed once. Written 2026-08-31.
>
> This is the layer *above* [onboarding-plan.md](onboarding-plan.md), which
> covers setup — pairing, accounts, the airlock. Setup ends when the SPA opens;
> this plan is everything after.

## Why

Three facts, all discovered by reading the code, drive the shape:

**The reveal is a ceremony scheduled before the orchestra arrives.** The "Meet
yourself" step (`RevealSection.svelte`) renders `/api/census` counts at the one
moment they are guaranteed to be thin. Nothing kicks when a source is
connected — a fresh OAuth source produces its first rows on the next cron slot,
15–30 minutes later; people and places resolve on a 15-minute tick; and the day
narration (SEGMENT → NARRATE) only runs on *completed* days at the box's
maintenance hour, draining backlog at about one day per hour. The payoff of
connecting your life is inherently asynchronous. A page the user passes through
once cannot show it; a page the user returns to can.

**We have three partial getting-started surfaces and zero whole ones.** The
onboarding sources step (`ConnectWorld.svelte`), `MobileOnboarding.svelte`
mounted separately in the app shell, and `setupState.svelte.ts` — a store that
polls `GET /api/setup/state` every 60s, tracks `first_source` / `first_device`
/ `first_sync` / `remote_access`, and is rendered *nowhere* (only the
remote-access toast consumes it). The store was clearly built for the page this
plan describes.

**Home is weak, and this is the vehicle for rebuilding it.** Rather than adding
a first-run dress to a page we don't like, the getting-started work *is* the
Home rebuild: the lifeline becomes the spine of Home from minute one, and the
getting-started material is a set of sections that individually retire. The
page never switches modes; it sheds.

## Decisions (settled in discussion, 2026-08-31)

- **Onboarding = the founder's letter, full page, Continue at the end.**
  Introductions, sources, and the reveal steps are demolished. The letter keeps
  the covenant without holding anyone's data hostage behind steps they may
  want to skip.
- **Nobody signs anything.** Introductions is not folded into the letter as a
  signature — it becomes the top section of the getting-started page.
- **No gating on sources.** Sources was always skippable; the empty-box path
  must remain first-class (DIY, chat-import-only).
- **No trait inference. Ever, but especially not here.** No Big 5, no
  Myers-Briggs, no machine-written portrait. The observed-data portrait was
  deliberately deleted (2026-08-26): the machine writes narrative identity only
  while it is empty, and the document is the user's own words thereafter.
  Salience doctrine says significance is user-sourced, never inferred. What the
  page may say about the *person* is limited to deterministic observation about
  the *record* — earliest name, the span, which lane runs deepest. The portrait
  comes out of the interview, in the user's words.
- **One dismissibility rule, not per-item judgment calls:** sections that *ask*
  retire when answered (or are dismissible); sections that *show* retire on
  their own when they have nothing left to say. This is Home's existing rule —
  "a section with nothing to say does not render" — run forward in time.
- **Graduation is not a state.** There is no `getting_started_complete` flag,
  no mode switch, nothing to migrate later. When the asking sections have
  retired, what remains is Home: the lifeline, the day, the record.

## The page, top to bottom

Each section names its retire/dismiss rule. Order is also priority: the page
should read as an essay that happens to have some questions in it, not a
checklist with a nice font.

**1. Introductions.** Who are you — the content of today's onboarding
`Introductions.svelte`, ported. Retires the moment it's answered. Not
dismissible, but it is one small ask, not a form.

**2. Connect your world.** `ConnectWorld.svelte` ports over nearly intact —
sources, devices, the BLE choreography. Retires on the first source or device
win (`setupState`). Dismissible: "later" collapses it to one quiet row for the
chat-import-only or DIY-minimal user. It never nags. `MobileOnboarding.svelte`
is deleted; phone renders the same section.

**3. The lifeline.** Never retires — it is the spine of the new Home. Drawn
from `GET /api/wiki/lifeline`, which already buckets every timestamped
`data_*` table into lanes server-side (indexed, `width_bucket`, 2,000-bucket
cap) and reports **`first_seen` per lane** — exactly the distinction the
drawing needs: "nothing happened" is different ink from "nothing was watching
yet." Art direction: hand-drawn / notebook, a line that visibly *accretes* —
sparse left edge, thickening right edge, the user's own dates and earliest
names as marginalia. The census observations (oldest thing, span, deepest
lane) live as annotations on this drawing, not as a stats table. Load the
`frontend-design` skill before building this; serif never bold.

**4. What arrives when.** The honest schedule, replacing the old "What happens
next" prose with the pipeline's real promises: transcripts within minutes
(2-minute cron), people and places within the quarter hour (entity resolver
tick), search shortly after (embedding cron), your first day written up
tomorrow morning (maintenance hour, default 4am local; backlog drains ~one day
per hour). Self-retiring by construction — each line disappears as its promise
lands, and the section is gone within a day on its own. Not dismissible
because it dismisses itself. This section is also the reason to come back
tomorrow, which matters more than any single wow.

**5. Your first conversation.** A deep link into the narrative interview chat
(`chat_narrative_interview`) — the link that today does not exist: the old
reveal *mentions* the waiting conversation but never points at it. Retires
once the user has said anything in the interview (the same signal ChatView
uses to grow "Write it up").

**6. Enrichment.** Create your first applet; learn about notebooks; whatever
we add next. Individually dismissible, never gating, additive forever — this
is the extensible tail the page exists to carry. Start with two or three rows,
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
  disease with extra steps. On the page that introduces a person to their own
  record, a broken query must not read as "you have nothing."
- **Census cost.** ~27 sequential uncached `count(*)` queries per load. Fine
  for a page seen once; not fine for the homepage. Either cache briefly or
  derive the marginalia from the lifeline response, which is one query per
  lane and already consumed by Home.

## Build order

1. **The letter stands alone.** Collapse `steps.ts`, rewire Continue, delete
   the step strip. Onboarding is done shrinking before anything new exists —
   this slice is shippable by itself and the old reveal simply never renders.
2. **The frame.** Rebuild `HomeView` as spine + sections with the retire rule
   as an actual mechanism (each section declares its own "nothing left to say"
   predicate). Rehome the existing day components inside it.
3. **The lifeline drawing.** The one genuinely new build. `getLifeline()` is
   already consumed by Home, so this is rendering work: lanes, `first_seen`
   edges, accreting ink, marginalia from census. Design-heavy; prototype
   before polishing.
4. **Port the askers.** Introductions and ConnectWorld into their sections;
   wire retire/dismiss to `setupState`; delete `MobileOnboarding`.
5. **The schedule + the interview link.** Section 4's lines keyed to real
   signals (first transcription row, first `wiki_people` row, first
   `narrated_at`), section 5's deep link and retire signal.
6. **Census fixes + enrichment rows.** The three fixes above; then the first
   two enrichment items.

Slices 1 and 2–3 can proceed in parallel; 4–5 depend on 2.

## Open questions

- Where dismissed-section state lives (per-box server state vs. local). Server
  feels right — the page should shed the same way on every glass.
- Whether the letter also carries the "what is this thing" privacy posture
  currently spread across onboarding copy, or whether that's a docs link.
- How loud section 5 should be for a user who never wants the interview —
  retire-on-dismiss may need to apply there too.

## Death condition

When the letter-only onboarding and the shedding Home ship, this plan dies.
What survives: a record of the rebuild (why the reveal died, the retire rule),
and manual pages for the getting-started sections users actually see.
