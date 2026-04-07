# Dayline Autonomic Scoring — Design Document

> The dayline chart shows two per-event z-scored signals on the same ±3σ scale:
> **Novelty** (Novel ↑ / Routine ↓) and **Autonomic** (Stress ↑ / Recovery ↓).
> Together they reveal when your body's response to a moment diverges from your baseline —
> something no wearable can do alone.

---

## The Core Insight

Wearables can tell you "your HR was 78." But they can't tell you:

> "Your HR is usually 70 during design standups. Today's 78 is +2.3σ above your baseline for this type of event."

That requires three things no wearable has:

1. **Event classification** — what were you doing? (calendar + location + app data + LLM summary)
2. **Personal history** — what does your body usually do during similar events? (12 weeks of embedded events paired with HR)
3. **Embedding similarity** — matching today's event to historically similar events (embedding infrastructure)

The dayline has all three. The embedding space IS the activity classifier. "Design standup with Maya and David about onboarding drop-off" captures everything — meeting type, people, topic — in one 768-dimensional vector. No explicit categories needed.

---

## Two Signals, One Chart

### Novelty (Entropy) — "How unusual is this event's content?"

| Property | Value |
|----------|-------|
| **Input** | Event summary embedding (768-dim, nomic-embed) |
| **Method** | Cosine distance from 12-week embedding centroid |
| **Comparison** | Global — all baseline events form one centroid |
| **Question** | "Is this event unusual IN MY LIFE?" |
| **Output** | `novelty_z` on `wiki_events` (±3σ clamped) |

The centroid approach works for novelty because you're asking a global question. Every past event defines what "normal" looks like. The centroid IS your normal. Distance from it IS unusualness.

**What's captured:** Semantic/cognitive novelty — new topics, new people, new places, unusual combinations.

