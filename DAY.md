# Day Page Architecture

The day page answers four questions about a single day. Each question has a different temporal scope, different data requirements, and different implementation maturity.

## The Four Questions

| # | Question | Scope | Data Needed | Status |
|---|----------|-------|-------------|--------|
| Q1 | **Coverage** — How complete is today's data across the 7 W6H dimensions? | Today, static | W6H context vector (binary ontology presence × weights) | Built. Accordion dropdown with per-dimension bars. |
| Q2 | **Entropy** — How ordered or chaotic was this day? | Today vs history (cross-day); today over time (intra-day) | Cross-day: `chaos_score` on `wiki_days`. Intra-day: per-event entropy (not yet built). | Cross-day: built (toolbar metric). Intra-day: designed below, not yet implemented. |
| Q3 | **Narrative Shape** — What happened throughout the day? | Today, temporal (event-time) | LLM-identified events with labels, times, locations | Built. Timeline bar + table. |
| Q4 | **Alignment** — Is this day's shape conducive to who I want to become? | Today vs aspiration | Narrative identity document + comparison mechanism | Not built. See "Why Alignment Is Hard" below. |

## Q1: Coverage

**What it measures**: Data presence across 7 W6H dimensions (who, whom, what, when, where, why, how). Binary: does ANY ontology with non-zero weight in that dimension have data today?

**Why binary works**: We care that body data EXISTS (heart rate, steps, sleep), not that there are 3,000 readings. Quality differences are already encoded in per-ontology weights — location visits > raw GPS points, calendar events > app usage. Volume doesn't matter; category coverage does.

**Formula**: For each dimension d, sum the max weight from present ontologies, normalize by the max possible weight. The context vector is a 7-element array of 0–1 scores. Overall completeness = mean of all 7.

**Implementation**: `compute_context_vector()` in `day_summary.rs`. Displayed via `ContextVector.svelte` accordion (per-dimension bars) and `DayToolbar.svelte` (single % number).

**Key insight**: Coverage is a single snapshot of the entire day. It has no time axis. Don't conflate it with Q3 (narrative shape over time).

## Q2: Entropy

**Definition**: Entropy here means **novelty, unexpectedness, cognitive load** — measured through information theory (predictability via embedding similarity). Not data volume. Not ontology count. A monotonous day with 3,000 heart rate readings and 500 location points is LOW entropy. A single unexpected phone call that changes everything is HIGH entropy.

### Cross-Day (built)

**What it measures**: How semantically different today was from your 30-day baseline, per W6H dimension.

**How it works** (`day_scoring.rs`):

1. Collect text from all ontologies for the day, grouped by W6H dimension weight
2. Embed each dimension's text blob via nomic-embed (768-dim)
3. Compare to exponentially-decayed centroid of prior 30 days
4. `dim_chaos = 1 - cosine_sim(today_dim_embedding, centroid_dim_embedding)`
5. `chaos_score = Σ(dim_chaos × coverage[dim]) / Σ(coverage[dim])`

This is already embedding-space math. The cross-day score IS semantic novelty — "how different is what happened today from what usually happens, in each dimension of life."

**Implementation**: `chaos_score` on `wiki_days`. Single metric in toolbar. Needs ~3+ summarized days for meaningful calibration.

### Intra-Day Entropy (designed, not built)

**The idea**: A narrative arc visualization sitting directly above the event timeline. Same X axis (00:00–24:00), Y axis is entropy (0 = order, 1 = chaos). Sleep and quiet moments hug the bottom; midday work with many competing signals pushes toward the top.

```
1.0 ─                    ╱╲        ╱──╲
     │                  ╱  ╲      ╱    ╲
     │        ╱╲      ╱    ╲    ╱      ╲
     │       ╱  ╲    ╱      ╲──╱        ╲
     │      ╱    ╲──╱                    ╲
0.0 ─ ────╱                              ╲────
     00:00  06:00  09:00  12:00  15:00  18:00  24:00
     sleep   wake   work    lunch  work   evening  sleep
```

