# The Day Page

The day page is an account of one day that the system writes and the person
reads, usually in the evening. The person can correct it, annotate it and
restyle it, and over months the pages add up to a detailed account of their
life.

---

## The Four Questions

The day page answers four questions about a single day. Each question has a different temporal scope, different data requirements, and different implementation maturity.

| # | Question | Scope | Data Needed | Status |
|---|----------|-------|-------------|--------|
| Q1 | **Coverage** — How complete is today's data? | Today, static | LLM-assessed data quality rating | Planned (replaces W6H weights) |
| Q2 | **Entropy** — How ordered or chaotic was this day? | Today vs 12-week history | Per-event novelty (`novelty_z` + `local_novelty_z` on `wiki_events`, local embeddings) plus a live rhythm strip for the day in progress. | Built (per-event + live rhythm). The old cross-day `chaos_score` was cut. |
| Q3 | **Narrative Shape** — What happened throughout the day? | Today, temporal (event-time) | LLM-identified events with labels, times, locations | Built. The day line + the event timeline, in Data. |
| Q4 | **Alignment** — Is this day's shape conducive to who I want to become? | Today vs aspiration | Narrative identity document + comparison mechanism | Not built. See "Why Alignment Is Hard" below. |

---

## Page Layout (top to bottom)

This is the layout the page was designed from; the page as built differs
(Article and Data views, `DayPage.svelte`). §4 and §10 describe what is
built.

### 1. Navigation Bar (sticky)

- **Yesterday / Tomorrow** text links at edges (e.g., `← Thu, Feb 12 ... Sat, Feb 14 →`)
- **Calendar icon** in center → opens month-grid date picker
- Kill the current week picker

### 2. Pencil Sketch (chapter header illustration)

- Full-width, ~120-160px tall, above the title
- **Topical, not narrative**: highest-novelty concrete noun from the day (a bungalow, a kayak, a coffee cup on a patio)
- Not generated on days with no notable novelty; a day without a sketch is fine
- User picks global style in settings: pen & ink, watercolor, architectural marker, oil painting, pencil
- Uses space `accent_color` as single color accent against monochrome linework
- **Cost**: ~$0.04/image via DALL-E 3 or Midjourney API through Virtues Bridge. One per user per night = ~$1.20/month/user
- Nightly queue: scheduler fires after day summary, picks highest-novelty entity/noun, generates image, stores blob
- Can be turned off in settings. User can also customize the generation prompt
- **Decided 2026-10-07: no header picture.** A painting at the head of the
  page was tried in a specimen and rejected. What is built instead is a
  picture about once a week, of a public place or a generic setting a page
  names, set into its section as a figure, in the owner's chosen style
  (`api/day_picture.rs`).

### 3. Day Header

- Serif title: "Friday, February 13, 2026"
- Subtitle: relative badge ("Today", "Yesterday", "3 days ago") + timezone

### 4. The day line (built, in Data)

One instrument with no modes: no pills, no Sleep or Autonomic view, no
"heard N of 24 hours" bar. It is the first section of the day page's Data
view, headed "The day line" (`DaylineChart.svelte`, `lib/wiki/dayLine.ts`).

- **The day's own clock** across, local midnight to local midnight: a
  daylight-saving day is 23 or 25 hours wide, and an event that ends at
  midnight ends at the right edge. On today, a line marks now.
- **Each known event is a block** from its start to its end, as tall as it
  was unlike the owner's usual: above the line when unlike, below when more
  usual than usual, clamped at ±3. The score is `local_novelty_z`, else
  `novelty_z` (`usualScore`). Unknown stretches stay empty; an event nobody
  scored is an outline on the line.
- **Hover** gives the label, the times, and "Unlike your usual" (z ≥ 1),
  "Like your usual", "Your server hasn't scored this", or "Most unlike your
  usual" for the event furthest out, the one the event timeline below opens
  and badges.
- **Coverage lanes** under it, one thin lane per stream (Audio, Location,
  Messages, Health, Screen), filled where that stream recorded something,
  from the day's lifeline lanes. What was heard is read here.
- Sleep, heart rate and `autonomic_z` are not drawn.

Dimensions (a radar of user-defined axes) and a topology view are not built.

### 5. Autobiography (system voice)

The autobiography is the **meaning layer**: it synthesizes the day instead of retelling it in order. Event summaries already capture what happened and when (the fact layer). The autobiography's job is to surface what connected, what was unusual, and what the day was *about*. These serve different embedding targets: event concat answers "find days where I went to the noodle bar," the autobiography answers "find days that felt like this one" or "days where work and personal life collided."

**Tone**: Warm and precise. It must not sound like a surveillance report or a therapist's notes. Its length is justified only if it tells the user something they couldn't have written themselves: connections across data types, comparisons with their baseline, behavioral patterns found in the data.

