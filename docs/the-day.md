# The Day Page

> "The page should feel like opening a leather journal that somehow already has today's entry written in it — but you get to correct it, annotate it, beautify it, and eventually, over months, it becomes the most honest record of your life that exists."

One sentence: **The day page is a mirror you read at night.**

---

## The Four Questions

The day page answers four questions about a single day. Each question has a different temporal scope, different data requirements, and different implementation maturity.

| # | Question | Scope | Data Needed | Status |
|---|----------|-------|-------------|--------|
| Q1 | **Coverage** — How complete is today's data? | Today, static | LLM-assessed data quality rating | Planned (replaces W6H weights) |
| Q2 | **Entropy** — How ordered or chaotic was this day? | Today vs 12-week history | Cross-day: `chaos_score` on `wiki_days`. Intra-day: per-event novelty (not yet built). | Cross-day: built. Intra-day: designed below. |
| Q3 | **Narrative Shape** — What happened throughout the day? | Today, temporal (event-time) | LLM-identified events with labels, times, locations | Built. Timeline bar + table. |
| Q4 | **Alignment** — Is this day's shape conducive to who I want to become? | Today vs aspiration | Narrative identity document + comparison mechanism | Not built. See "Why Alignment Is Hard" below. |

---

## Page Layout (top to bottom)

### 1. Navigation Bar (sticky)

- **Yesterday / Tomorrow** text links at edges (e.g., `← Thu, Feb 12 ... Sat, Feb 14 →`)
- **Calendar icon** in center → opens month-grid date picker
- Kill the current week picker

### 2. Pencil Sketch (chapter header illustration)

- Full-width, ~120-160px tall, above the title
- **Topical, not narrative**: highest-novelty concrete noun from the day (a bungalow, a kayak, a coffee cup on a patio)
- Not generated on days with no salient novelty — absence is fine
- User picks global style in settings: pen & ink, watercolor, architectural marker, oil painting, pencil
- Uses space `accent_color` as single color accent against monochrome linework
- **Cost**: ~$0.04/image via DALL-E 3 or Midjourney API through Virtues Bridge. One per user per night = ~$1.20/month/user
- Nightly queue: scheduler fires after day summary, picks highest-novelty entity/noun, generates image, stores blob
- Can be turned off in settings. User can also customize the generation prompt

### 3. Day Header

- Serif title: "Friday, February 13, 2026"
- Subtitle: relative badge ("Today", "Yesterday", "3 days ago") + timezone

### 4. Dayline Chart Container (one instrument, multiple lenses)

- **Pill selector tabs** above a single chart area:
  - **Dayline** (default): 3D ribbon chart — "What was the shape of my day?" The signature view.
  - **Energy**: Body battery hourly curve — simple area chart, no 3D. "When did I have gas in the tank?"
  - **Entropy**: Routine vs novelty — "How chaotic vs. ordered was today?"
  - **Topology**: Fragmentation, context-switching, topic density — "Where was my focus?"
  - **Dimensions**: Radar chart of user-defined axes — "Who was I today?" User enters 3-5 word descriptions per axis (3-8 axes). System measures orthogonality/polarity in embedding space and gives feedback.
- **Thin horizontal minibar** at the base of the chart: the compressed timeline bar (current DayTimeline bar chart). Hover syncs with the chart above and the vertical timeline below via `hoveredEventId`.

### 5. Autobiography (system voice)

The autobiography is the **meaning layer** — synthesis, not chronology. Event summaries already capture what happened and when (the fact layer). The autobiography's job is to surface what connected, what was unusual, and what the day was *about*. These serve different embedding targets: event concat answers "find days where I went to Tatsu-ya," the autobiography answers "find days that felt like this one" or "days where work and personal life collided."

**Tone**: Warm but precise — closer to a perceptive friend reflecting your day back to you than either a surveillance report or a therapist's notes. It should earn its length by saying something the user couldn't have written themselves: cross-domain connections, baseline comparisons, behavioral patterns surfaced from data.