### Candidate Approaches (evaluated)

#### 1. W6H Shannon Entropy (structural complexity)

Measures how spread the W6H activation is across dimensions at a single moment.

```
H = -Σ (p_i * log2(p_i))  where p_i = w6h[i] / sum(w6h)
```

Sleep `[0.9, 0, 0, 0, 0, 0, 0]` → low (concentrated in one dimension). Work meeting `[0, 0.8, 0.8, 0.7, 0.3, 0.3, 0]` → high (spread across many).

**What it captures**: "How many kinds of experience are happening simultaneously." Internal complexity of a moment.

**What it misses**: Two meetings with identical W6H profiles but radically different content (architecture review vs layoffs) score the same. No sequential awareness — doesn't know if this moment is surprising relative to the previous one.

**Verdict**: Cheap, directionally correct, useful as a fallback for events with continuous-stream data but no embeddable text. Not the primary signal.

#### 2. Ontology Count / Data Density (rejected)

Count of distinct ontologies with data in the event's time window.

**Fatal flaw**: You can have 15 ontologies firing during a monotonous day at your desk (heart rate + steps + GPS + emails + calendar + app usage...). Count measures instrumentation density, not experiential chaos. A routine day with good sensors scores HIGH. This is wrong by every definition of entropy — physics (microstates), information theory (surprise), psychology (novelty), narrative (tension).

**Verdict**: Rejected as primary signal. The rate of change matters; raw count does not.

#### 3. Transition Rate (W6H cosine distance between consecutive events)

`transition_entropy = 1 - cosine_sim(w6h_event_n, w6h_event_n-1)`

**What it captures**: Context switching — "did the type of life happening change?" Work → call → email → meeting = high. Sustained deep work = low.

**What it misses**: Operates in 7-dim W6H space — too coarse. Can't distinguish two meetings with identical W6H profiles but different topics. Subsumed by #4.

**Verdict**: Correct direction, insufficient resolution. #4 does everything #3 does and more.

#### 4. Per-Event Embedding Novelty (the correct paradigm)

For each event, collect source text from ontologies in its time range, embed via nomic-embed (768-dim), measure transition:

```
event_entropy = 1 - cosine_sim(event_n_embedding, event_n-1_embedding)
```

**Why this is the most correct approach**:

- **Naturally includes W6H context switching**: Different W6H profiles → different source text → different embeddings → large cosine distance. If you go from sleep (text: "Sleep: 7h30m") to a work meeting (text: "Calendar: Architecture review, Email from John: ..."), the embeddings are maximally distant.
- **Goes beyond W6H**: Two meetings with identical W6H profiles `[0, 0.8, 0.8, 0.7, 0.3, 0.3, 0]` but different topics (architecture vs layoffs) produce semantically different text → different embeddings → detected as a transition. W6H-only approaches are blind to this.
- **Consistent with cross-day**: The cross-day chaos score already uses `1 - cosine_sim(today, centroid)` in embedding space. Intra-day uses the same math, just scoped to consecutive events instead of day-vs-30-day-average. Same paradigm, different scale.
- **Correct by every definition**: Information theory (measures surprise/predictability), psychology (measures novelty/cognitive load), narrative (measures scene changes), physics (measures disorder/distance from equilibrium).

**How it works in practice**:

1. For each event with time range `[start, end]`, collect text from ontologies with data in that range (reuse `collect_ontology_texts()` logic from `day_scoring.rs`, but scoped to event time window instead of full day)
2. Embed the combined text via nomic-embed → 768-dim vector
3. Store embedding on `wiki_events` (BLOB column, same format as `wiki_day_embeddings`)
4. Compute `entropy = 1 - cosine_sim(this_event, previous_event)` for sequential transition
5. First event of the day: compare to previous day's last event embedding, or use W6H Shannon entropy as fallback

