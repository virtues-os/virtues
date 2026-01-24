# The Living Book: Narrative Identity Architecture

> "The unexamined life is not worth living." — Socrates

This document defines the philosophical and technical architecture for Virtues' autobiographical onboarding—a system that helps users author their lives rather than configure their profiles.

---

## The Core Insight

**Authorship, not Configuration.**

Traditional onboarding asks: "What are your values?" (Configuration)
We ask: "What are the chapters of your life?" (Authorship)

**Narrative-Identity First.** We are text-driven, not SQL/tabular for character traits. No structured virtue/vice tables. The AI understands users through their stories, not through tagged attributes.

> "We don't ask what you value. We ask what you've lived."

---

## The Fractal Resolution of a Life

A human life has natural resolution levels:

| Level | Unit | Duration | Description |
|-------|------|----------|-------------|
| **L0** | Telos | Orthogonal | One sentence. The interpretive lens. Lives in profile. |
| **L1** | Act | 3-7 years | Major life phases. The "spine" of the book. |
| **L2** | Chapter | 3-18 months | Narrative arcs within an Act. Emergent from conversation. |
| **L3** | Day | 24 hours | Natural human rhythm. Sleep resets. |
| **L4** | Event | 30min - 6hrs | Calendar-like blocks. Bridge to praxis. |
| **L5** | Action | Minutes | Concrete actions or moments of decision. |
| **L6** | Signal | Seconds | Not autobiographical. Stays in domain tables (health, location, etc.). |

**The Day (L3)** is a powerful abstraction—humans must sleep, creating natural boundaries.

**Events (L4)** map to how people actually calendar their lives, making this the bridge between narrative and praxis.

**For MVP:** Focus on Acts + Chapters. Days/Events/Actions come later via integrations.

### On Telos (L0)

Telos is **orthogonal to the timeline**—it's not at the "top" of the fractal, it's perpendicular to it. The timeline is horizontal (when); telos is vertical (why/toward what).

**Telos is one freeform sentence.** Keep it simple.

**Example:**

```
"To become a Catholic man who creates more than he consumes, to love his wife like Christ loved the church,
and to leave his children better equipped than he was to find truth/beauty/and goodness in the world."
```

Telos lives in `user_profile.telos` (TEXT). It's the interpretive lens through which the timeline gains meaning.

**Note:** Telos is circular—you need it to make sense of your life, but it emerges from your life. The system embraces this by allowing iteration.

---

## The Onboarding Flow: Skeleton to Story

### Phase 1: The Skeleton (60 seconds)

**Goal:** Establish the Acts (L1). The spine of the book.

**Prompt:** "If your life was a book, what are the major Acts? Don't overthink it—could be cliche like 'High School' or 'College'. Just rough titles and years."

**Example Output:**

```
Act I: Growing Up Ohio (1995-2013)
Act II: The College Years (2013-2017)
Act III: Young Adult (2017-2022)
Act IV: Living Intentionally (2022-Present) [Active]
```

**Constraints:**

- Minimum: 4 Acts
- Maximum: 16 Acts (soft limit, warn if exceeded)
- Current/unfinished Act marked as `[Active]`

**Result:** `narrative_node` rows with type='act', dates, title, is_active. Chapters emerge later.

---

### Phase 2: The Vomit Draft (Per Act)

**Goal:** Extract unbridled thoughts. Stream of consciousness. Text input (no voice/SST for MVP).

**Mechanism:** User enters an Act. The Biographer Agent prompts based on the title.

**Example Prompt:**
> "You called this 'The Grind.' That implies struggle. Who were you fighting? Was this building something, or just surviving?"

**User Input:** No grammar concerns. Pure recall and emotion.

**Example Stream:**
> "It was mostly about proving my dad wrong honestly. I was working 80 hours at the agency. I felt numb but effective. I drank too much coffee. I lost touch with Sarah during this. But I learned how to code."

**Chapter Discovery:** As the user narrates, Chapters (L2) emerge naturally from the conversation. The AI identifies sub-arcs and proposes them:
> "It sounds like there's a distinct chapter here around 'The Burnout' in late 2019. Should we break that out?"

This is iterative—Chapters are discovered through narrative, not pre-defined.

---

### Phase 3: The Alchemist (AI Processing)

The AI processes the raw stream into **polished narrative only**:

> "A period defined by high-intensity professional output and emotional suppression. Motivated by a desire for external validation (proving father wrong), the user sacrificed personal relationships and physical health to acquire technical mastery."

**What we DON'T do:**