**What's NOT captured (by design):**
- Time of day (waking at 8:30 vs 6:30 — same summary text, same embedding). The LLM hourly action would naturally mention unusual timing in the summary if notable.
- Circadian rhythm (irrelevant — semantic content doesn't vary with biology)

### Autonomic (Physiological Response) — "Is your body responding the way it usually does when you do this kind of thing?"

| Property | Value |
|----------|-------|
| **Input** | Avg HR during event window (primary), HRV/SDNN when available (supplementary) |
| **Method** | Similarity-weighted regression across all baseline events |
| **Comparison** | Local — weighted by embedding similarity + recency (spacetime) |
| **Question** | "Is my body responding unusually FOR THIS TYPE OF EVENT?" |
| **Output** | `autonomic_z` on `wiki_events` (±3σ clamped) |
| **+Y label** | Stress (tooltip: "Sympathetic activation") |
| **-Y label** | Recovery (tooltip: "Parasympathetic dominance") |

The similarity-weighted approach works because you're asking a local question. Comparing your standup HR to your running HR is meaningless. The embedding similarity ensures you're comparing standups to standups, runs to runs.

**Two weighting dimensions (spacetime):**

```
weight = embedding_similarity × recency
```

- **Embedding similarity (space):** `exp(-distance² / 2σ²)` — standups weight heavily for other standups, near-zero for runs
- **Recency (time):** `exp(-days_ago / 21)` — 3-week half-life exponential decay, because your body changes over time

No circadian normalization — the raw circadian shape IS part of the autonomic story. The afternoon dip is real. Morning freshness is real.

**Context-gated HR/HRV composite:**

```
Physical activity events (avg_hr > resting + 2σ):
    autonomic_z = hr_z
    # HRV compresses to near-zero above ~100bpm

Sedentary/cognitive events:
    autonomic_z = 0.3 × hr_z + 0.7 × (-hrv_z)
    # HRV dominates for mental stress (AUC 0.78 vs 0.65)

Sleep events:
    autonomic_z = -hrv_z
    # Resting HRV is the gold standard for recovery
```

**Scoring formula (per signal):**

```
expected_hr = Σ(weight_i × hr_i) / Σ(weight_i)
expected_std = sqrt(Σ(weight_i × (hr_i - expected_hr)²) / Σ(weight_i))
hr_z = (today_event_hr - expected_hr) / expected_std
```

Same formula applies independently for HRV. Both hr_z and hrv_z are stored separately, then combined via context gating above.

**Discordance detection:** When hr_z and hrv_z disagree by >1.5σ, flag the event — these are often the most interesting (early illness, overtraining, hidden stress).

---

## Why Different Methods for Different Signals

| | Novelty | Autonomic |
|---|---|---|
| **Comparison geometry** | Centroid (global) | Similarity-weighted (local) |
| **Why** | "Is this unusual in my life?" needs all events as baseline | "Is my body responding unusually for this type?" needs activity-specific comparison |
| **Temporal weighting** | None (could add recency to centroid as enhancement) | Recency decay (essential — body changes over weeks) |
| **Activity normalization** | Not needed (semantic content is activity-independent) | Essential (HR 78 means different things for standup vs run) |
| **Circadian** | N/A | Raw shape preserved — NO normalization |

---

## The Demand vs Supply Narrative

The two lines on the chart tell a demand-vs-supply story:

- **Novelty line** (dark) = how semantically unusual is this moment? (Novel ↑ / Routine ↓)
- **Autonomic line** (blue) = how is your body responding compared to baseline for this type of moment? (Stress ↑ / Recovery ↓)

| Novelty | Autonomic | Meaning |
|---------|--------|---------|
| High | High | Novel situation, body noticed. Excited, stressed, engaged. |
| High | Low | Novel situation, body was calm. Experienced, composed, flow state. |
| **Low** | **High** | **Routine situation, body activated. Hidden stress. Something's off.** |
| Low | Low | Normal day, normal body. Autopilot. |

The **low novelty + high autonomic stress** case is the killer insight no wearable can surface. "You were doing your usual commute but your HR was 2σ above your commute baseline. What was different?" Maybe anxiety, poor sleep, a stressful text. The chart flags it. The user reflects.

When the autonomic line crosses below the novelty line → **strain** (the moment demands more than your body is equipped for). When autonomic is below novelty → **surplus** (you have capacity for what's happening).

---

## The Exercise Paradox — Solved

The naive "battery drain" model breaks during exercise: running at HR 152 would show as maximally depleted, even though runners feel great.

The embedding-based approach dissolves this entirely:

- "45-minute run on Mueller trails" → find similar past runs in embedding space
- Similar runs had avg HR: [148, 152, 145, 150, 147] → mean 148.4, std 2.6
- Today's run HR: 152
- Autonomic z-score: (152 - 148.4) / 2.6 = **+0.9σ** — slightly harder than usual, not an outlier

Compare to naive resting-HR approach: 152 vs resting 62 = +90 bpm = absurd outlier.

The embedding normalizes exercise automatically because similar events (other runs) ALSO had high HR. The comparison is always apples-to-apples.

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

When available, blend: `signal = 0.6 × hr_z + 0.4 × hrv_z_inverted`

But HRV is sparse (every 1-3 hours from Apple Watch, requires stillness). For V1, HR-only is correct.

### What about a "battery" model?

We explored and rejected the battery/drain model (Garmin Body Battery style). The problem: every day produces the same monotonic curve — charge overnight, drain all day. The variation is small and boring. Per-event physiological deviation is a fundamentally richer signal.

---

## Apple Watch Data Availability

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
- **Day 14+:** Fully personalized. Require N≥5 similar events (combined weight above threshold) to compute autonomic z-score for an event. Events with no similar history fall back to showing no autonomic signal (honest > fabricated).

---

## Event HR Annotation Pipeline

Not chicken-and-egg — sequential pipeline:

```
1. Apple Watch → data_health_heart_rate (raw HR samples, every 5-10 min)
2. Hourly action → wiki_events (event boundaries + summaries + embeddings)
3. Post-processing → annotate wiki_events.avg_hr from HR data in event window:
   SELECT AVG(bpm) FROM data_health_heart_rate
   WHERE timestamp >= event.start_time AND timestamp < event.end_time
4. Autonomic scoring → embedding similarity + HR comparison → autonomic_z
```

---

## Implementation Plan

### Schema Changes
- Add `avg_hr REAL` to `wiki_events` (nullable, populated when HR data exists)
- Add `autonomic_z REAL` to `wiki_events` (nullable, computed by autonomic scoring)
### Scoring Engine
- `core/src/dayline/autonomic_scoring.rs` — context-gated HR/HRV comparison
- Uses existing `vec_search` for embedding similarity lookup
- Composite kernel: embedding_similarity × recency (spacetime, 2 factors)
- Produces `autonomic_z` per event, clamped to ±3σ

### Chart (DaylineChart.svelte)
- Second line on same ±3σ chart (blue/primary color, lower opacity)
- Novelty line = dark, event-driven, spiky
- Autonomic line = blue, event-driven, spiky (NOT monotonic — per-event, not a drain curve)
- Visual treatment for "surprise divergence" moments (low novelty + high autonomic stress)

### Seed Data (for demo/testing)
- Add realistic avg_hr values to demo day events + baseline events
- Compute autonomic_z from seeded data to test the chart

---

## Spacetime: The Unifying Principle

Both scoring methods operate in spacetime — but weight the dimensions differently:

**Novelty** emphasizes **space** (embedding distance from centroid). Time is implicit (the 12-week window defines the baseline, but events within it are weighted equally). The question is spatial: "where does this event sit in the landscape of my life?"

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
- HR 75 + HRV high → mildly activated, plenty of reserve (handling it easily)
- HR 75 + HRV low → mildly activated, no reserve (struggling at this level)
Both HR and HRV are needed for the full picture.

### Established but Nuanced

**SDNN from Apple Watch (60-second windows).** Research standard is 5-minute recordings in controlled position. Apple Watch uses ~60s during uncontrolled daily activity. Correlation with clinical devices: r ≈ 0.8-0.9 in controlled conditions, lower during movement. Directionally reliable but noisier than research-grade.

**The 60/40 HR/HRV weighting.** Our composite formula (`0.6 × hrv_z + 0.4 × (-hr_z)`) is a reasonable heuristic but NOT from published research. No validated formula exists for combining HR and HRV into a single "capacity" score. Garmin's Body Battery is the closest analogue and its formula is proprietary. The 60/40 weighting favors HRV because it more directly measures reserve/capacity, while HR partially reflects demand.

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