**Supplementary signal**: W6H Shannon entropy (#1) as a secondary score for events with no embeddable text (e.g., pure heart rate + steps during a run). Cheap to compute, already available from W6H activation.

### Why Compute Both

Shannon entropy and embedding novelty are orthogonal — they diverge in exactly the interesting cases:

| Moment | Shannon (internal complexity) | Embedding novelty (sequential surprise) |
|--------|------|------|
| Deep sleep | Low (pure `who`) | Low (same as prev sleep) |
| Waking up | Low (still mostly `who`) | **High** (very different from sleep text) |
| 3rd work meeting in a row | **High** (whom + what + when + where) | Low (similar content to prev meeting) |
| Sudden crisis call | **High** (many dims active) | **High** (semantically different from anything before) |

Shannon says "this moment is experientially rich." Embedding novelty says "this moment is surprising." A rich but routine moment (meeting #3) has high Shannon, low novelty. A simple but surprising moment (unexpected call during a quiet evening) has low Shannon, high novelty.

**Display**: Blend for the arc visualization: `displayed_entropy = α × embedding_novelty + (1-α) × shannon` where α ≈ 0.7. Primary signal is embedding novelty; Shannon adds the internal-complexity dimension. Both stored separately for transparency.

### Visualization

SVG area chart, same width as timeline bar below it. Smooth curve connecting per-event entropy values. Shaded area under curve. Muted styling — this is context, not the main content. Event boundaries shown as subtle vertical markers aligning with the timeline below.

### Storage

On `wiki_events`:

- `embedding BLOB` — 768-dim nomic-embed vector of event source text
- `entropy REAL` — the computed `1 - cosine_sim(event_n, event_n-1)` score (0–1)
- `w6h_entropy REAL` — Shannon entropy of W6H activation vector (fallback/supplement)

Computed alongside `w6h_activation` in `store_structured_events()`. Embedding uses existing `LocalEmbedder` singleton.

## Q3: Narrative Shape

**What it measures**: What happened throughout the day, when, and for how long.

**Why a timeline, not an arc**: There's no single "Y axis" for a day's narrative. Salience? Heart rate? Productivity? Energy? Moral weight? Too many competing dimensions. The honest representation is a flat timeline with labeled events — the X axis IS the shape.

**Implementation**: LLM identifies 8–16 events during Tollbooth summary generation. Stored in `wiki_events` with `auto_label`, `start_time`, `end_time`, `auto_location`. Displayed as:

- **Timeline bar**: Horizontal bar spanning 00:00–24:00 with colored segments per event
- **Timeline table**: Time, event label, location, duration

**The one exception**: Entropy (Q2 intra-day) is the one Y-axis that makes sense over time, because it's a meta-property of the data itself, not a subjective interpretation. That's why the entropy arc sits above the timeline — it adds a single meaningful vertical dimension.

## Q4: Alignment (Future)

**The question**: Is today's shape conducive to the person I want to become?

**Why it's hard**:

1. **Requires a structured narrative identity document that doesn't exist yet.** The telos doc needs to be W6H-shaped — "who I want to be (self, health, mental)", "whom I want to be with (family, community)", "why I'm doing this (purpose)" — and anchored to a specific future date. Aspirations without a time horizon are unfalsifiable.

2. **Mixes structured and unstructured components.** Habits and todos are structured (did I meditate? did I exercise?). Values and motifs are unstructured (am I becoming more patient? more present?). The comparison mechanism needs to handle both, and the unstructured part requires semantic understanding.

3. **The W6H framework does double duty.** In Q1, W6H measures data coverage (which categories of data are present). In Q4, the same 7 dimensions would measure values alignment (which life dimensions am I investing in). Same numbers, different interpretation. The formula is identical; the meaning changes depending on which question you're asking.

4. **Comparison is non-trivial.** Even with a structured identity doc, computing "how aligned was today" requires either:
   - Cosine similarity between today's W6H profile and desired W6H profile (crude but computable)
   - Semantic similarity between today's event embeddings and identity doc embeddings (richer but requires v2 embedding infrastructure)
   - LLM-based judgment (most nuanced but expensive and non-deterministic)

5. **The identity doc itself needs to evolve.** "Who I want to become" changes. The system needs to handle versioned identity documents and compute alignment against the version that was current at the time of each day.

**When to build**: After event embeddings (v2) and narrative identity doc schema are in place. The comparison layer is an overlay on top of the existing day data, not a replacement for anything.

---

## Data Flow

```
Tollbooth LLM call (generate_day_summary)
    ├── Autobiography text (Layer 3)
    ├── Structured events JSON (Layer 2)
    │     └── For each event:
    │           ├── store in wiki_events (auto_label, start/end time)
    │           ├── compute W6H activation (ontology presence × weights)
    │           └── store w6h_activation as JSON on wiki_events
    ├── Context vector (Layer 1) — binary ontology presence → W6H scores
    └── Chaos score (cross-day entropy)
```

## W6H: The Seven Experiential Dimensions

The W6H dimensions are the irreducible questions you'd ask to fully reconstruct any moment of lived experience. They aren't lenses you choose to apply — they're structural features of any situated moment. Sleep has a `who` whether you measure it or not. A meeting has a `whom` and `where` whether sensors are present or not. The dimensions exist; coverage measures how many you can observe.

| Dim | Question | What it captures | Example ontologies |
|-----|----------|-----------------|-------------------|
| who | Who am I right now? | Body, health, mental state, identity — the subject | heart_rate (0.8), sleep (0.9), workout (0.7) |
| whom | Who else is here? | Other people, relationships — the social field | message (1.0), email (0.9), calendar (0.8) |
| what | What is happening? | Activity, content, events — the substance | transcription (1.0), calendar (0.8), document (0.7) |
| when | When does this matter? | Temporal significance — the scheduling | calendar (0.7), sleep (0.5), location_visit (0.4) |
| where | Where am I? | Place, space, context — the setting | location_point (1.0), location_visit (0.9) |
| why | Why am I doing this? | Purpose, motivation — the intent | transcription (0.8), conversation (0.5), document (0.4) |
| how | By what means? | Method, process, tools — the mechanism | app_usage (0.7), workout (0.6), steps (0.4) |

### Classical Heritage

The impulse to decompose human acts into these questions is ancient. The *septem circumstantiae* (seven circumstances) of classical rhetoric — attributed to Hermagoras of Temnos (1st century BC), refined by Cicero and Thomas Aquinas — asked *Quis, quid, ubi, quibus auxiliis, cur, quomodo, quando*. The original Latin categories overlap considerably (how / by what means / why blur together), and the mapping to our W6H is loose, not literal. But the core insight is the same: a finite set of orthogonal questions can fully characterize any situated human experience. We're building on something traditional, not inventing from scratch, which is equally important for human accessibleness of language as it is for AI models and their training data to understand our paradigms.

## Key Files

| File | Role |
|------|------|
| `packages/virtues-registry/src/ontologies.rs` | Ontology registry: W6H weights, temporal types, source configs |
| `core/src/api/wiki.rs` | Day data API: sources, events, day CRUD |
| `core/src/api/day_summary.rs` | Tollbooth LLM call, event parsing, W6H computation |
| `apps/web/src/lib/components/wiki/DayPage.svelte` | Main day page component |
| `apps/web/src/lib/components/wiki/DayTimeline.svelte` | Timeline bar + table |
| `apps/web/src/lib/components/wiki/DayToolbar.svelte` | Toolbar with coverage %, entropy, generate button |
| `apps/web/src/lib/components/wiki/ContextVector.svelte` | Coverage accordion (7 dimension bars) |
| `apps/web/src/lib/wiki/types/day.ts` | Frontend types: DayPage, DayEvent, ContextVector |
| `apps/web/src/lib/wiki/api.ts` | API client: getDaySources, getDayEvents |
