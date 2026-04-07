# Day Page Plan

> "The page should feel like opening a leather journal that somehow already has today's entry written in it — but you get to correct it, annotate it, beautify it, and eventually, over months, it becomes the most honest record of your life that exists."

One sentence: **The day page is a mirror you read at night.**

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
- No completeness indicator on the day page (see W6H section — deferred to monthly/quarterly reflection)

### 4. Dayline Chart Container (one instrument, multiple lenses)

- **Pill selector tabs** above a single chart area:
  - **Dayline** (default): 3D ribbon chart — "What was the shape of my day?" The signature view. The LED on the server.
  - **Energy**: Body battery hourly curve — simple area chart, no 3D. "When did I have gas in the tank?"
  - **Entropy**: Routine vs novelty — "How chaotic vs. ordered was today?"
  - **Topology**: Fragmentation, context-switching, topic density — "Where was my focus?"
  - **Dimensions**: Radar chart of user-defined axes — "Who was I today?" User enters 3-5 word descriptions per axis (3-8 axes). System measures orthogonality/polarity in embedding space and gives feedback.
- **Thin horizontal minibar** at the base of the chart: the compressed timeline bar (current DayTimeline bar chart). Hover syncs with the chart above and the vertical timeline below via `hoveredEventId`.

### 5. Autobiography (system voice)

- 2-5 sentence AI-generated narrative of the day
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
- **Location** (editable on click, entity-resolved: "Home" not "30.2961, -97.7325")
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

### 10. Ontologies (collapsed reference section)

- **"Ontologies (47)"** — collapsed accordion at the bottom
- **One unified table** when expanded: all ontology records interleaved chronologically
- Each row: timestamp, source-type icon as the row marker (tiny calendar, message, pin, etc.), label, preview text
- Dots are visual rhythm only — no metric encoding. The source-type icon *is* the dot.
- This is the power-user audit trail / trust layer

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

## Day Data Quality

### Remove W6H Context Weights

**TODO**: Remove `context_weights: ContextWeights` (the `[f32; 7]` array) and all `CTX_WHO`/`CTX_WHOM`/etc. constants from `ontologies.rs`. The per-ontology W6H weights are deprecated — they were an attempt to deterministically score day completeness but the quality-vs-quantity problem (16 joke emails ≠ 3 breakup emails) makes them unreliable, and maintaining hand-tuned weights per ontology is overhead for questionable value.

### Replacement: LLM-Assessed Data Quality (Hourly Event Cron)

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
- **Display**: Subtle indicator near the header (tooltip or small label). Not prominent — a quiet signal.

### Future: "Virtues Wrapped" (Monthly/Quarterly Reflection)

Aggregate data_quality ratings + ontology record counts across 30-90 days for a periodic reflection feature. "You had 58 rich days, 22 good days, and 10 sparse days this quarter. Your weekends are consistently sparse — consider wearing your watch on Saturdays." This is where structural coverage insights belong — not on the daily page.

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
- Sources: **keep header** — "Sources (47)" as collapsed accordion label

---

## Ontologies: Unified Table

Replace the current 8+ grouped-by-source-type display with one chronological table:

```
 6:30  🛏  Woke up                          sleep
 7:15  📍  Bike commute to office            location
 7:45  💬  Coffee & Slack catch-up           app
 9:00  📅  Design standup                    calendar
 9:12  💬  "onboarding flow feels clunky"    message:slack
```

Source-type icon serves as the row marker (no separate dot). Icons are tiny and muted — visual rhythm, not data encoding. No metric indicators in this section; metrics belong in the dayline chart and timeline detail panel.

---

## Open Questions / Future Items

- [ ] Journal feature: Full journal tab/page is separate work. Day page just shows a window into it.
- [ ] Sketch generation model: DALL-E 3 vs Midjourney API vs self-hosted. Cost is low either way (~$1.20/mo/user). Route through Bridge.
- [ ] How does the in-progress summary compute? Lightweight SQL aggregation on page load vs. a scheduled partial summary (use the event hourl llm cron to also return? -- probably not but it's an idea).
- [ ] GPS breadcrumb smoothing for the temporal map scrubber
