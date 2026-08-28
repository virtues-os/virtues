# Stories: Resolution, Significance, and the Narrative Architecture

> **SHELVED — cut from v1 on 2026-07-22.** The claim-style story (a thesis whose
> body is a rendered, cited account gathered by the magnet) is not being built:
> a spike on real data could not establish who it helps, and the plumbing needed
> real work before that value was ever proven. `wiki_stories` + `wiki_story_members`
> are dropped (migration 0060); `app_notebooks` is the surviving primitive, kept
> under the name "notebooks". Significance (the six-witness quorum) was already
> cut on its merits in migration 0038. This document is retained as design
> history — the reasoning and the vocabulary are still worth having if the
> feature is ever revisited — but nothing here is live. The durable technical
> findings from the spike (the magnet's dead centroid dimension, the reranker
> scale mismatch, and "don't semantically index structured data") live with the
> magnet code and the notebook fix, not here.

*Design document — 2026-07-11. Captures the full design exploration of entity resolution, temporal resolution, significance, and the `wiki_stories` unification. Extends and partially supersedes [notebooks-plan.md](../notebooks-plan.md) (Notebooks fold into Stories) and builds on [the-day.md](../the-day.md) (the day page / event timeline is the temporal spine this attaches to).*

---

## 0. The essence, in one line

> Every datum → **{stay as dust | resolve}** → if resolve: **into what** → **attached to which record** → **weighted by significance (why)**.

Virtues receives a life's raw exhaust (locations, messages, transcripts, health signals, journals, purchases) and must decide what stays as unstructured, searchable **dust** and what gets **resolved** into narrative structure — entities, topics, events, stories — and with what weight. "Stay as dust" is the default branch and the majority case. Most of a life should resolve to nothing, and that is the anti-cosmic-meaning valve: the system must never manufacture meaning out of shopping lists.

Two invariants sit above everything else in this document:

1. **Narrative is a lossy, re-derivable sample of evidence — never the reverse** (§3). The data lake holds the integral; the biography samples it.
2. **Maintenance is bounded by what the user chose to claim** (§6). Claim nothing, maintain nothing. The AI owns what it inferred; the human owns what they touched; the AI never silently restructures the human's.

> **V1 SCOPE (decided 2026-07-13, plan in §8).** V1 builds the segmentation + resolution foundation: **entity resolution (people/places/orgs — `wiki_things` deprecated), event resolution + class-by-neighborhood + topics, nightly day summaries, hand-made stories (magnet-or-folder), `wiki_marginalia`, and `reference_time` delivery (no prospective table — the future is a query).** The composed narrative layers — AI-discovered motifs, the axiology tree (virtues/valence/direction), the examen, discovery — are **designed and deferred**: they arrive later as additional readers/writers of the same rows. §4–§7 describe the full design; §8 is what gets built now.

---

## 1. Grounding

The architecture was not designed from these sources, but it converged on them from engineering constraints — which is the strongest signal it's right. When measure theory and cascade-consistency independently produce the same asymmetry, the asymmetry is probably true.

### 1.1 Philosophy & theology