- No structured virtue/vice extraction
- No "AI tagging virtue" to database tables
- No axiological tables at all

The narrative IS the data.

---

### Phase 4: The Mirror (Review)

**Goal:** Validation of soul, not just facts.

**AI presents the polished Chapter/Act narrative.**

**The Magic Question:**
> "Does this sound like who you were then?"

User approves or edits. Move to next Act/Chapter.

---

## Romanticizing the Hard Parts

The system should gently encourage users to *include* their suffering, faults, and hardships—not as failures but as part of the journey.

**The Biographer might say:**
> "Some of the most meaningful chapters are the hardest ones. The breakdowns, the losses, the times you weren't your best self. These aren't stains on your story—they're where the story gets interesting."

The goal is to normalize including the hard stuff, framing it as *narrative richness* rather than *confession*. Users should romanticize their suffering as part of their beautiful journey, not hide it.

---

## Gaps and Silences

Not every period needs to be detailed. Users may have times they don't want to discuss—trauma, shame, "the lost years."

**Two types of gaps:**

| Type | Example | How to Handle |
|------|---------|---------------|
| **Private** | "2015-2017 was hard. I don't want to detail it." | Allow acknowledgment without content |
| **Stuck** | "I think about this loss too much. I need closure." | Note existence, don't force elaboration |

**Valid input:**
> "There's a chapter here—call it 'The Dark Years' (2015-2017). I've worked through it and prefer not to document the details."

This is meaningful narrative data. The AI knows something significant happened, knows not to probe, knows the user has processed it.

**Don't force completeness.** Gaps are data too.

---

## The Current Chapter (The "Now")

The current Act/Chapter is **unfinished and malleable**—marked `[Active]` in the spine.

For MVP: Treat it as editable like any other. No special handling.

**Conceptually:** The "now" is where praxis and narrative meet. Past Acts inform where you're going; future aspirations inform how you see the past. The present chapter is the synthesis point.

---

## Revision Model

**Full user control.** No locking. Users can edit any Act/Chapter anytime.

If a user finds a deeper "why" later, they update the narrative. New understanding doesn't replace the old—it enriches the story.

The system doesn't enforce versioning for MVP. Users simply edit.

---

## Entity Resolution (Future Enhancement)

While we don't extract structured virtues/vices, we DO want entity resolution for **people, places, and things** mentioned in narratives.

**Approach:** Custom markdown-like syntax for inline entity references:

```markdown
I lost touch with <person id="sarah-chen">Sarah</person> during this time.
I was working at <org id="acme-agency">the agency</org> in
<place id="chicago-il">Chicago</place>.
```

**Benefits:**

- Pure markdown (no JSON blocks)
- Enables entity graphs without losing narrative flow
- Can link to `entities_person`, `entities_place`, etc.
- Future: GRAPHRAG-style traversal across Acts

**Note:** This is a future enhancement, not part of MVP onboarding.

---

## The ContextVector Model

Life isn't stateless. Every moment exists within a **ContextVector**:

| Dimension | Question | Example |
|-----------|----------|---------|
| **Who** | Who were you? Who was there? | "The anxious founder", "Sarah, my cofounder" |
| **What** | What was happening? | "Fundraising Series A" |
| **When** | Where in your story? | "Year 2 of The Grind" |
| **Where** | Physical/digital location? | "SF, remote, coffee shops" |
| **Why** | What was driving this? | "Proving competence" |
| **How** | What was your mode of operation? | "Grinding, suppressing emotion" |

**Per-Chapter Completeness:** The system can prompt users when a chapter is missing dimensions:
> "This chapter is light on 'Who'—were there important people during this time?"
> "I'm not seeing much 'Why'—what was motivating you here?"

**Time** (`when`) is the most powerful dimension—the linear thread that gives everything else meaning.

---

## The Living Book Interface

### View 1: Table of Contents (The Vertical Spine)

Academic, clean, classy. The primary navigation.

```
Act I: The Origin (1990-2008)
  └─ Chapter 1: Small Town Kid
  └─ Chapter 2: First Failures
Act II: The Chaos (2008-2014)
  └─ Chapter 3: Finding My People
  └─ Chapter 4: The Breakup
Act III: Building (2014-2022)
  └─ Chapter 5: The Grind
  └─ Chapter 6: The Pivot
Act IV: Awakening (2022-Present) [Active]
  └─ Chapter 7: Finding Peace
  └─ [Current Chapter - In Progress]
```

### View 2: The Editor (Future)

A Notion-like beautiful document editor for deep chapter work. Not part of onboarding MVP.