**What good looks like** (epigraph: *The Trader Joe's detour*):

> The routine held until 4:12 PM, when you broke your usual commute to visit the Seaholm Trader Joe's for the first time in your logged history. Your baseline Friday grocery run takes 18 minutes and averages $45. Today you lingered in the aisles for 52 minutes and swiped for $328.50. On the drive home, you sent a voice memo to Maya: *"I just bought enough snacks to survive a winter."* Despite the heavy spend, your HRV that evening was 12% above your Friday baseline.

**Why this works** (the bar the system should clear):

1. **Universally recognizable.** "Went in for milk, left with $300 of stuff" is a near-universal moment. The autobiography honors the small anomaly rather than dramatizing a constructed life-pivot. If the format works on the Trader Joe's run, it works on every day.
2. **Cross-ontology causal beat with timestamps and numbers.** Location (first visit), financial (transaction amount + duration), audio (voice memo with specific quote), biometric (HRV deviation) — four ontologies, one paragraph, each anchored to a number that could be verified.
3. **A baseline comparison the system actually computes.** "Your baseline Friday grocery run takes 18 minutes and averages $45" is the system openly drawing on history — its legitimate vantage point. Stated plainly. No fake neutrality.
4. **The user's own voice is the totem.** *"I just bought enough snacks to survive a winter"* is the photo-album trigger — the line that, surfaced in a year as an "on this day" memory, brings the whole afternoon back. The system's only editorial act is *picking the right line*; it does not write it.
5. **The closer is data, not interpretation.** "HRV that evening was 12% above your Friday baseline" lands as a fact, not a verdict. The reader infers what the splurge did to them. The narrator never says "it settled you" or "it stressed you out" — those are pronouncements the AI cannot honestly make.

**What it deliberately doesn't do**:
- No mom-narration ("and then you went to the store, and then you came home")
- No fabricated sensory ambiance (no "the aisles smelled of cardamom") — the system has no ontology for that
- No pronouncement of meaning ("the splurge was really about something else")
- No fake neutrality (the narrator openly draws on baseline data; selection is the interpretation, owned)

**Length**: 120-180 words. Long enough to synthesize a real day, short enough to read over morning coffee. Also the sweet spot for nomic-embed-text; longer dilutes the semantic signal. System prompt should target this explicitly: no preamble, no sign-off, just prose.

**Format**: Plain text. No markdown, no formatting. The autobiography is a paragraph, not a document.

**Entity linking (future)**: If entity chips are ever added to the autobiography, the LLM must return specific entity UUIDs/IDs for linked entities — no fuzzy matching at render time. Either the LLM knows the exact ID or it doesn't link. For V1, plain text only.

**Embedding note**: For Personal Dimensions axes (e.g., social/solitary, being/becoming), the autobiography is the better embedding surface because those axes are thematic, not factual. A raw event list won't project well onto "being/becoming." A synthesis that says "three domains are pulling at you" will.

**UI details**:
- Editable inline (current contenteditable approach for now; CodeMirror later)
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
- A 3-hour block is visually 3x the height of a 1-hour block — proportionality is honesty
- Each rect shows: time range, one-word label, colored bar
- **Sleep events**: Small/compressed rects, muted styling
- **Transit events**: Small/compressed rects, muted styling
- **Unknown/insufficient data**: Dashed-outline rects at same proportional height, "+" button to add an event
- **"Now" marker**: Horizontal line at current time for today's page. Below it: empty dashed space (the future of today)
- Hover on a rect → detail panel updates on the right, map pans to that location, dayline chart highlights that point

#### Right column (~65% width): Detail panel for selected/hovered event

- **Event label** (large, editable on click)
- **Location** (editable on click, entity-resolved: "Home" not "41.8781, -87.6298")
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

### 10. Metadata (collapsed reference section)

- **Data quality**: LLM-assessed rating (`rich`, `good`, `partial`, `sparse`) with one-sentence note
- **W6H completeness**: Fun afterthought — the day summary LLM decides how complete the day feels across experiential dimensions. Not a core metric, just color commentary.
- **Ontologies**: collapsed accordion — "Sources (47)"
- **One unified table** when expanded: all ontology records interleaved chronologically
- Each row: timestamp, source-type icon as the row marker (tiny calendar, message, pin, etc.), label, preview text

---

## Today In-Progress State

When viewing today before the nightly summary has run:

- Dayline chart shows **partial curve** — data points up to now, dotted continuation to right edge
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

### Cross-Day (built)

**What it measures**: How semantically different today was from your 12-week baseline.

**How it works** (`day_scoring.rs`):

1. Collect text from all ontologies for the day
2. Embed via nomic-embed (768-dim)
3. Compare to 12-week centroid with day-of-week weighted average
4. `chaos_score = 1 - cosine_sim(today_embedding, centroid_embedding)`

Implementation: `chaos_score` on `wiki_days`. Needs ~3+ summarized days for meaningful calibration.

### Intra-Day Novelty (designed, not built)

Per-event novelty scored as cosine distance from a 12-week centroid with day-of-week weighting. This is the same paradigm as cross-day, scoped to events: "Is this event unusual IN MY LIFE?"

See the **Dayline Scoring** section below for the full novelty signal specification.

---

## Q3: Narrative Shape

**What it measures**: What happened throughout the day, when, and for how long.

**Why a timeline, not an arc**: There's no single "Y axis" for a day's narrative. Salience? Heart rate? Productivity? Energy? Moral weight? Too many competing dimensions. The honest representation is a flat timeline with labeled events — the X axis IS the shape.

**Implementation**: LLM identifies 8-16 events during virtues-api summary generation. Stored in `wiki_events` with `auto_label`, `start_time`, `end_time`, `auto_location`. Displayed as:

- **Timeline bar**: Horizontal bar spanning 00:00-24:00 with colored segments per event
- **Timeline table**: Time, event label, location, duration

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

> The dayline chart shows two per-event z-scored signals on the same +/-3 sigma scale:
> **Novelty** (Novel up / Routine down) and **Autonomic** (Stress up / Recovery down).
> Together they reveal when your body's response to a moment diverges from your baseline —
> something no wearable can do alone.

### The Core Insight

Wearables can tell you "your HR was 78." But they can't tell you:

> "Your HR is usually 70 during design standups. Today's 78 is +2.3 sigma above your baseline for this type of event."

That requires three things no wearable has:

1. **Event classification** — what were you doing? (calendar + location + app data + LLM summary)
2. **Personal history** — what does your body usually do during similar events? (12 weeks of embedded events paired with HR)
3. **Embedding similarity** — matching today's event to historically similar events (embedding infrastructure)

The dayline has all three. The embedding space IS the activity classifier. "Design standup with Maya and David about onboarding drop-off" captures everything — meeting type, people, topic — in one 768-dimensional vector. No explicit categories needed.

### The Three Questions (Dayline)

The dayline chart answers three questions about each day, read left to right:

**Q0: "How did I sleep, and how ready am I?"**
Sleep architecture (phases, depth, fragmentation) and morning readiness score. The foundation — where the day begins. This is DATA, not a scored signal.

**Q1: "What was different about today?"**
The **novelty** signal. Semantic/cognitive unusualness — new people, new places, unusual topics, novel combinations. Z-scored against 12 weeks of personal history using embedding similarity with day-of-week weighted average. This is the narrative arc of the day.

**Q2: "How did my body handle it?"**
The **autonomic** signal. Physiological response compared to personal baseline for contextually similar events. Z-scored using embedding-weighted HR comparison. This is the physical reality beneath the narrative.

Q1 and Q2 are the two scored signals (+/-3 sigma). Q0 is contextual data (sleep phases + readiness score) that frames the scored signals. The relationship between Q1 and Q2 is where the unique insight lives — routine events that stressed you, novel experiences you handled with ease.

### Novelty Signal — "How unusual is this event's content?"

| Property | Value |
|----------|-------|
| **Input** | Event summary embedding (768-dim, nomic-embed) |
| **Method** | Cosine distance from 12-week centroid with DoW weighted average |
| **Comparison** | Global — all baseline events form one centroid |
| **Question** | "Is this event unusual IN MY LIFE?" |
| **Output** | `novelty_z` on `wiki_events` (+/-3 sigma clamped) |

The centroid approach works for novelty because you're asking a global question. Every past event defines what "normal" looks like. The centroid IS your normal. Distance from it IS unusualness.

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

No circadian normalization — the raw circadian shape IS part of the autonomic story. The afternoon dip is real. Morning freshness is real.

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

### The Demand vs Supply Narrative

The two lines on the chart tell a demand-vs-supply story:

- **Novelty line** (dark) = how semantically unusual is this moment? (Novel up / Routine down)
- **Autonomic line** (blue) = how is your body responding compared to baseline for this type of moment? (Stress up / Recovery down)

| Novelty | Autonomic | Meaning |
|---------|--------|---------|
| High | High | Novel situation, body noticed. Excited, stressed, engaged. |
| High | Low | Novel situation, body was calm. Experienced, composed, flow state. |
| **Low** | **High** | **Routine situation, body activated. Hidden stress. Something's off.** |
| Low | Low | Normal day, normal body. Autopilot. |

The **low novelty + high autonomic stress** case is the killer insight no wearable can surface. "You were doing your usual commute but your HR was 2 sigma above your commute baseline. What was different?" Maybe anxiety, poor sleep, a stressful text. The chart flags it. The user reflects.

When the autonomic line crosses below the novelty line — **strain** (the moment demands more than your body is equipped for). When autonomic is below novelty — **surplus** (you have capacity for what's happening).

### The Exercise Paradox — Solved

The naive "battery drain" model breaks during exercise: running at HR 152 would show as maximally depleted, even though runners feel great.

The embedding-based approach dissolves this entirely:

- "45-minute run on Mueller trails" — find similar past runs in embedding space
- Similar runs had avg HR: [148, 152, 145, 150, 147] — mean 148.4, std 2.6
- Today's run HR: 152
- Autonomic z-score: (152 - 148.4) / 2.6 = **+0.9 sigma** — slightly harder than usual, not an outlier

Compare to naive resting-HR approach: 152 vs resting 62 = +90 bpm = absurd outlier.

The embedding normalizes exercise automatically because similar events (other runs) ALSO had high HR. The comparison is always apples-to-apples.

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

We explored and rejected the battery/drain model (Garmin Body Battery style). The problem: every day produces the same monotonic curve — charge overnight, drain all day. The variation is small and boring. Per-event physiological deviation is a fundamentally richer signal.

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

- **Days 1-3:** Insufficient baseline. No autonomic z-scores computed. Chart shows novelty only.
- **Days 4-14:** Building baseline. Autonomic scores computed but flagged as "calibrating" (low confidence due to small sample size).
- **Day 14+:** Fully personalized. Require N>=5 similar events (combined weight above threshold) to compute autonomic z-score for an event. Events with no similar history fall back to showing no autonomic signal (honest > fabricated).

---

## Event HR Annotation Pipeline

Not chicken-and-egg — sequential pipeline:

```
1. Apple Watch -> data_health_heart_rate (raw HR samples, every 5-10 min)
2. Hourly action -> wiki_events (event boundaries + summaries + embeddings)
3. Post-processing -> annotate wiki_events.avg_hr from HR data in event window:
   SELECT AVG(bpm) FROM data_health_heart_rate
   WHERE timestamp >= event.start_time AND timestamp < event.end_time
4. Autonomic scoring -> embedding similarity + HR comparison -> autonomic_z
```

---

## Spacetime: The Unifying Principle

Both scoring methods operate in spacetime — but weight the dimensions differently:

**Novelty** emphasizes **space** (embedding distance from centroid). Time is implicit (the 12-week window defines the baseline, with DoW weighting). The question is spatial: "where does this event sit in the landscape of my life?"

**Autonomic** emphasizes **both space AND time** equally. The composite kernel weights by embedding similarity (space) AND recency (time). The question is spatiotemporal: "how does my body respond to events LIKE this, RECENTLY?"

This duality — spatial novelty vs spatiotemporal autonomic response — is core to the personal OS. Every signal in a person's life has both a "what" dimension (semantic content) and a "when" dimension (temporal context). The dayline chart encodes both.

---

## What Makes This Unique

No existing product combines:
1. LLM-generated event classification (semantic understanding of WHAT happened)
2. Embedding-space activity similarity (implicit activity clustering without labels)
3. Per-event physiological deviation scoring (how YOUR body responded vs YOUR baseline for THIS type of event)
4. The two-signal narrative (novelty as semantic unusualness, autonomic as physiological response deviation)

Wearables have #3 partially (HR data) but lack #1 and #2 (they don't know what you were doing). Life-logging apps have #1 but lack #3 (they don't have physiological data). The dayline integrates all four.

The "surprise divergence" — routine event with unusual physiological response — is the signature insight. It surfaces something the user couldn't see from either data source alone. It requires the totality of a life's data.

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

**Stimulant effects are honest, not intuitive.** Caffeine depletes autonomic reserve while boosting subjective alertness. Our model shows the physiological truth. For habitual users, this self-corrects: the baseline includes their usual caffeine response, so only DEVIATIONS from their caffeine pattern register. For non-habitual users, the model correctly shows "your body is stressed by this unfamiliar substance."

**Cannot measure cognitive performance or emotional state.** No wrist sensor captures working memory, reaction time, creativity, mood, or motivation. The autonomic score is a physiological proxy (~70-80% correlated with subjective energy reports per Firstbeat validation data). The 20-30% gap is mostly stimulant effects and post-exercise endorphins.

**Short-term HRV measurements are noisy.** 60-second SDNN from Apple Watch has higher variance than 5-minute research recordings. Individual readings should be treated as directional estimates, not precise measurements. The z-scoring against many similar events averages out this noise.

### Naming Decision

We use **"Autonomic"** as the line name with **"Stress / Recovery"** as the axis labels.

- **"Autonomic"** — scientifically precise (we ARE measuring the autonomic nervous system), entering common vocabulary alongside terms like serotonin and cortisol
- **"Stress" (+Y)** — physiologically honest. Exercise, caffeine, mental load are all stressors. One syllable, instant recognition. Tooltip: "Sympathetic activation"
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
- Second line on same +/-3 sigma chart (blue/primary color, lower opacity)
- Novelty line = dark, event-driven, spiky
- Autonomic line = blue, event-driven, spiky (NOT monotonic — per-event, not a drain curve)
- Visual treatment for "surprise divergence" moments (low novelty + high autonomic stress)

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
    |           |-- embed event summary -> 768-dim vector
    |           \-- annotate avg_hr from HR data in event window
    |-- Data quality rating (LLM-assessed)
    \-- Chaos score (cross-day novelty vs 12-week DoW-weighted centroid)
```

---

## Section Headers

- Reduce from current large serif h2 to smaller, lighter text
- Most sections don't need explicit headers — the content is self-evident
- Dayline chart: no header (visually obvious)
- Autobiography: no header (it's the opening paragraph)
- Journal: no header (distinct left-border styling identifies it)
- Timeline: no header or very small "Timeline" label
- Map: no header or very small "Movement" label
- Entities: inline, no header
- Metadata: **keep header** — "Sources (47)" as collapsed accordion label

---

## W6H: The Seven Experiential Dimensions (Reference)

The W6H dimensions are the irreducible questions you'd ask to fully reconstruct any moment of lived experience. They aren't lenses you choose to apply — they're structural features of any situated moment. Sleep has a `who` whether you measure it or not. A meeting has a `whom` and `where` whether sensors are present or not.

W6H is used as color commentary by the day summary LLM — a fun afterthought on how complete the day feels across experiential dimensions. Not a core scoring mechanism.

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

The impulse to decompose human acts into these questions is ancient. The *septem circumstantiae* (seven circumstances) of classical rhetoric — attributed to Hermagoras of Temnos (1st century BC), refined by Cicero and Thomas Aquinas — asked *Quis, quid, ubi, quibus auxiliis, cur, quomodo, quando*. The original Latin categories overlap considerably (how / by what means / why blur together), and the mapping to our W6H is loose, not literal. But the core insight is the same: a finite set of orthogonal questions can fully characterize any situated human experience.

---

## Key Files

| File | Role |
|------|------|
| `crates/virtues-registry/src/ontologies.rs` | Ontology registry: source configs |
| `virtues-core/src/api/wiki.rs` | Day data API: sources, events, day CRUD |
| `virtues-core/src/api/day_summary.rs` | virtues-api LLM call, event parsing |
| `virtues-core/src/dayline/autonomic_scoring.rs` | Context-gated HR/HRV autonomic scoring |
| `apps/web/src/lib/components/wiki/DayPage.svelte` | Main day page component |
| `apps/web/src/lib/components/wiki/DayTimeline.svelte` | Timeline bar + table |
| `apps/web/src/lib/components/wiki/DayToolbar.svelte` | Toolbar with metrics, generate button |
| `apps/web/src/lib/components/wiki/ContextVector.svelte` | Coverage accordion |
| `apps/web/src/lib/wiki/types/day.ts` | Frontend types: DayPage, DayEvent |
| `apps/web/src/lib/wiki/api.ts` | API client: getDaySources, getDayEvents |

---

## Open Questions / Future Items

- [ ] Journal feature: Full journal tab/page is separate work. Day page just shows a window into it.
- [ ] Sketch generation model: DALL-E 3 vs Midjourney API vs self-hosted. Cost is low either way (~$1.20/mo/user). Route through Bridge.
- [ ] How does the in-progress summary compute? Lightweight SQL aggregation on page load vs. scheduled partial summary.
- [ ] GPS breadcrumb smoothing for the temporal map scrubber
- [ ] Dimensions radar chart: how to measure orthogonality/polarity of user-defined axes in embedding space