- **Aristotle, *Nicomachean Ethics*.** Every action aims at some good; the final end (*telos*) is eudaimonia — flourishing over "a complete life." Virtue is a *hexis*: a stable disposition acquired by habituation. Book III's voluntary/involuntary distinction — praise and blame attach only to the voluntary — is why **agency is a witness in the significance formula**: moral appraisal requires knowing what a person *did* versus what merely *happened to them*.
- **MacIntyre, *After Virtue* (1981).** "Man is in his actions and practice… essentially a story-telling animal." The intelligibility of any action requires a narrative; a good life has *narrative unity*. His three-level scheme — practices → narrative unity of a life → tradition — maps directly onto events → stories → telos.
- **Ricoeur, *Time and Narrative* (1983–85).** The keystone. *"Time becomes human time to the extent that it is organized after the manner of a narrative."* Emplotment (*mise en intrigue*) mediates between lived phenomenological time and cosmic clock time. His threefold mimesis — life as already story-shaped (mimesis₁), configured into a story (mimesis₂), the story reshaping how life is then lived (mimesis₃) — **is literally this pipeline**: raw events → stories → telos → changed living. *Oneself as Another* (1990) adds narrative identity proper: a person is the character of their own story.
- **Heidegger, *Being and Time* (1927).** *Thrownness* (Geworfenheit — the facticity you find yourself in; what befalls you) versus *projection* (Entwurf — the possibilities you throw yourself toward; your projects). This pair is the ontology behind the **agency witness** and behind declared-vs-discovered stories.
- **Ignatius of Loyola, *Spiritual Exercises* (1548).** The Examen: a structured daily review — gratitude, review of the day's movements, consolations and desolations, resolution for tomorrow. It is precisely a **plan-vs-actual, affect-tagged daily retrospective**. The declared-vs-revealed delta (§5.3) is the Examen made computable.
- **Augustine, *Confessions*.** The first narrative-identity autobiography; Book XI's *distentio animi* (past as memory, present as attention, future as expectation) is the ancestor of the bitemporal model. **Kierkegaard** (*Journals*, 1843): "Life can only be understood backwards; but it must be lived forwards" — the one-line justification for out-of-order evidence, backfill, and non-monotonic resolution.
- **Being vs. becoming** (Parmenides/Heraclitus → Plato's *Timaeus* → process thought). The telos document is a *becoming* artifact — perpetually revised, never final. The declared/revealed delta is being-vs-becoming rendered as data.

### 1.2 Psychology

- **McAdams, *The Stories We Live By* (1993); McAdams & McLean (2013).** Identity *is* the internalized, evolving life story integrating reconstructed past and imagined future. Narrative themes of **agency** and **redemption** empirically predict well-being. The `wiki_stories` table is McAdams' life-story model as schema. Corollary: **narration is itself a mattering signal** — if the user journaled about it, it mattered.
- **Bruner (1986).** Narrative vs. paradigmatic modes of thought — why the product surface is memos and stories, not dashboards and ops queues.
- **Damasio, somatic marker hypothesis (*Descartes' Error*, 1994).** Bodily signals tag options with value before deliberation. Grounds the **arousal witness**: heart-rate and HRV deviations are honest, pre-verbal testimony about what mattered. No notes app has this signal.
- **Memory science.** Self-reference effect (Rogers et al. 1977): self-relevant material is remembered better. Von Restorff (1933): the novel item is disproportionately remembered — the ancestor of z-scored surprise. Flashbulb memories (Brown & Kulik 1977) — with Neisser's caution that vividness ≠ fidelity, a warning against over-trusting any single signal. Peak-end rule (Fredrickson & Kahneman 1993): the remembering self keeps peaks and endings, neglects duration — the day summary should too.
- **Samuelson, revealed preference (1938).** Infer value from choices made, not introspection. The **cost/investment witness** is revealed preference; the telos is stated preference; the divergence between them is the product's deepest insight.
- **Sellen & Whittaker, "Beyond Total Capture" (CACM 2010).** The canonical critique of lifelogging (MyLifeBits lineage, back to Bush's memex, 1945): capture is easy; **retrieval-with-meaning is the hard problem**. The story/telos layer is the answer to that critique.
- **Complementary Learning Systems (McClelland, McNaughton & O'Reilly, *Psychological Review* 1995).** The brain solves this exact architecture problem with two systems at two cadences: fast episodic capture (hippocampus) and slow consolidation into semantic structure (neocortex, largely during sleep). The hourly hot pass (capture, segment, attach) and the slow batched passes (adjudicate, re-summarize, discover) are the same division of labor — **the two-cadence pipeline is how memory actually works**, not just a cost optimization.

### 1.3 Mathematics

- **Measure theory — the sum/integral asymmetry.** A sum is an integral against a counting measure; it sees only discrete, enumerable points. An integral against Lebesgue measure sees the continuum. The narratable events of a life — the stories — are a **set of measure zero** against the lived continuum. Consequences: (a) the **lattice renders the continuum** — an empty day page still exists, because the day was still lived (calendars and photo apps are counting-measure technologies; Virtues is a Lebesgue technology); (b) **stories are sparse** — they never tile time, and un-storied time is not "unknown," it is simply time; (c) **narrative is a lossy sample of context, never the reverse** — you can re-integrate new stories from the evidence, but you cannot recover evidence from stories. The data lake holds the integral; the biography samples it. *That asymmetry is the architecture.*
- **φ(t), the integrand.** The poetic version ("depth of presence") is unmeasurable and must not be scored — computing a "life quality number" is pseudoscience in calculus clothing. The honest version: **φ(t) ≈ life-signal density** — salience density plus context entropy (how fragmented vs. focused, how fast context is shifting — chaos vs. order). Every component already exists (`novelty_z`, `hr_z`, `autonomic_z`, entity/topic counts). **The Dayline is the plot of φ(t).** It stays descriptive (intensity), never evaluative (quality).
- **Geometry — stories live in a metric space.** Each story is a **centroid** in embedding space (cosine metric); membership is proximity plus explicit overrides. The structural dynamics are geometric predicates, not LLM judgments: **emerge** = a dense, time-spanning cluster among unattached significant points; **merge** = two centroids converge below a distance threshold; **split** = intra-story variance goes bimodal or the centroid drifts far from its origin; **die** = no new attachments and decayed weight below floor. Drift is self-correcting: a story that wanders *is* the split signal. Cheap geometry detects; the LLM only adjudicates; the prudence gate decides whether to ask.
- **Calculus of decay.** Attention retention follows `exp(−λ_kind · Δt)` — the Ebbinghaus forgetting-curve shape — with kind-specific half-lives: a reminder decays in days, a story in months, a virtue and the telos effectively never (λ ≈ 0). Decay is computed at read time from `last_touched_at`, never stored — impermanence as a function, not a mutation.
- **Statistics of surprise.** Surprise is z-scored deviation from *personal* baseline (LOF/density on embeddings; z-scores on physiology), never a global norm. Two cautions: z-scores are **relative**, so an absolute floor is required or a boring week manufactures a false peak; and baselines need history, so new users start on a weak global prior that converges to personal over weeks.
- **The algebra of the significance sum.** Additive, not multiplicative — and this is a *safety property*, not a stylistic choice. Multiplication lets any zero witness annihilate the whole: a declared telos would zero out everything undeclared (your mother vanishes because she isn't on your stated career path). Addition means **any one witness can testify alone**. Independence of witnesses is what protects the undeclared life.
- **Bitemporality.** Every piece of evidence carries two time coordinates: `reference_time` (when the described thing happened/happens — event time) and `ingest_time` (when we learned it — knowledge time). They are independent axes; most of the system's hard cases (backfill, prospective events, out-of-order resolution) are just motion in this 2-D time plane. (Production analog: Zep/Graphiti's bitemporal knowledge-graph edges, arXiv 2501.13956.)

### 1.4 Technical state of the art (2024–2026)

The converged ER pipeline in the literature matches the design here: **embedding-based blocking → LLM pairwise/cluster adjudication** (Peeters & Bizer, arXiv 2310.11244; in-context clustering ER, SIGMOD 2025 — batch records per call for ~5× fewer API calls; BoostER, WWW 2024 for cost routing). Incremental/streaming ER is mature (Gruenheid et al., VLDB 2014; progressive ER, 2025) but LLM-in-the-loop *streaming* ER is open ground — this system is at the frontier, not behind it. Dedicated NER models (GLiNER-class) are optional pre-filters only; extraction is a small-LLM structured-output task because the ontology is idiosyncratic (projects, pets, concepts) and extraction must co-emit normalization, typing, roles, and `reference_time`.

**Graphiti/Zep (arXiv 2501.13956) — adopt four principles, not the stack.** Researched in depth (2026-07): the closest production analog (temporal KG agent memory). Wholesale adoption fails on verified facts: Python-only, **no Postgres backend** (Neo4j/FalkorDB required — a stack transplant for a single tenant's ~10⁴–10⁵-node graph that recursive CTEs over `wiki_entity_refs` handle); it has **no dust concept** — every episode fires multiple LLM calls (extract, dedup, invalidate), which is the #1 practitioner complaint ("the bill scales with how much you ingest") and exactly the resolve-everything anti-pattern the gate prevents. Notably, **Zep deprecated its LLM-rated fact-importance feature (Feb 2026)** — a funded company tried LLM-assigned significance and retreated; evidence *for* witness-based significance over asking a model "how important is this?". Their "context lake" is enterprise positioning (a governed fleet of per-user graphs — multi-tenant RBAC/audit); single-tenant by doctrine, we have no such problem. Their communities are graph-topological (label propagation), not thematic — centroids stay ours. **Adopt four named principles:** (1) bitemporal edges with four timestamps (`valid_at`/`invalid_at` + `created_at`/`expired_at`), invalidate-don't-delete; (2) episode-as-provenance — every derived node/edge points back to the raw records that produced it; (3) candidate-generation-then-LLM-judge dedup (independently converged with our blocking→adjudication design); (4) recipe-style retrieval, especially **node-distance-from-a-focal-node** reranking for story-scoped search.

For the segmentation upgrade path: **Bayesian Online Changepoint Detection** (Adams & MacKay 2007) is the algorithm family the hourly cron would graduate to; **HDBSCAN** (Campello et al. 2013) is the natural fit for the discovery pass (density-based, no preset cluster count, handles noise — dust — natively).

---

## 2. The ontology: two axes and a hinge

### 2.1 Time is a coordinate system, not an entity

The temporal lattice — seconds → hours → **days** → months → years → life — is **deterministic scaffolding that exists whether or not anything happened**. It is indexed against, never resolved. There is no "day resolution" or "chapter resolution" as ER problems; days and years are coordinates. The empty day page is a feature: it renders the integrand, the continuum that was lived even when nothing was enumerable.

Purely linear, exclusive, deterministic. No context lives on this axis.

### 2.2 Space (context) is the W4H

*Contextulae*: infinitesimal, stateless snapshots of ontologies — the raw data rows. Who/What/Why/How, with **When demoted from content to coordinate**: a snapshot doesn't *have* a when the way it has a who; it is *located at* a when (point or interval). Context is measured, never authored. Entities (people/places/things/orgs) are the **atoms** of this axis; stories are the **clusters**.

### 2.3 Stories are the hinge: context over time

**Story = contextulae ∫ time** — context integrated over when, in a way that is recurrent, coherent, or significant. Not necessarily linear or contiguous: a story's **temporal support is a set of intervals**, not one. One short interval reads as an *event*; one long interval as a *chapter*; many scattered intervals across years as a *thematic arc* (scuba over 8 years; an oil-painting weekend; "research on consciousness"). Resolution is scale, not kind.

Stories serve both registers: the dramatic (relationships, arcs, becoming) and the flat (research projects, notes, collections — the NotebookLM use case). The machinery is identical; only display language differs, chosen by the LLM at write-time from content (no `kind` enum — any taxonomy the data doesn't generate is a tax on the user).

### 2.4 The axiological tree and the two directions

```
        VALUE flows DOWN  (normative — "why")
        telos ──▶ stories (nested: virtues, projects, pursuits…)
                                 ▲
        EVIDENCE flows UP (compositional — "what happened")
                        events ──┘
```

Two tables on the axiological axis:

- **`wiki_telos`** — the root. One document: the narrative-identity statement of who the user is and is becoming. The user's single required act of authorship. Never a "level-1 story"; it is the **axis everything is measured against**.
- **`wiki_stories`** — a nested tree (`parent_id`) beneath it. Virtues, vices, projects, research, aspirations, motifs, notebooks, arcs, chapters — all one primitive, differentiated by fields (§4), not tables. "Become patient" parents "meditation practice"; "build a company" parents "learn sales."

**The story is the only object that is both a means (hangs off the telos — axiology) and a container of evidence (events attach to it — mereology).** Value descending meets evidence ascending, and they meet at the story. That is why the primitive deserves the name.

The tree is simultaneously three ladders — **value abstraction** (telos → virtue → project → act), **timescale of change** (decades → years → months → hours), and **context breadth** (whole life → domain → pursuit → moment) — because *what changes slowly is what is general*. The temporal structure comes free from the value structure.

**Events stay a separate table** (`wiki_events`), and the operational rule that ends the confusion:

> **Events live inside days. Stories live across them.**

Litmus: does it have a clock start/end? → event ("dinner at tweetys, 9–10:30pm"). Is it a theme that accrues members? → story ("Austin trip" — multi-day, contiguous support; "scuba" — 8 years, scattered support). Anything super-day or non-contiguous is a story. Events are born by **segmentation** (boundary detection); stories are born by **centroid attraction** (declared or discovered) — formation mechanics are axis-specific and never shared. **Corrigibility mechanics (§3.6) are uniform** across events, days, and stories — one law for all derived prose, two birth processes. This is also why `wiki_acts`/`wiki_chapters` fold into stories eventually but events never do — fold them **last**, after stories/virtues/notebooks are proven.

---

## 3. Architecture: append-only evidence, derived narrative

### 3.1 The hardest problem in the system

Not entity resolution (bounded, playbooked, visible failures). The hardest problem is **keeping a mutually-dependent graph coherent as evidence arrives out of order and overturns prior beliefs**:

1. **Circular dependency** — WHO/WHEN/WHERE co-determine each other; there is no topological resolution order. Whichever axis is *certain* pins down the axis that is *uncertain* ("Mike" resolves via the 3pm meeting; "last Tuesday" sharpens via the dinner-with-Sarah event).
2. **Out-of-order evidence** — ingest order ≠ event order, always.
3. **Non-monotonicity** — new evidence *overturns*, not just adds (the Sarah-A link was actually Sarah-B).
4. **Cascade** — one flipped link invalidates event rollups, day summaries, story memberships, deltas.
5. **The killer: these failures are invisible and compound.** A mislink can be seen and fixed; an un-cascaded revision silently lies forever. For a product whose promise is a faithful record of a life, **silent drift is death**.

### 3.2 The resolution: projection, not patching

**Evidence is append-only and immutable** — every record, every mention, every resolution decision, with confidence and both time coordinates. The narrative layer (timeline, entity links, day summaries, stories, deltas) is a **deterministic, re-derivable projection** of the log. Revision = append new evidence + re-derive the affected window. Corrigibility falls out (the log is the history). Bitemporality falls out. Recompute cost is what free local compute is for.

**No dependency tracking.** "A used B to derive C" bookkeeping explodes combinatorially. Instead: **time is the dependency graph.** Every datum is temporally anchored, so blast radius = a time window:

```
new evidence → stamp reference_time → mark window DIRTY → re-derive window
```

The one non-temporal cascade — an entity re-link — reduces to time windows cheaply via `wiki_entity_refs` (which records reference that entity → which days → mark dirty).

**Human edits are evidence — the highest-confidence kind.** Re-derivation respects them, never clobbers (`is_user_edited`, `user_label`, `last_edited_by` already encode this pattern).

**Scope note:** free dirty-window re-derivation applies to *hot* objects and to derived **data** (rollup caches, memberships, indexes). Derived **prose** that has settled is governed by the period close (§3.6) — margins accrue automatically; regeneration waits for human acceptance.

**Corrigibility has three legs:** provenance/logging (have it), revisability (resolutions are not write-once), non-destructive supersession (prior belief kept, so "what did we believe, and when" is answerable).

### 3.3 Attachment invariant

**Evidence binds to the record, never to the event.** `wiki_entity_refs (source_table, source_id)` is the authoritative edge; `wiki_events.entities` is a derived, read-only rollup cache (kept, but demoted — never hand-edited, rebuilt on re-segmentation). Events get re-cut; records are immutable; provenance lives at the bottom. Forward references need a second, distinct edge type: *evidence*-attachment (the text mentions Sarah) vs. *relevance*-attachment (Sarah may attend the future party). DRY means single source of *truth*, not single copy.

### 3.4 Two layers: Resolution then Appraisal

- **L1 — Resolution (build the true record):** raw → entities / topics / events / dust. Deterministic where possible, agentic where hard. Runs first, cheap, local. (Topic resolution is the easy member of the family — loose, non-entity theme tags per event, one cheap LLM emission during the hourly pass; no identity problem, no adjudication.)
- **L2 — Appraisal (make meaning of the record):** resolved structure → day summaries, story summaries, deltas, the examen, surfaced reminders. Consumes L1. Runs lazily, batched.

Day "resolution" is L2 rollup, not resolution. The morning-after journal entry ("didn't have a good time last night") splits across the seam: its *facts* backfill L1 (an event annotation with provenance — factual backfill vs. **appraisal**, the affective layer sensors can't capture); its *valence* feeds L2.

### 3.5 Temporal delivery: one mechanism, three faces

Backfill (evidence about the past), prospection (evidence about the future — the party email creates a *planned* layer that reality later reconciles against, never overwrites), and delta-detection (calendar says X, location says Y) are **one engine**: route evidence whose `reference_time ≠ ingest_time` to its timeline anchor; note conflicts. Plans are preserved beside actuals — the **planned/actual delta** is the richest purely-temporal insight (fulfilled / missed / diverged), and *diverged* is the most interesting: it reveals unstated values.

Routing uses cheap relevance (cosine — which measures *aboutness*), and relational temporal references resolve **deterministically on the lattice**: "the place after tweetys" = the successor event on that night's timeline; no LLM guessing. One caution learned the hard way: **cosine cannot detect contradiction** — "I loved tweetys" and "I hated tweetys" are embedding-*near* (same entities, same vocabulary). Contradiction detection is a valence/assertion comparison, not a distance.

### 3.6 Corrigibility: the period close

The stability model for all derived prose (events, days, stories). Like accounting: books open → free edits; books closed → adjusting entries require sign-off.

```
HOT (today/this week — the user hasn't "received" it yet):
    the AI synthesizes and revises freely. No stability contract exists.

SETTLED (read, or aged N days — the books close):
    1. Evidence and notes keep appending to the object's MARGIN — always, automatic.
    2. Prose NEVER regenerates on its own.
    3. Accumulated margin notes raise a quiet badge:
       "3 notes since this was written — refresh summary?"
       Accept → minimal-diff re-synthesis (which reads the margin first).
       Ignore → prose stays; the margin carries the truth. Forever is fine.
```

Three rules make this safe rather than merely simple: **(a)** the badge threshold counts *quorum-clearing* notes, not raw notes (five trivia ≠ one bombshell); **(b)** **mechanical reference updates are exempt** — an entity re-link (Sarah-A → Sarah-B) is a deterministic substitution applied automatically, not a re-synthesis awaiting a click; **(c)** **the AI reads margins too** — retrieval and chat include notes, so stale prose is cosmetic, never epistemic: the assistant sees "user later said tweetys wasn't great" even if the summary was never refreshed. Cascades become **offers, not rewrites**: an accepted event-refresh drops a margin note on its parent day ("an event in this day was revised"), which raises the day's own badge — same rule at every level, human-ratified at every level. Attenuation is natural: most changes don't survive compression into the parent's summary, so most badges never climb. Human retrospective appraisal **outranks AI-inferred valence**, superseded non-destructively ("we inferred you enjoyed it; you corrected us on 8/11" — visible provenance).

**Fluents: correction vs. change.** States of the self (preferences, traits, relationships) are **fluents** — facts with validity intervals, not timeless attributes. "I don't like that candy" has two readings: a *correction* ("the record was always wrong") or a *change* (the unsaid "anymore" — the past liking was **true** and has now *ended*). The discriminator is `reference_time`: anchored to a past event ("that trip to tweetys… didn't like it") → correction of that record; unanchored present-tense assertion → terminate the interval **now**, touch nothing behind it. **Ambiguity defaults to change, never correction** — a wrong termination loses nothing and is reversible; a wrong retro-correction rewrites true history en masse (and is a perfect prudence-gate question when it matters: "never liked it, or went off it?"). The invariant, which is also a defense against human consistency bias — memory's own habit of retro-aligning past feelings with present ones, the exact failure a life-log exists to resist:

> **The present may close the past's intervals. It may never edit their contents — unless it explicitly refers to them.**

---

## 4. `wiki_stories`: the unified primitive

One table absorbs Notebooks, virtues, vices, projects, research, aspirations, motifs, arcs. The former Notebooks concern (source collections for IR) is subsumed: a story with explicitly pushed members *is* a folder; centroid attraction makes it a magnet. Scoped retrieval over a story beats NotebookLM because the corpus includes what the user *lived*, not just what they uploaded — uploaded sources, conversations, events, and notes in one retrieval context.

### 4.1 Fields

```sql
wiki_stories
  id, title, parent_id → wiki_stories        -- the axiological tree
  telos_id → wiki_telos                       -- root anchor (nullable)

  origin        ai | user                     -- provenance, permanent
  state         aspiring | active | dormant | completed
                -- display: Someday | Active | Quiet | Done
  valence       positive | negative | neutral -- virtue | vice | project…
  direction     cultivate | remove | pursue | none
  completable   bool                          -- todos/projects true; virtues false

  centroid      vector                        -- current value; history in wiki_revisions
  significance  float                         -- computed, shown as ordering only
  last_touched_at                             -- decay clock (weight computed at read)

  auto_title / user_title                     -- parallel authorship columns
  auto_summary / user_summary                 -- AI re-derives auto_*, never user_*
  abstract      text                          -- running top-level paragraph (soft-updated)

  last_edited_by  ai | human                  -- AUTHORITY (see §6)
  pinned        bool                          -- user's binary "this matters"
```

**Companion primitives (trimmed 2026-07-13 — tables arrive with their writers):**

```sql
wiki_marginalia -- MEMORY (prose). The margins of everything: AI + human memos per object.
  subject_type/subject_id (event|story|day|entity|telos), kind (observation |
  style_note | correction | appraisal | memo), body, author (ai|human), created_at
  -- "user hates the phrase 'quality time'"; "corrected location 3/2"; the margin
  -- of §3.6. Read-before-write on every re-synthesis. Indexed for retrieval:
  -- chat reads margins, so stale prose never misleads the assistant.

er_mentions   -- EVIDENCE. Extraction output: surface, type, status (floating|
  linked|dismissed|promoted), entity link, embedding, reference_time+granularity.
  -- Mention-level, NOT a reference_time column on every data_* table, because
  -- (a) one record references many times ("fun last saturday… dinner next
  -- friday" = two reference_times, two directions); (b) reference_time is
  -- LLM-derived interpretation — writing it onto immutable raw records breaks
  -- the facts/interpretation split; (c) one index beats 15 columns + a UNION.
  -- er_extraction_log (source_table, source_id) is the once-per-record gate —
  -- it IS the "processed" flag; data_* tables carry no bookkeeping columns.

dirty_at        -- CACHE, not a queue. One nullable TIMESTAMPTZ on wiki_events/
  wiki_days/wiki_stories: stamped when new evidence lands on a settled object,
  cleared on refresh. Badges and "most pending evidence" are joins over
  marginalia/mentions since the stamp. (wiki_dirty the table: deleted.)

-- wiki_revisions: REMOVED from v1. Its real job was slow-drift detection for
-- AI-discovered motifs — deferred with the discovery layer, returns with it.
-- Prose history, if ever wanted, is a marginalia entry.
```

**V1 restriction on stories:** hand-made only (`origin = user`, always), replacing Notebooks one-for-one, with one behavioral switch: **`auto_add_materials` (bool) — magnet or folder.** On = the hourly attach step ANNs new events/artifacts against this story's centroid and auto-collects (the thing NotebookLM can't do). Off = a plain folder, purely pushed members. This single boolean is the entire ambient tier compressed into an opt-in, per-story choice — the maintainability principle embodied. `valence`/`direction`/`state`/`completable` ship in the schema but stay dormant until the axiology layer lands. **No AI-created stories in v1; no discovery pass; no `kind` enum ever** (species — episode/pattern/pursuit — is derivable from support-shape × intent when it's ever needed).

**No prospective/future-events table.** A future-referring record is a record whose `reference_time > now` — **the future is a query, not a table.** Extraction stamps `reference_time` (+ granularity: exact|day|week|month) on mentions; when a window goes hot, the resolution pass pulls records referencing it as context ("phantom data" costs one WHERE clause); backfill is the same scan pointed backward. Timeless intent ("someday, Japan") = `reference_time NULL` → lives as a hand-made story or stays dust. Fulfilled/missed reconciliation state is deferred with the examen; until then a margin note on the day suffices.

**`wiki_things` deprecated.** Topics are universals (categories); things were particulars ("Biscuit," "the boat") — don't merge them into tags, and don't run open-ended thing-ER (every noun becomes a row — the cosmic-meaning trap in entity costume). Projects/hobbies → stories; concepts → topics; the rare mattering particular accumulates as floating mentions (the safety net — retained, searchable) until promotion pressure or a prudence question mints it. V1: stop writing to the table, exclude from ER; drop in a later cleanup migration.

Membership = **centroid + overrides**: auto-attracted members (ANN proximity) + `user_included` / `user_excluded` (permanent). Self-maintaining by default, correctable when wrong. Temporal support (the set of intervals) is *derived* from members' timestamps, not stored.

- **Virtue** = valence positive, direction cultivate, non-completable. **Vice** = negative, remove. Past/present/future is already the `state` field. "My virtues" page = a *view* (`direction = cultivate`), not a table.
- **Todos are not stories** — they are items *inside* stories (completion predicate = the axis: todo has one, motif doesn't). Praxis lives inside narrative; the discrete act in service of the arc, the arc in service of the telos.
- **Entities are not stories** — Sarah is an entity; "my relationship with Sarah" is a story. Atoms vs. clusters.
- **Declared vs. discovered, ought vs. is:** a *declared* story (origin=user, e.g. "learn sax") starts with an empty temporal support and a centroid seeded from the declaration text — it **seeks evidence**. A *discovered* story clusters first and **seeks a name**. Same object; the gap between declaration and evidence **is the examen, for free**. State transitions are *emergent from evidence accrual*: no evidence → aspiring; accruing → active; stopped → dormant; closed → completed. A declared story whose support stays empty is the ought/did delta at story scale.

### 4.2 Ambient tier

Discovered stories are created freely but **surfaced almost never**. Hundreds of low-significance ambient motifs exist invisibly as the AI's contextual index (they measurably improve scoped retrieval, ER blocking, and chat context). Only a rare high-significance discovery is *proposed* ("you've had a scuba thing for 8 years — want to make it a story?"). Proposal → adoption → workspace is the flywheel. Never flood the story list.

---

## 5. Significance

### 5.1 Three quantities, kept distinct

```
S  significance   how much this MATTERS       (evidence-anchored)
U  surprise       how UNEXPECTED it is        (novelty_z / LOF — exists)
Λ  salience       what earns spend/attention  Λ = S · (0.5 + U) · exp(−λ_kind·Δt)
```

The wedding: high S, low U → fully recorded and storied, but not flagged as "surprising." Spam: some U, zero S → dust. Keeping S and U separate is what makes both cases correct. **Surprise enters with a floor** (the `0.5 +`) — it amplifies but cannot gatekeep; pure multiplication would zero the expected-but-momentous (the wedding). **Spend and surfacing key off Λ; S is an input.**

### 5.2 What gets scored: signals vs. artifacts

Significance is never applied to every raw row. Raw rows have two natures:

- **Signals** — continuous samples (GPS points, heart-rate samples, app-usage ticks). **Never individually scored.** They are the integrand: they *compose into* events/visits and *supply witness inputs* to whatever they compose into (the HR stream supplies arousal; durations supply cost-time).
- **Artifacts** — discrete, self-contained items (a message, an email, a transcription chunk, a purchase, a visit, a journal entry). These are **candidates** — things that could earn extraction, structure, or attention — and candidates are what get scored.

Significance is a **common interface over heterogeneous ontologies**, not a formula needing every field. Each ontology maps whatever columns it has onto whichever witnesses it can testify to; the rest stay silent:

| Ontology | agency | cost | arousal | affinity |
|---|---|---|---|---|
| message | you sent it | — | HR during, if worn | text ≈ claimed centroids |
| purchase | chosen vs. auto-renew | amount | — | merchant/category |
| location visit | deliberate trip vs. routine | duration | HR during visit | place ≈ claimed places |
| transcription chunk | you spoke vs. listened | duration | HR during | content proximity |
| calendar event | you organized it | duration | — | attendees/title |

**Measurement doctrine:** significance is measured on **evidence only** — artifacts and events (records testify; events aggregate records via **peak-end**: `0.5·max + 0.5·recency-weighted mean`). Stories are never measured directly — **a story inherits the significance of its attached evidence** ("what does arousal mean for the patience story?" = the aggregated somatic weight of the moments attached to it). And **claimed/pinned stories bypass the machinery entirely** — affinity = 1 by declaration; testimony is for the unclaimed 99%. Two regimes, one system: *claimed = significance by fiat; unclaimed = significance by testimony.* The researcher pinning a notebook-style story never encounters the witness machinery at all.

### 5.3 The six witnesses — quorum, not weights

| Witness | Question | Register | Source |
|---|---|---|---|
| **affinity** | is it near what you *declared*? | axiological (word) | embedding proximity to telos/story tree — **user-sourced, never inferred** |
| **cost** | what did you *spend*? | economic (deed) | time (`app_usage`, durations) + money (`financial_transaction`) — **and nothing else**: energy expenditure is a property of the activity, not of mattering |
| **agency** | did you *initiate* it, or did it arrive? | volitional | deterministic per record: sent vs. received, organized vs. invited, chosen purchase vs. auto-renewal, wrote vs. consumed, searched vs. fed. Per-channel baselines (obligatory work email ≠ volition) |
| **arousal** | did your *body* react? | somatic | `hr_z`/`hrv_z`/`autonomic_z`, z-scored against the **K most similar events** (event-class conditional baseline — the existing conditional-W5H novelty doctrine; dissolves the exercise confound: an easy gym day with high arousal pops, a hard one doesn't). Its job: **the cheapest honest mattering-filter needing no user input and no LLM** — the body votes before any model runs |
| **persistence** | does it *return* over time? | temporal | recurrence, log-damped |
| **centrality** | is it *woven in*? | structural | **graph edges, not embedding proximity** — count of records/events/co-occurrences linking to the entity in `wiki_entity_refs`. Deliberately **telos-blind** |

**Scoring is a quorum, not a weighted sum.** Each witness is z-scored against its own personal baseline and declared **loud** above its threshold. Then:

```
0 loud → dust
1 loud → hold as ambient evidence (attach, don't structure)
2 loud → structure (promote into the narrative layer)
3+ loud → candidate for surfacing (enter the prudence gate)
```

No arbitrary w₁–w₆ to defend. Ordinal and interpretable ("three witnesses testified: your body reacted, you initiated it, it keeps returning"). Robust to the real correlation structure (cost↔persistence high — agency splits them: commute vs. meditation; affinity↔centrality moderate — the *divergences* are the product; correlated witnesses firing together just reach quorum easier, which is correct behavior). Missing witnesses are **silent, not averaged** — no renormalization; absence of testimony is not testimony. Weighted sums may return *if* the labeled week shows quorum is too crude; quorum is the version containing no number anyone must defend.

Each witness survives the removal test: drop affinity → cosmic meaning from shopping lists; drop cost → no revealed preference → no examen; drop agency → meditation and traffic look identical (and moral appraisal becomes impossible — Aristotle Book III); drop arousal → lose the pre-LLM filter and the somatic layer; drop persistence → one busy Saturday becomes The Scuba Story; drop centrality → **telos tyranny: your mother disappears** because she isn't on your stated career path. Centrality must stay telos-blind precisely so it can testify *against* the telos when the telos is wrong about what matters. The witness symmetry is post-hoc poetry — hold it loosely; admit a seventh only if something breaks without it (candidate: **narration** — journaled-about-it as testimony; may fold into agency).

**Witness disagreements are the product:** affinity vs. cost = declared vs. revealed (the examen); affinity vs. arousal = stated vs. felt ("you say you love this job; your body disagrees"); persistence vs. agency = habit vs. imposition.

### 5.3 The two telos

- **Declared telos** — who you say you're becoming (the document).
- **Revealed telos** — what you actually spend life on (computed: cost + agency + persistence aggregated over the story tree).

The delta between them is the deepest thing the app can say — the examen at the scale of a life rather than a day. It is also *human*: being and becoming, and people understand that without being taught. The rates of change stack correctly: telos changes slowest, virtues faster, stories faster, events constantly.

### 5.4 Storage rule (consequence of the feedback loop)

Creating/pinning a story raises affinity for everything near its centroid → past dust retroactively becomes significant. Good loop, but it means **significance is a function of the current story graph**:

> Store the **immutable witnesses** (arousal, agency, cost — facts about the moment). Derive the **graph-dependent witnesses** (affinity, centrality) at compute time.

Baking affinity in at ingest = silent cascade drift, the exact failure this architecture exists to prevent. Same law as everything else: facts append-only, interpretation derived.

### 5.5 Scopes

Artifact (momentary — the quorum runs here), event (peak-end aggregate of member artifacts + own physiology), entity (stable — aggregate over its participations, plus persistence + centrality, which only exist at aggregate scope), story (inherits from members; bypassed if claimed). One interface, different witness availability per scope (§5.2 table).

---

## 6. Authority model: who may change what

`origin` is provenance (permanent, informational). **`last_edited_by` is authority.** The moment a human pins, renames, or edits, authority transfers.

**Field updates — always safe, via parallel columns.** The AI writes `auto_*`; the human writes `user_*`; display prefers `user_*`. The AI keeps re-deriving forever; human intent is *structurally impossible* to clobber. (Centroid-plus-overrides is this same pattern applied to membership.) No locks — a lock would freeze improvement.

**Structural operations — permission-gated:**

| Story | AI may |
|---|---|
| never human-touched | split / merge / kill / re-parent **freely** (its own inference) |
| human-touched | **propose only** |
| telos | **propose only, ever** |

Two accepted consequences, stated as product values: **(1) Autonomy over tidiness** — the AI never silently reorganizes what a human declared, even to fix an obvious duplicate; for an app about a person's own axiology, silent restructuring of stated values is worse than untidiness. **(2) Authority doesn't cascade** — pinning "become patient" does not freeze the AI-discovered sub-stories beneath it; you own the node you touched, the AI owns what it inferred underneath. Otherwise one pin ossifies a subtree.

**The maintenance-less invariant:** HITL burden is bounded by how much the user chose to claim. Claim nothing, maintain nothing.

---

## 7. Dynamics and cadences

### 7.1 The pipeline

```
Data (sources/streams, some with deterministic ER attached)
   │
   ▼
HOURLY CRON  (hot, cheap)
   ├─ Time-ER: event boundary this hour?  → wiki_events        [heuristic + gated LLM]
   ├─ Space-ER: extract mentions (+reference_time), BLOCK only  [local LLM + ANN; no adjudication]
   ├─ Gate: Λ below floor → DUST (embedded, searchable, unstructured — never deleted)
   ├─ Route: reference_time ≠ now → deliver to target window; mark DIRTY
   └─ ATTACH: event embedding →ANN→ story centroids → attach → mark story dirty
                                                    [one vector search; free; deterministic]
   ▼
BATCHED ADJUDICATION  (slow lane, Lite slot, batch/off-peak pricing)
   └─ only gate-promoted ambiguities: identity links, story ops, deltas
   ▼
SLOW PASSES
   ├─ Re-summarize (daily/weekly): dirty stories crossing a material-change threshold
   │    (≥N new attachments), top-K by significance per run — hard budget cap.
   │    Soft-update the abstract; append to the CRM-style timeline.
   └─ Discover (weekly): cluster unattached significant material;
        require density + TIME-SPREAD (persistence — one Saturday ≠ a story) + Λ.
        Created freely, surfaced rarely. Build this pass LAST.
   ▼
SURFACE  (memos, deltas, reminders — weighted by Λ, decaying)
   ▼
FEEDBACK (confirm/dismiss → graph writes + significance-weight learning)
```

The hourly cron is the pragmatic stand-in for realtime changepoint detection; the machinery is identical — realtime CPD would only mark windows dirty *sooner*. Non-breaking upgrade path.

**Cost discipline:** extraction fires **once per item** (gate on already-processed `source_id`) — the rolling window must not re-bill the same record 120×. A local on-device small model makes L1 grunt work (extraction, gating, blocking) effectively free and private; adjudication/appraisal uses the `Lite` slot on slow/batch pricing. **Free compute does not retire the gate** — when inference is free, the scarce resources become user attention and graph cleanliness.

### 7.2 Story lifecycle operations (geometric detection, LLM adjudication)

emerge / merge / split / drift / die / re-parent — same op-set as entity ER, one level up, reusing the same machinery (blocking, prudence gate, memos, precision-over-recall). Differences: stories have **no deterministic anchors** (harder — purely semantic) but wrong merges are merely annoying, not corrupting (safer — can be more aggressive than entity ER).

**Retroactive sweep:** on birth, a story ANNs its centroid against **all historical material** — a discovered 8-year scuba arc must claim its own past, not start today.

### 7.3 The two review surfaces

- **Proactive → chat, prudence-gated:** `interrupt = Λ · ambiguity · actionability`. Ambiguity = 1 − (top1 − top2) candidate margin — the same rerank-gap signal already computed in search (`VIRTUES_RERANK_GAP`). Actionability = would an answer change a link/reminder/attribution. All three high or silence. One question at a time, conversational.
- **Passive → a wiki page for gardeners:** open threads, low-confidence links, unreconciled deltas, browsable. **Opt-in, never an inbox.** Chat = AI-initiated (rare); page = user-initiated (anytime).

**The contract:** *"You don't maintain this. Occasionally I'll ask one good question. If you enjoy curating, there's a page — but you never have to open it."*

### 7.4 Identity vs. participation (both, always)

Identity = the node ("who is Sarah"). Participation = the edge (`wiki_entity_refs` with `role` — "Sarah was at this"). Orthogonal; each exists without the other (participation-with-unknown-identity: "you talked to *someone* at 3pm"; identity-without-participation: "Sarah is my sister"). Schema already splits them correctly. Mentions with no context resolve **deferred**: mention → floating (searchable) → evidence accrues (co-occurrence, calendar, location) → link, or `IDENTIFY` question, or permanent dust. Promotion threshold guards against singleton phantom entities: recur N times, or corroborate cross-source, or human confirms.

---

## 8. The V1 plan

The census — six objects, every one with a defined job and a defined UX; the deferred layers (motifs, axiology, examen, discovery) arrive later as readers/writers of these same rows:

```
wiki_people / wiki_places / wiki_orgs         entities (wiki_things: deprecated)
wiki_events  (+ topics, class-by-neighborhood) segmented narrative, 8–16/day
wiki_days                                      lattice + nightly summary
wiki_stories                                   hand-made, magnet-or-folder (auto_add_materials)
wiki_marginalia                                the margins of everything
mentions (+ reference_time)                    the evidence layer — floating, delivered, promoted
```

### Workstreams

**W1 — Foundations (one migration + embedder invariant).**
`wiki_marginalia`; `mentions` (surface, type, normalized, role, `reference_time` + granularity, embedding, source record ref, once-per-item processed gate); Notebooks→Stories rename (`0032_notebooks_rename.sql` precedent) with `origin`, `auto_add_materials`, `centroid`, `pinned`, `last_edited_by`, `auto_*`/`user_*` splits (+ dormant `valence`/`direction`/`state`/`completable`); `wiki_revisions`; dirty flags; deprecate `wiki_things` (stop writes, exclude from ER). Embedder invariant: standardize the CPU/manual path on gte-small-384 to match the NPU path; switching embedders requires a guarded full re-embed — never a silent mix of vector spaces.

**W2 — Entity resolution (people / places / orgs; no things).**
Places stay deterministic (in X spot for Y time — geometry, exists). People-structured stays deterministic (email/calendar keys — exists). Orgs: merchant/domain matching (exists, harden). New: entity embeddings + aliases into the vector index (unblocks blocking); the extraction action (Lite/local slot, structured output, once-per-item gate) emitting mentions; ANN+BM25 blocking; **batched adjudication** (Lite, slow lane) for the hard semantic "Sarah"s; the review page + the prudence-gated question (`Λ · ambiguity · actionability`, ≤1/day).

**W3 — Event resolution (the hourly cron).**
Boundary detection normalized to 8–16 events/day (the target count is itself the segmentation weight); event embedding; **class-by-neighborhood** — an event's class is its K nearest past events, no enum; one kNN serves three consumers (classification, arousal conditional baseline, novelty conditional baseline); topic tags (one cheap emission); witnesses per the §5.2 ontology mapping + quorum gating extraction spend.

**W4 — Day layer (the nightly cron).**
Nightly day summary; receipt/settlement (books close on view — §3.6); margins accrue + badges (count of quorum-clearing notes); **`reference_time` delivery**: when a window goes hot, pull records referencing it (forward = phantom/planned context, backward = backfill → margin note + badge). No prospective table.

**W5 — Stories (small, after W3).**
Rename ships in W1; behavior ships here: the **attach step** in the hourly cron — event/artifact embedding → ANN against centroids of stories with `auto_add_materials = on` → attach → mark dirty. One vector search, one insert, one flag. Abstract soft-updates for dirty stories under the period-close rule.

**W6 — Tune (the labeled week).**
Hand-label one real week (ground truth for entity links, boundaries, significance). Tune: per-witness loudness thresholds, quorum levels, decay λ, absolute floors, settle timing. Stand up the §9 eval harness. **The build is the live test** — W2/W3 running on real data *produces* the labeled week almost for free.

Sequencing: W1 → W2 ∥ W3 → W4 → W5 → W6, with W6 overlapping everything once data flows.

### Deferred (designed above, not built now)
Discovery pass / ambient motifs (also *quality*-gated: banal "ai"/"research" clusters until significance is tuned — ship it early and it poisons the feature); the axiology tree (virtues/vices/valence, telos onboarding); the examen + revealed telos + fulfilled/missed reconciliation; folding acts/chapters into stories (last — most entangled with the day page).

## 9. Evals: what "personal intelligence" means

Not an open question — a commitment. Component tests: entity-linking F1 (label a week, not a year); event-segmentation agreement vs. the user's own day-splits; temporal-attribution accuracy (did the journal-about-Tuesday land on Tuesday?); insight precision@k weighted by user rating; triviality false-positive rate ("why did you tell me this" — the direct measure of the significance gate); cold-recall QA weeks later ("who was at the party?"); plan/actual reconciliation accuracy; reminder actionability (accepted/acted-on vs. dismissed).

**The integration test is narrative reconstruction:** hold out the journal, reconstruct the week from raw exhaust alone, compare to the user's own account. Right people, right events, right significance, no fabricated meaning — that is personal intelligence; everything else is a component test.

## 10. Outstanding questions

1. **Cold start** — *accepted, not designed around.* Every system has it; no great system fixes it, and warping the architecture for week one would be a mistake. Standing mitigations that need no extra design: the telos document seeds affinity at onboarding; arousal + persistence + agency need no user input; interaction frequency bootstraps entity affinity (counting behavior, not inferring meaning); weak global z-baseline converges to personal over weeks. Par for the course.
2. **Loudness thresholds + quorum levels.** What z counts as "loud" per witness; do quorum levels (1/2/3+) survive the labeled week, or does a weighted refinement return? Learn from accept/dismiss later — with exploration probes, since surfaced-only feedback teaches precision but never recall.
3. **Absolute floors.** Z-scores are relative; a boring week must not manufacture a false peak. Where do the floors sit, per scope?
3b. **Settlement + badges (product).** When do the books close — on read, after N days, explicitly? Where do refresh badges live so they never congeal into the ops-inbox we rejected? Are margins user-visible by default or AI-internal with an inspector? Does a *rejected* refresh clear the badge forever or re-arm on new notes?
3c. **Redaction escape hatch.** "Never delete" needs one deliberate exception: user-initiated redaction (privacy, painful history) — content scrubbed, provenance skeleton kept (the Zep/Graphiti redact pattern). Scope it before launch, not after the first request.
4. **Record granularity.** The atomic unit ER runs on — a message? a transcript sentence? Chunking policy for long transcripts (many mentions, many reference-times) is undecided and shapes everything upstream.
5. **Multi-source event fusion.** Co-temporal fusion is easy (timestamps + geo). Semantic same-ness without co-location (a call + an email thread + a meeting about one project) is unscoped.
6. **Partition vs. sparse overlay.** The flush-24h event contract vs. "stories are sparse, the lattice carries completeness." Leaning: completeness from the lattice/Dayline; relax `wiki_events` from partition to overlay (retire `is_unknown`). Not blocking; decide before folding acts/chapters.
7. ~~Notebooks direction~~ **Resolved:** one `wiki_stories`, hand-made, `auto_add_materials` = magnet-or-folder; the IR/sources workflow carries over one-for-one.
8. **φ(t) surface.** Chaos/order + context-entropy is computable — is it a Dayline overlay, an internal signal only, or both? (Never a score.)
9. **Story-scoped retrieval mechanics.** Centroid-filtered ANN vs. membership-filtered joins vs. both; interaction with the existing hybrid (BM25 + dense + conditional rerank) stack.
10. **Ambient story volume.** Hundreds of invisible motifs: storage/index cost is trivial, but does ANN-against-centroids degrade as centroid count grows? Cap, prune by decay, or hierarchical centroids?
11. **On-device model selection.** Which small local model for L1 (extraction/gating), and does it run within the box's compute envelope alongside embed/rerank sidecars?
12. **Eval labeling economics.** The eval suite (§9) needs ground truth; hand-labeling a user's week is the cheapest honest unit. Who labels, how often, and does the HITL feedback stream double as eval data?

---

## Appendix: vocabulary

| Term | Meaning |
|---|---|
| **dust** | un-promoted raw material: embedded, searchable, unstructured, never deleted |
| **contextulae** | stateless W4H snapshots (raw ontology rows); *when* is a coordinate, not content |
| **lattice** | deterministic time scaffolding (hour/day/month/year/life); indexed against, never resolved |
| **story** | context over time; a named, persisted cluster (centroid + overrides) with narrative state |
| **memo** | the human-readable running prose about anything unresolved (a story's abstract, an open question) — the surface the user reads instead of an ops queue |
| **witness** | one independent register of mattering (affinity, cost, agency, arousal, persistence, centrality) |
| **Λ (salience)** | S · U · decay — what earns compute, structure, and attention |
| **prudence gate** | Λ · ambiguity · actionability — whether to ask the human anything |
| **examen** | the computed delta between declared (ought) and revealed (did), at day/story/life scale |
| **dirty window** | a time span whose derived projections must be recomputed after new evidence |
| **planned/actual** | the two layers of an event; reconciliation annotates, never overwrites |
| **signal / artifact** | continuous samples (never scored; compose into events) vs. discrete candidates (scored via the witness quorum) |
| **quorum / loud** | a witness is *loud* above its personal z-threshold; the count of loud witnesses is the score |
| **margin** | an object's append-only note stream (`wiki_marginalia`) — always writable, read by retrieval and by every re-synthesis |
| **hot / settled** | before/after the books close; settled prose regenerates only by human acceptance (§3.6) |
| **fluent** | a state-of-self with a validity interval; present-tense assertions terminate intervals, only past-anchored ones correct records |