---

## The Biographer Agent

### Role

An empathetic ghostwriter who separates:

- **Cognitive Load** (user recalls and feels)
- **Executive Function** (AI structures and polishes)

### Tone

- Thoughtful friend, not coach
- Retains emotional edge—no "corporate polish"
- **Bad:** "The user focused on professional development."
- **Good:** "A chapter of aggressive expansion at the cost of intimacy."

### Key Behaviors

1. **Contextual Prompting:** Infer from Act/Chapter title before asking
2. **Chapter Discovery:** Identify sub-arcs and propose them
3. **ContextVector Gaps:** Notice missing dimensions (who/what/when/where/why/how)
4. **Mirror Questions:** "Does this sound like who you were?"
5. **Encourage Inclusion:** Gently prompt for the hard parts as narrative richness

### Tools

```typescript
create_act(title, start_year, end_year?, is_active)
propose_chapter(act_id, title, start_date, end_date, reasoning)
create_chapter(act_id, title, start_date, end_date)
save_raw_input(node_id, raw_text)
save_narrative(node_id, narrative_text)
save_telos(telos_text)
mark_narrative_complete()
```

---

## Data Architecture

### Core Table: `narrative_node`

```sql
CREATE TABLE narrative_node (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  parent_id UUID REFERENCES narrative_node(id),
  type TEXT NOT NULL CHECK (type IN ('act', 'chapter', 'day', 'event', 'action')),
  title TEXT NOT NULL,
  narrative_text TEXT,
  raw_input TEXT,
  start_date DATE,
  end_date DATE,
  is_active BOOLEAN DEFAULT FALSE,
  metadata JSONB,
  embedding vector(768),
  embedded_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ DEFAULT NOW(),
  updated_at TIMESTAMPTZ DEFAULT NOW()
);
```

### Importance & Visibility (Purely Narrative)

No structured salience/visibility fields. Instead, importance and AI behavior preferences are captured in the narrative itself:

> "This chapter was the most transformative of my life, but it's painful—I'd rather not have it come up unless I bring it up myself."

The Biographer prompts for emotional relationship to each node:
> "How do you feel looking back at this time? Formative? Painful? Something you'd rather not dwell on?"

This gets woven into `narrative_text`. The AI reads and respects natural language instructions about how to handle each part of the story.

### Telos in Profile

```sql
ALTER TABLE user_profile ADD COLUMN telos TEXT;
```

One freeform sentence. That's it.

### Embedding Strategy

**Embed:** Acts, Chapters, Days, Events + ontological entities (people, places, transactions, etc.)

**Don't embed:** Actions, Signals (too granular / not normalized)

### What We're Deprecating

The following tables should be removed:

- `axiology_telos` → replaced by `user_profile.telos`
- `axiology_virtue` → removed
- `axiology_vice` → removed
- `axiology_temperament` → removed
- `axiology_preference` → removed

Character is captured in story, not in rows.

---

## Decisions Made

| Question | Decision |
|----------|----------|
| Voice vs Text | Text only for MVP. No SST/TTS. |
| Act Count | Min 4, Max 16 (soft limits with warnings) |
| Chapter Discovery | Emergent from conversation, not pre-defined |
| Axiological Tables | Remove. Narrative-identity first. |
| Entity Resolution | Future enhancement with inline markdown syntax |
| Current Chapter | Marked as `[Active]`. Editable like any other. |
| Telos | One TEXT field in user_profile. Freeform sentence. |
| Gaps/Silences | Allow minimal-content Acts. Don't force completeness. |
| Revision | Full user control. No locking. |
| Hard Parts | Encourage inclusion as narrative richness |
| Importance/Visibility | Purely narrative. No salience fields. AI reads natural language. |
| Derivation/Inference | Not for MVP. All nodes explicit. |
| Days/Events/Actions | Future. Focus on Acts + Chapters for onboarding. |

---

## Open Questions

1. **Privacy Gradient:** Which narrative data goes to LLM vs stays local?
2. **Chapter Granularity:** How do we prevent over-fragmentation? (Soft cap of 3-6 chapters per Act?)
3. **Cross-Act Themes:** How does AI notice patterns across Acts? (e.g., "Sarah appears in three Acts")

---

## Next Steps

1. **Create `narrative_node` table** and migration
2. **Add `telos` field** to user_profile
3. **Build Biographer Agent tools** (create_act, propose_chapter, etc.)
4. **Prototype the onboarding chat flow**
5. **Design Table of Contents UI** (vertical spine)
6. **Deprecate axiological tables**