**What good looks like** (epigraph: *The Trader Joe's detour*):

> The routine held until 4:12 PM, when you broke your usual commute to visit the Trader Joe's across town for the first time in your logged history. Your baseline Friday grocery run takes 18 minutes and averages $45. Today you lingered in the aisles for 52 minutes and swiped for $328.50. On the drive home, you sent a voice memo to Marisol: *"I just bought enough snacks to survive a winter."* Despite the heavy spend, your HRV that evening was 12% above your Friday baseline.

**Why this works** (the bar the system should clear):

1. **Most people will recognize it.** "Went in for milk, left with $300 of stuff" happens to almost everyone. The autobiography gives weight to a small anomaly instead of inventing a dramatic turning point. If the format works for an ordinary grocery run, it works for any day.
2. **One sequence of events across data types, with timestamps and numbers.** Location (first visit), financial (transaction amount + duration), audio (voice memo with specific quote), biometric (HRV deviation) — four ontologies in one paragraph, each tied to a number that could be verified.
3. **A baseline comparison the system actually computes.** "Your baseline Friday grocery run takes 18 minutes and averages $45" shows the system using the person's history, which it has and should use. It says so directly and does not pretend to be neutral.
4. **The user's own words are the anchor.** *"I just bought enough snacks to survive a winter"* is the line that, shown a year later as an "on this day" memory, brings the whole afternoon back. The system's only editorial choice is *picking the right line*; it does not write it.
5. **The last sentence states data and leaves the interpretation to the reader.** "HRV that evening was 12% above your Friday baseline" is a fact. The reader works out what the splurge did to them. The narrator never says "it settled you" or "it stressed you out", because the AI has no evidence for claims like those.

**What it does not do**:
- No step-by-step narration ("and then you went to the store, and then you came home")
- No invented sensory detail (no "the aisles smelled of cardamom") — the system has no data source for that
- No statements about what something meant ("the splurge was really about something else")
- No pretended neutrality (the narrator openly uses baseline data; choosing what to include is itself the interpretation, and the narrator does not hide that)

**Length**: 120-180 words. Long enough to synthesize a real day, short enough to read over morning coffee. Also the sweet spot for the local embedding model; longer dilutes the semantic signal. System prompt should target this explicitly: no preamble, no sign-off, just prose.

**Format**: Plain text. No markdown, no formatting. The autobiography is a paragraph, not a document.

**Entity linking (future)**: If entity chips are ever added to the autobiography, the LLM must return specific entity UUIDs/IDs for linked entities — no fuzzy matching at render time. Either the LLM knows the exact ID or it doesn't link. For V1, plain text only.

**Embedding note**: For Personal Dimensions axes (e.g., social/solitary, being/becoming), the autobiography is the better embedding surface because those axes are thematic, not factual. A raw event list won't project well onto "being/becoming." A synthesis that says "three domains are pulling at you" will.

**UI details**:
- Editable inline (current contenteditable approach for now)
- Subtle authorship signal: slightly muted when system-authored, full opacity when user-edited
- Pencil icon on hover to edit

### 6. Journal Section (user voice)

- **When journal exists**: First 3-5 lines, truncated with "Continue reading..." link to full journal page
- Styled differently from autobiography: thin left border (blockquote feel), slightly indented
- **When empty**: One muted line: *"Add your own account of the day →"* — links to journal page
- The day page is for reading; the journal page is for writing
- **Scope**: Outline/placeholder only for now. Full journal tab/feature is separate work.

### 7. Vertical Timeline (master-detail, the main content)

**Two-column layout within the timeline section:**

#### Left column (~35% width): Time-proportional vertical strip

- Thin colored bars, height proportional to event duration
- A 3-hour block is visually 3x the height of a 1-hour block, so the strip shows durations accurately
- Each rect shows: time range, one-word label, colored bar
- **Sleep events**: Small/compressed rects, muted styling
- **Transit events**: Small/compressed rects, muted styling
- **Unknown/insufficient data**: Dashed-outline rects at same proportional height, "+" button to add an event
- **"Now" marker**: Horizontal line at current time for today's page. Below it: empty dashed space (the future of today)
- Hover on a rect → detail panel updates on the right, map pans to that location

#### Right column (~65% width): Detail panel for selected/hovered event

- **Event label** (large, editable on click)
- **Location** (editable on click, entity-resolved: "Home" not "38.7223, -9.1393")
- **Duration** and **time range**
- **Event summary** (1-3 sentences from the AI, editable)
- **Novelty score** with context ("Rare for a Friday" or "Part of your routine")
- **Topics** as small chips
- **Source badges** (tiny icons: calendar, message, location, etc.)
- Default state (nothing hovered): shows the highest-novelty event or most recent event

#### Event Editing UX

- **Correcting**: Click any field (label, location, summary) → inline editable. No modal. Fix it in place.
- **User overrides** preserved on regeneration (existing `userLabel`, `userLocation`, `userNotes` model)
- **Subtle edit indicator**: Text changes from muted to full foreground color when user-edited. Tiny dot or different weight.
- **Adding events**: "+" button in unknown/gap segments. Minimal inline form: label, location, time range. Three fields. No modal.
- **Hiding events**: Soft delete via `userHidden`. Don't show by default, recoverable.

### 8. Movement Map

- Interactive map with route between stops (curved lines, not straight segments)
- **Entity-resolved place markers**: "Home (visited 312x)" not just a pin. Hover shows: name, first visit, total visits, typical time spent
- **Temporal scrubber**: Hover over timeline (left column) → dot on map shows where you were. GPS breadcrumbs make this smooth
- Map syncs with `hoveredEventId` — selecting an event pans/zooms to that location

### 9. Entities (inline, not a separate section)

- Flow inline after the map, not as a headed section
- Chips: "People: Maya Chen, Jess" — clickable to entity pages
- Places shown on map already; only people and organizations here

### 10. Data ontologies (built, in Data)

- The last section of Data: every record of the day in **one table**, all
  ontologies interleaved chronologically, with a chip per ontology (and its
  count) to filter it
- There is no Metadata section: no data-quality rating and no W6H on the page

---

## Today In-Progress State

When viewing today before the nightly summary has run:

- The day line marks now with a line and draws the events so far; nothing is projected past now
- In place of the autobiography: *"It's 2:47 PM. You've visited 3 places, exchanged 14 messages, and spent 45 minutes in motion. Your day is still being written."*
- Computed from available source data on page load (not live-updating)
- Timeline shows events so far with the "now" marker
- Map shows today's movement so far
- Full autobiography + complete dayline generate at end-of-day (or on-demand)

---

## Q1: Data Quality

### LLM-Assessed Quality (Hourly Event Cron)

The hourly event cron already processes new source data throughout the day. Add one field to its structured output:

```json
{
  "events": [...],
  "data_quality": {
    "rating": "good",
    "note": "Strong location and calendar coverage. No health data — Apple Watch wasn't worn?"
  }
}
```

- **`rating`**: One of `"rich"`, `"good"`, `"partial"`, `"sparse"` — grounded with examples in the system prompt so the LLM is calibrated
- **`note`**: One sentence, human-readable. What's strong, what's missing, optionally a gentle question.
- **LLM judges both quality AND quantity**: The LLM naturally weighs informational diversity — it knows 200 HR readings without location or calendar data is still a thin day, and that 3 breakup emails outweigh 16 joke emails. No need for a separate deterministic quantity metric; one LLM signal covers both.
- **Deterministic gate before LLM runs**: Simple SQL check — count distinct ontology tables with ≥1 row for this date. If < 2 distinct ontologies, skip the hourly cron entirely (too thin to process). This is a 3-line query, not a framework.
- **Cost**: Zero — one extra field in the JSON already being requested
- **Updates hourly**: The in-progress day page can show a live quality signal that improves through the day ("sparse" at 8 AM → "good" by 3 PM → "rich" by evening)
- **Gates nightly summary**: If still `sparse` at end of day, skip autobiography generation or show "Not enough data to write today's entry"
- **Display**: Subtle indicator in the metadata section at the bottom of the day page.

### Future: "Virtues Wrapped" (Monthly/Quarterly Reflection)

Aggregate data_quality ratings + ontology record counts across 30-90 days for a periodic reflection feature. "You had 58 rich days, 22 good days, and 10 sparse days this quarter. Your weekends are consistently sparse — consider wearing your watch on Saturdays." This is where structural coverage insights belong — not on the daily page.

---

## Q2: Entropy / Novelty

### Cross-Day (cut)

An earlier design embedded the whole day's text per-domain and stored
`chaos_score = 1 - cosine_sim(today_embedding, centroid_embedding)` on
`wiki_days`. It was deleted with the scoring rewrite — the column no longer
exists. Cross-day entropy is now answered two ways: per-event novelty
aggregated over the day (below). The home page's live rhythm strip, which
answered it a second way, was removed with the home page on 2026-10-06.

### Intra-Day Novelty (built)

Per-event novelty, embedded on-box via the local embedding sidecar
(`dayline/novelty.rs`). Two orthogonal z-scores against a recency- and
phase-weighted baseline of recent events, never blended: `novelty_z` (cosine
distance from a kernel-weighted centroid — exponential recency decay ×
von Mises kernels on hour-of-day and weekday) and `local_novelty_z` (a Local
Outlier Factor, "off-pattern for its kind"). NULL below 3 baseline days.

See the **Dayline Scoring** section below for the full novelty signal specification.

---

## Q3: Narrative Shape

**What it measures**: What happened throughout the day, when, and for how long.

**Why a timeline, not an arc**: There's no single "Y axis" for a day's narrative. Importance? Heart rate? Productivity? Energy? Moral weight? There are too many competing dimensions. A flat timeline with labeled events shows the day without inventing a single measure; the sequence along the X axis is the shape.

**Implementation**: LLM identifies 8-16 events during virtues-api summary generation. Stored in `wiki_events` with `auto_label`, `start_time`, `end_time`, `auto_location`. Displayed in the day page's Data view as:

- **The day line**: the day's clock with each event as a block, as tall as it was unlike the owner's usual (see §4)
- **The event timeline**: time, label, duration, the event's people and its place when the place has a real name; an unknown stretch reads "Not recorded"

---

## Q4: Alignment (Future)

**The question**: Is today's shape conducive to the person I want to become?

**Why it's hard**:

1. **Requires a structured narrative identity document that doesn't exist yet.** The telos doc needs to be anchored to a specific future date. Aspirations without a time horizon are unfalsifiable.

2. **Mixes structured and unstructured components.** Habits and todos are structured (did I meditate? did I exercise?). Values and motifs are unstructured (am I becoming more patient? more present?). The comparison mechanism needs to handle both, and the unstructured part requires semantic understanding.

3. **Comparison is non-trivial.** Even with a structured identity doc, computing "how aligned was today" requires either:
   - Cosine similarity between today's event profile and desired profile (crude but computable)
   - Semantic similarity between today's event embeddings and identity doc embeddings (richer)
   - LLM-based judgment (most nuanced but expensive and non-deterministic)

4. **The identity doc itself needs to evolve.** "Who I want to become" changes. The system needs to handle versioned identity documents and compute alignment against the version that was current at the time of each day.

**When to build**: After event embeddings and narrative identity doc schema are in place. The comparison layer is an overlay on top of the existing day data, not a replacement for anything.

---

## Dayline Scoring: Novelty + Autonomic

> Two per-event z-scored signals on the same +/-3 sigma scale:
> **Novelty** (Novel up / Routine down) and **Autonomic** (Stress up / Recovery down).
> Together they reveal when your body's response to a moment diverges from your baseline —
> something no wearable can do alone.
>
> The day line draws novelty only (`local_novelty_z`, else `novelty_z`; §4).
> `autonomic_z` is still computed (`dayline/autonomic_scoring.rs`), and no
> view draws it.

### What a wearable cannot compute

Wearables can tell you "your HR was 78." But they can't tell you:

> "Your HR is usually 70 during design standups. Today's 78 is +2.3 sigma above your baseline for this type of event."

That requires three things no wearable has:

1. **Event classification** — what were you doing? (calendar + location + app data + LLM summary)
2. **Personal history** — what does your body usually do during similar events? (12 weeks of embedded events paired with HR)
3. **Embedding similarity** — matching today's event to historically similar events (embedding infrastructure)

The dayline has all three. The embedding space acts as the activity classifier. "Design standup with Maya and David about onboarding drop-off" captures everything — meeting type, people, topic — in one 768-dimensional vector. No explicit categories needed.

### Three questions the scoring answers

The scoring was designed to answer three questions about each day. The day line draws Q1 only; Q0 and Q2 are not on the page.

**Q0: "How did I sleep, and how ready am I?"**
Sleep architecture (phases, depth, fragmentation) and morning readiness score. The foundation — where the day begins. This is DATA, not a scored signal.

**Q1: "What was different about today?"**
The **novelty** signal. Semantic/cognitive unusualness — new people, new places, unusual topics, novel combinations. Z-scored against 12 weeks of personal history using embedding similarity with day-of-week weighted average. This signal tracks how unusual each part of the day was.

**Q2: "How did my body handle it?"**
The **autonomic** signal. Physiological response compared to personal baseline for contextually similar events. Z-scored using embedding-weighted HR comparison. This signal measures how the body responded.

Q1 and Q2 are the two scored signals (+/-3 sigma). Q0 is contextual data (sleep phases + readiness score) that gives context to the scored signals. The most useful information comes from comparing Q1 and Q2: routine events that stressed you, and novel experiences you handled with ease.

### Novelty Signal — "How unusual is this event's content?"

| Property | Value |
|----------|-------|
| **Input** | Event summary embedding (local embedding sidecar) |
| **Method** | Cosine distance from 12-week centroid with DoW weighted average |
| **Comparison** | Global — all baseline events form one centroid |
| **Question** | "Is this event unusual IN MY LIFE?" |
| **Output** | `novelty_z` on `wiki_events` (+/-3 sigma clamped) |

The centroid approach works for novelty because you're asking a global question. Every past event contributes to what "normal" looks like. The centroid represents your normal, and distance from it measures unusualness.

**What's captured:** Semantic/cognitive novelty — new topics, new people, new places, unusual combinations.

**What's NOT captured (by design):**
- Time of day (waking at 8:30 vs 6:30 — same summary text, same embedding). The LLM hourly action would naturally mention unusual timing in the summary if notable.
- Circadian rhythm (irrelevant — semantic content doesn't vary with biology)

### Autonomic Signal — "Is your body responding the way it usually does?"

| Property | Value |
|----------|-------|
| **Input** | Avg HR during event window (primary), HRV/SDNN when available (supplementary) |
| **Method** | Similarity-weighted regression across all baseline events |
| **Comparison** | Local — weighted by embedding similarity + recency (spacetime) |
| **Question** | "Is my body responding unusually FOR THIS TYPE OF EVENT?" |
| **Output** | `autonomic_z` on `wiki_events` (+/-3 sigma clamped) |
| **+Y label** | Stress (tooltip: "Sympathetic activation") |
| **-Y label** | Recovery (tooltip: "Parasympathetic dominance") |

The similarity-weighted approach works because you're asking a local question. Comparing your standup HR to your running HR is meaningless. The embedding similarity ensures you're comparing standups to standups, runs to runs.

**Two weighting dimensions (spacetime):**

```
weight = embedding_similarity x recency
```

- **Embedding similarity (space):** `exp(-distance^2 / 2*sigma^2)` — standups weight heavily for other standups, near-zero for runs
- **Recency (time):** `exp(-days_ago / 21)` — 3-week half-life exponential decay, because your body changes over time

No circadian normalization. The raw circadian shape is part of the autonomic signal, because the afternoon dip and morning freshness are real physiological effects.

**Context-gated HR/HRV composite:**

```
Physical activity events (avg_hr > resting + 2 sigma):
    autonomic_z = hr_z
    # HRV compresses to near-zero above ~100bpm

Sedentary/cognitive events:
    autonomic_z = 0.3 x hr_z + 0.7 x (-hrv_z)
    # HRV dominates for mental stress (AUC 0.78 vs 0.65)

Sleep events:
    autonomic_z = -hrv_z
    # Resting HRV is the gold standard for recovery
```

**Scoring formula (per signal):**

```
expected_hr = sum(weight_i x hr_i) / sum(weight_i)
expected_std = sqrt(sum(weight_i x (hr_i - expected_hr)^2) / sum(weight_i))
hr_z = (today_event_hr - expected_hr) / expected_std
```

Same formula applies independently for HRV. Both hr_z and hrv_z are stored separately, then combined via context gating above.

**Discordance detection:** When hr_z and hrv_z disagree by >1.5 sigma, flag the event — these are often the most interesting (early illness, overtraining, hidden stress).

### Reading the two signals together: demand vs supply

Read together, the two signals tell a demand-vs-supply story. The day line does not draw the autonomic signal, so this reading is not on the page:

- **Novelty line** (dark) = how semantically unusual is this moment? (Novel up / Routine down)
- **Autonomic line** (blue) = how is your body responding compared to baseline for this type of moment? (Stress up / Recovery down)

| Novelty | Autonomic | Meaning |
|---------|--------|---------|
| High | High | Novel situation, body noticed. Excited, stressed, engaged. |
| High | Low | Novel situation, body was calm. Experienced, composed, flow state. |
| **Low** | **High** | **Routine situation, body activated. Hidden stress. Something's off.** |
| Low | Low | Normal day, normal body. Autopilot. |

The **low novelty + high autonomic stress** case is the one no wearable can detect on its own. "You were doing your usual commute but your HR was 2 sigma above your commute baseline. What was different?" Maybe anxiety, poor sleep, a stressful text. Nothing on the page flags it yet.

When the autonomic line crosses below the novelty line — **strain** (the moment demands more than your body is equipped for). When autonomic is below novelty — **surplus** (you have capacity for what's happening).

### Scoring exercise against similar exercise

The naive "battery drain" model breaks during exercise: running at HR 152 would show as maximally depleted, even though runners feel great.

The embedding-based approach avoids this problem:

- "45-minute run on the usual loop" — find similar past runs in embedding space
- Similar runs had avg HR: [148, 152, 145, 150, 147] — mean 148.4, std 2.6
- Today's run HR: 152
- Autonomic z-score: (152 - 148.4) / 2.6 = **+0.9 sigma** — slightly harder than usual, not an outlier

Compare to naive resting-HR approach: 152 vs resting 62 = +90 bpm = an implausible outlier.

The embedding normalizes exercise automatically because similar events (other runs) also had high HR. The comparison is always between similar activities.

### Why Different Methods for Different Signals

| | Novelty | Autonomic |
|---|---|---|
| **Comparison geometry** | Centroid (global) | Similarity-weighted (local) |
| **Why** | "Is this unusual in my life?" needs all events as baseline | "Is my body responding unusually for this type?" needs activity-specific comparison |
| **Temporal weighting** | DoW weighted average over 12 weeks | Recency decay (essential — body changes over weeks) |
| **Activity normalization** | Not needed (semantic content is activity-independent) | Essential (HR 78 means different things for standup vs run) |
| **Circadian** | N/A | Raw shape preserved — NO normalization |

---

## Physiological Signal: HR as Primary, HRV as Supplementary

### Why avg HR (not HRV) is the primary signal

1. **Dense data** — Apple Watch samples HR every 5-10 min. Every event will have multiple HR readings in its window.
2. **Activity-type normalization handled by embeddings** — the main confound of HR (physical activity raises it) is handled by comparing to similar events, not to resting.
3. **Simple, intuitive, explainable** — "your HR was higher than usual for this type of event."

### Why HRV adds value (V2 enhancement)

HRV (SDNN from Apple Watch) provides additional specificity:
- HR 78 + normal HRV = moderate activation, probably fine
- HR 78 + depressed HRV = real stress, sympathetic dominant
- HR 78 + elevated HRV = recovering, parasympathetic rebound

When available, combine using context-gated weighting (see above) — physical activity uses HR only, sedentary/cognitive events weight HRV at 0.7, and sleep uses HRV only.

But HRV is sparse (every 1-3 hours from Apple Watch, requires stillness). For V1, HR-only is correct.

### What about a "battery" model?

We explored and rejected the battery/drain model (Garmin Body Battery style). The problem: every day produces the same monotonic curve — charge overnight, drain all day. The day-to-day variation is small and tells the person little. Per-event physiological deviation carries much more information.

### Apple Watch Data Availability

| Signal | HealthKit Type | Frequency | Use in Autonomic Scoring |
|--------|---------------|-----------|----------------------|
| Raw HR | `heartRate` | Every 5-10 min | **Primary** — avg HR per event window |
| HRV (SDNN) | `heartRateVariabilitySDNN` | Every 1-3 hrs + sleep | Supplementary (V2) |
| Resting HR | `restingHeartRate` | 1x/day | Personal baseline calibration |
| Active calories | `activeEnergyBurned` | Continuous | Context (active vs sedentary) |
| Workouts | `HKWorkout` | Per session | Context for exercise events |
| Sleep stages | `sleepAnalysis` | During sleep | Excluded from autonomic scoring |

**Apple Watch HRV limitation:** SDNN spot-checks, not continuous beat-to-beat RMSSD. Measured every 1-3 hours during the day, every 15-30 min during sleep. Cannot be triggered programmatically from an iOS app — Apple controls sampling frequency.

---

## Cold Start

- **Days 1-3:** Insufficient baseline. No autonomic z-scores computed.
- **Days 4-14:** Building baseline. Autonomic scores computed but flagged as "calibrating" (low confidence due to small sample size).
- **Day 14+:** Fully personalized. Require N>=5 similar events (combined weight above threshold) to compute autonomic z-score for an event. Events with no similar history show no autonomic signal (showing nothing is better than showing an invented score).

---

## Event HR Annotation Pipeline

The steps run in sequence, so no step waits on a later one:

```
1. Apple Watch -> data_health_heart_rate (raw HR samples, every 5-10 min)
2. Hourly action -> wiki_events (event boundaries + summaries + embeddings)
3. Post-processing -> annotate wiki_events.avg_hr from HR data in event window:
   SELECT AVG(bpm) FROM data_health_heart_rate
   WHERE timestamp >= event.start_time AND timestamp < event.end_time
4. Autonomic scoring -> embedding similarity + HR comparison -> autonomic_z
```

---

## How each signal weights similarity and time

Both scoring methods use two dimensions, embedding similarity ("space") and time, but weight them differently:

**Novelty** emphasizes **space** (embedding distance from centroid). Time is implicit (the 12-week window defines the baseline, with DoW weighting). The question is spatial: "where does this event sit in the landscape of my life?"

**Autonomic** emphasizes **both space AND time** equally. The composite kernel weights by embedding similarity (space) AND recency (time). The question is spatiotemporal: "how does my body respond to events LIKE this, RECENTLY?"

Every signal in a person's life has both a "what" dimension (semantic content) and a "when" dimension (temporal context), and the two methods weight them differently. The day line draws the novelty side only.

---

## Comparison with existing products

No existing product combines:
1. LLM-generated event classification (semantic understanding of WHAT happened)
2. Embedding-space activity similarity (implicit activity clustering without labels)
3. Per-event physiological deviation scoring (how YOUR body responded vs YOUR baseline for THIS type of event)
4. The two-signal narrative (novelty as semantic unusualness, autonomic as physiological response deviation)

Wearables have #3 partially (HR data) but lack #1 and #2 (they don't know what you were doing). Life-logging apps have #1 but lack #3 (they don't have physiological data). The dayline integrates all four.

The "surprise divergence" — a routine event with an unusual physiological response — is the main result this combination makes possible. It shows something the user couldn't see from either data source alone, and it needs many kinds of data combined.

---

## Scientific Grounding

### What We're Measuring

The "autonomic" signal is technically **acute cardiac autonomic modulation** — the real-time flexibility of the autonomic nervous system, compared to personal baseline for contextually similar events. This is distinct from several related scientific constructs:

| Construct | What it is | Timescale | Measurable from wrist? |
|-----------|-----------|-----------|----------------------|
| **Arousal** | How activated is your nervous system now? A STATE, not a resource. | Seconds/minutes | Partially (HR) |
| **Vagal tone** | Baseline nervous system flexibility. A TRAIT. The "size of your battery." | Weeks/months | Yes (resting HRV) |
| **Allostatic load** | Cumulative stress wear and tear. | Months/years | No (needs bloodwork) |
| **Self-regulatory capacity** | Can you control attention and behavior? (Ego depletion — contested.) | Hours | No (needs cognitive tests) |
| **Cardiac autonomic modulation** (what we measure) | How is your ANS performing NOW vs how it USUALLY performs in THIS context? | Minutes/hours | **Yes (HR + HRV)** |

Our "autonomic" score is closest to cardiac autonomic modulation, z-scored against personal context. It's most like arousal (current state, changes moment-to-moment) but with the context-aware comparison that makes it resource-like — "do you have more or less capacity than usual for this type of moment?"

### Well-Established Science (strong foundation)

**HRV reflects autonomic flexibility and self-regulatory capacity.** The neurovisceral integration model (Thayer & Lane, 2000, 2009) directly connects cardiac vagal tone to prefrontal cortex function. Higher resting HRV predicts better executive function, emotional regulation, and cognitive flexibility. Replicated across hundreds of studies. Not controversial.

**Cognitive load reduces HRV.** Mental effort — sustained attention, working memory, decision-making — activates the sympathetic nervous system and measurably reduces HRV (Taelman et al., 2009; Backs & Seljos, 1994; Mukherjee et al., 2011). Typical reduction: 15-30% in RMSSD during demanding cognitive tasks vs rest.

**Post-exercise parasympathetic rebound.** After exercise, HRV spikes above baseline and HR drops below resting for 30-90 minutes (Stanley, Peake, & Buchheit, 2013). The "feeling great after a run" has a measurable physiological signature.

**Caffeine effects on autonomic state.** Caffeine is sympathomimetic: increases HR ~5-10 bpm, decreases HRV ~10-20% acutely. However, habitual drinkers develop tolerance — chronic daily coffee produces minimal acute HRV change after 2-3 weeks of adaptation. The embedding-weighted comparison handles this: your baseline INCLUDES your habitual caffeine response.

**Individual response stereotypy.** People have characteristic, repeatable physiological responses to specific types of situations (Engel, 1960; Lacey & Lacey, 1958). Your body DOES respond consistently to "standups" differently than "deep focus." This validates the embedding-weighted comparison approach.

**HR + HRV carry independent information.** Same HR can indicate different states depending on HRV:
- HR 75 + HRV high — mildly activated, plenty of reserve (handling it easily)
- HR 75 + HRV low — mildly activated, no reserve (struggling at this level)
Both HR and HRV are needed for the full picture.

### Established but Nuanced

**SDNN from Apple Watch (60-second windows).** Research standard is 5-minute recordings in controlled position. Apple Watch uses ~60s during uncontrolled daily activity. Correlation with clinical devices: r ~ 0.8-0.9 in controlled conditions, lower during movement. Directionally reliable but noisier than research-grade.

**Context-gated HR/HRV weighting.** Our composite uses context-dependent gating: physical activity = HR only (HRV compresses above ~100bpm), sedentary/cognitive = 0.3xHR + 0.7xHRV (HRV dominates for mental stress, AUC 0.78 vs 0.65), sleep = HRV only (resting HRV is the gold standard for recovery). This is a reasonable heuristic informed by physiological constraints but NOT from a single published formula. No validated universal formula exists for combining HR and HRV into a single "capacity" score. Garmin's Body Battery is the closest analogue and its formula is proprietary.

**Polyvagal theory (Porges).** Influential but contested on anatomical grounds. However, the basic claim that HRV reflects autonomic flexibility does NOT depend on polyvagal theory — it's independently supported by the neurovisceral integration model.

### Novel (no direct precedent)

**Embedding-weighted physiological comparison.** No published research combines NLP event embeddings with per-event HR/HRV deviation scoring. The principle is sound (compare physiology to contextually similar situations — supported by individual response stereotypy research) but the specific implementation is new. No published validation.

**The demand/supply narrative.** Framing semantic novelty as "demand" and autonomic reserve as "supply" is our framework. The closest scientific precedent is the **transactional model of stress** (Lazarus & Folkman, 1984) — stress occurs when perceived demands exceed perceived resources. We operationalize both sides with quantitative signals.

**The "surprise divergence" detection.** Low-novelty + high-autonomic-deviation as a hidden stress indicator is a novel hypothesis. Reasonable but untested.

### Known Limitations

**Cannot distinguish excitement from stress.** High sympathetic activation (elevated HR, low HRV) could mean stress (negative) OR excitement (positive). The embedding comparison normalizes for activity type but can't detect valence within the same activity type. "Stressed about standup" and "excited about standup" look the same physiologically.

**Stimulant effects can look counterintuitive.** Caffeine depletes autonomic reserve while boosting subjective alertness, and the model shows the physiological effect. For habitual users, this corrects itself: the baseline includes their usual caffeine response, so only DEVIATIONS from their caffeine pattern show up. For non-habitual users, the model correctly shows "your body is stressed by this unfamiliar substance."

**Cannot measure cognitive performance or emotional state.** No wrist sensor captures working memory, reaction time, creativity, mood, or motivation. The autonomic score is a physiological proxy (~70-80% correlated with subjective energy reports per Firstbeat validation data). The 20-30% gap is mostly stimulant effects and post-exercise endorphins.

**Short-term HRV measurements are noisy.** 60-second SDNN from Apple Watch has higher variance than 5-minute research recordings. Individual readings should be treated as directional estimates, not precise measurements. The z-scoring against many similar events averages out this noise.

### Naming Decision

No view draws the autonomic signal at present. If one does, it uses **"Autonomic"** as the line name with **"Stress / Recovery"** as the axis labels.

- **"Autonomic"** — scientifically precise (we ARE measuring the autonomic nervous system), entering common vocabulary alongside terms like serotonin and cortisol
- **"Stress" (+Y)** — physiologically accurate. Exercise, caffeine, mental load are all stressors. One syllable, instant recognition. Tooltip: "Sympathetic activation"
- **"Recovery" (-Y)** — warm, intuitive. Tooltip: "Parasympathetic dominance"
- Column name: `autonomic_z`
- Morning score: **"Readiness"** (0-100 on wiki_days) — industry standard, measures starting autonomic state

UI hover explanation: *"The autonomic line shows how your body is responding compared to how it usually responds during similar moments. Stress means your body is mobilizing more than usual. Recovery means it's restoring."*

### Key References

- Thayer & Lane (2000, 2009): Neurovisceral integration model — HRV as index of prefrontal-subcortical circuit function
- Lazarus & Folkman (1984): Transactional model of stress — demands vs resources
- Taelman et al. (2009): Influence of mental stress on heart rate variability
- Stanley, Peake, & Buchheit (2013): Cardiac parasympathetic reactivation following exercise
- Backs & Seljos (1994): Metabolic and cardiorespiratory measures of mental effort
- Engel (1960): Individual response stereotypy in autonomic responses
- McEwen (1998): Allostatic load — the cost of chronic stress adaptation

---

## Implementation

### Schema Changes
- Add `avg_hr REAL` to `wiki_events` (nullable, populated when HR data exists)
- Add `autonomic_z REAL` to `wiki_events` (nullable, computed by autonomic scoring)

### Scoring Engine
- `virtues-core/src/dayline/autonomic_scoring.rs` — context-gated HR/HRV comparison
- Uses existing `vec_search` for embedding similarity lookup
- Composite kernel: embedding_similarity x recency (spacetime, 2 factors)
- Produces `autonomic_z` per event, clamped to +/-3 sigma

### Chart (DaylineChart.svelte)
- One signal: each event a block by `local_novelty_z`, else `novelty_z`, clamped to +/-3 sigma, with coverage lanes under it (§4)
- The autonomic line is not drawn, and neither is any treatment for "surprise divergence" moments

### Seed Data (for demo/testing)
- Add realistic avg_hr values to demo day events + baseline events
- Compute autonomic_z from seeded data to test the chart

---

## Data Flow

```
virtues-api LLM call (generate_day_summary)
    |-- Autobiography text (Layer 3)
    |-- Structured events JSON (Layer 2)
    |     \-- For each event:
    |           |-- store in wiki_events (auto_label, start/end time)
    |           |-- embed event summary (local embedding sidecar)
    |           \-- annotate avg_hr from HR data in event window
    \-- Data quality rating (LLM-assessed)
```

---

## Section Headers

- Reduce from current large serif h2 to smaller, lighter text
- Most sections don't need explicit headers — the content is self-evident
- The day line: headed "The day line" in Data, in sentence case like Data's other sections (Places, Event timeline, Data ontologies)
- Autobiography: no header (it's the opening paragraph)
- Journal: no header (distinct left-border styling identifies it)
- Timeline: "Event timeline", in Data
- Map: "Places", in Data, only when the day has location
- Entities: inline, no header
- Records: "Data ontologies", the last section of Data. There is no Metadata section

---

## W6H: The Seven Experiential Dimensions (Reference)

The W6H dimensions are the basic questions you'd ask to fully reconstruct any moment of lived experience. Every moment has these features, whether or not anything measures them. Sleep has a `who` whether you measure it or not. A meeting has a `whom` and `where` whether sensors are present or not.

The day summary LLM uses W6H only for a light secondary comment on how complete the day feels across these dimensions. It is not a core scoring mechanism.

| Dim | Question | What it captures | Example ontologies |
|-----|----------|-----------------|-------------------|
| who | Who am I right now? | Body, health, mental state, identity — the subject | heart_rate, sleep, workout |
| whom | Who else is here? | Other people, relationships — the social field | message, email, calendar |
| what | What is happening? | Activity, content, events — the substance | transcription, calendar, document |
| when | When does this matter? | Temporal significance — the scheduling | calendar, sleep, location_visit |
| where | Where am I? | Place, space, context — the setting | location_point, location_visit |
| why | Why am I doing this? | Purpose, motivation — the intent | transcription, conversation, document |
| how | By what means? | Method, process, tools — the mechanism | app_usage, workout, steps |

### Classical Heritage

The impulse to decompose human acts into these questions is ancient. The *septem circumstantiae* (seven circumstances) of classical rhetoric — attributed to Hermagoras of Temnos (1st century BC), refined by Cicero and Thomas Aquinas — asked *Quis, quid, ubi, quibus auxiliis, cur, quomodo, quando*. The original Latin categories overlap considerably (how / by what means / why blur together), and the mapping to our W6H is loose, not literal. Both rest on the same idea: a small fixed set of independent questions can describe any situated human experience.

---

## Key Files

| File | Role |
|------|------|
| `crates/virtues-registry/src/ontologies.rs` | Ontology registry: source configs |
| `virtues-core/src/api/wiki.rs` | Day data API: sources, events, day CRUD |
| `virtues-core/src/api/day_summary.rs` | virtues-api LLM call, event parsing |
| `virtues-core/src/dayline/autonomic_scoring.rs` | Context-gated HR/HRV autonomic scoring |
| `apps/web/src/lib/components/wiki/DayPage.svelte` | Main day page component: Article and Data |
| `apps/web/src/lib/components/wiki/DaylineChart.svelte` | The day line (§4); its geometry is `apps/web/src/lib/wiki/dayLine.ts` |
| `apps/web/src/lib/components/wiki/EventTimeline.svelte` | The event timeline |
| `apps/web/src/lib/components/wiki/DayPlaces.svelte` | Places: the stops strip and the map |
| `apps/web/src/lib/wiki/types/day.ts` | Frontend types: DayPage, DayEvent |
| `apps/web/src/lib/wiki/api.ts` | API client: getDaySources, getDayEvents |

---

## Open Questions / Future Items

- [ ] Journal feature: Full journal tab/page is separate work. Day page just shows a window into it.
- [ ] Sketch generation model: DALL-E 3 vs Midjourney API vs self-hosted. Cost is low either way (~$1.20/mo/user). Route through Bridge.
- [ ] How does the in-progress summary compute? Lightweight SQL aggregation on page load vs. scheduled partial summary.
- [ ] GPS breadcrumb smoothing for the temporal map scrubber
- [ ] Dimensions radar chart: how to measure orthogonality/polarity of user-defined axes in embedding space
