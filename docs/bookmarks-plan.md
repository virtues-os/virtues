# Bookmarks — capture, enrichment, and retrieval plan

Status: DESIGNED 2026-07-28 (conversation-complete, unbuilt)
Owner table: `data_content_bookmark` (migration 0007) — schema already fits; no
migration needed for v1 except the user-authored `why` column (see below).

## Why this exists

`data_content_bookmark` has a complete ontology descriptor
(`crates/virtues-registry/src/ontologies.rs` — `content_bookmark`: embedding
config, day-source config, citation chip mapping) and **zero producers**.
Meanwhile "save the 20 interesting things I hit per day, before I forget, and
find them again later" is a top feature request, especially from designers.

The `source_streams: ["stream_github_events"]` on the descriptor is dead
metadata — the stream layer was never built. Real pipeline shape is:
source applet → (lake archive of raw payload) → `data_content_bookmark`.

## The paradigm (one paragraph)

**Everything becomes text; the Omni slot is the eye; gte stays the only
index.** Every saved item is normalized to a row immediately (free, instant,
searchable by title/BM25). A budgeted background sweep then enriches it: a
vision/audio pass writes a structured *extraction record* (W5H-shaped, with
`likely_queries`), which is embedded per-aspect into the existing text stack.
Long videos are never ingested — link + free transcript only, full visual pass
strictly on demand. The user's *why* is never inferred — it is captured
cheaply (share-sheet note), harvested (source containers), or elicited at
review time; machine text and user text stay segregated.

## Decisions and their reasons

### Retrieval: captions-into-text-stack, NOT a multimodal embedding space

- Evidence (researched 2026-07-28): best head-to-head (slide retrieval,
  arXiv 2509.15211) puts caption-hybrid within ~3 nDCG of full visual
  retrieval; Pinterest routes VLM captions into production search embeddings
  (OmniSearchSage); no vector-DB vendor recommends one shared multimodal
  space for text+images (it degrades text geometry — jina-v5-omni exists to
  patch exactly that). CLIP-family compositionality is still unsolved at
  index time.
- virtues-specific: BM25 + ColBERT rerank are text-side regardless, so a
  VLM text representation must exist anyway; the embedder (gte-small 384-d
  on the Dragon NPU / EmbeddingGemma sidecar) is on-box, and routing the
  whole life corpus through a cloud embedder is a different product.
  A second engine is the magnet disease (see docs/ir-notes.md).
- **jina-v5-omni evaluated and rejected** for v1 (multi-B params, cloud/GPU
  only, forks the index, marginal measured win).
- v2 hedge: a **similarity-only** CLIP-class sidecar for "more like this"
  gallery browsing — never fused into search. Candidate: MobileCLIP2-S4
  (SigLIP-So400m quality, ~10ms on mobile NPU — but published numbers are
  Apple ANE; no Hexagon/QNN recipe exists, budget an OCR-spike-shaped
  effort, or start from Qualcomm AI Hub's precompiled CLIP). If
  appearance-queries underperform, gating this index into z-fusion for
  image-typed results is the cheap upgrade — pairs with the IR keystone
  refactor (`recall_and_fuse` / `rerank_and_finalize`).

### The extraction record (the "what")

One structured pass per visual item, every field nullable, low temperature,
free-prose `description` FIRST in the schema (model looks before the
constrained fields). Fields:

- `description` — free prose
- `medium` — photo | screenshot | ui_recording | reel | product | artwork | diagram | …
- `subject`, `entities` — what/who: objects, brands, people, products, places
- `setting` — where
- `style` — design vocabulary ("brutalist", "warm minimal")
- `palette` — colors BOTH ways: prose names for BM25/embedding ("brown
  shutters, green door, cream stucco") AND dominant colors as hex swatches
  (deterministically extractable from the image, no VLM needed) — the hex
  side powers search-by-color (see Surfaces)
- `visible_text` — verbatim OCR; "unreadable" is a legal value
- `likely_queries` — 3–6 strings of *what the user would type to find this*
  (Pinterest GEO finding: query-shaped captions human-rated 4.15 vs 2.21
  for literal captions — the single highest-leverage practice)
- `observed` vs `inferred` attributes kept distinguishable

**`why` is deliberately absent from the machine schema.** Significance is
user-sourced, never inferred (salience doctrine) — enforced by schema shape.

Embedding: each aspect embeds as its own row (multi-aspect beats one blob —
DreamLIP/Long-CLIP; also fits gte-small's short context), sharing the
bookmark record id. Multiple vectors collapsing to one result is the same
shape as magnet-collapse/multi-query in the IR keystone refactor.

Hallucination fencing: nullable-with-`unknown` everywhere, verbatim-or-
unreadable for text, originals archived in the lake so the whole extraction
layer is derived data — re-runnable wholesale when models improve.

### Model slots (no literals — slot semantics)

- **Omni slot** (`google/gemini-3-flash` today): everything with pixels or
  audio. Its registry doc comment already describes this job ("verbatim
  transcript PLUS scene/mood/music/entities"). ~$0.008/image; a heavy
  50-visual-save day ≈ $0.40 worst case. Enrichment uses no tools, so the
  Gemini-3 parallel-tool-call exclusion doesn't apply.
- **Lite slot** (`zai/glm-4.7-flash` today): everything text→text —
  summaries/chapters/likely_queries from transcripts and page text, tag
  suggestion, review-queue candidate-whys. (GLM flash line is text-only per
  public info; the gateway catalog is the authority. Even if a vision-capable
  cheaper model appears, that's a one-line Omni swap.)
- Video is effectively Gemini-only anyway (as of 2026-07: OpenAI has no API
  video input, Anthropic none; Gemini takes public YouTube URLs server-side
  and processes the audio track natively).

### Video/audio tiers (split by DURATION, not "video-ness")

| Tier | What | Cost | Rationale |
|---|---|---|---|
| Short-form ≤ ~3 min (reels, TikTok, screen recordings) | Full Omni pass, audio+visual; bump FPS above default 1 for UI recordings | 1–4¢ | A reel's content is visual+music; transcript alone indexes the wrong thing |
| Long-form (YouTube, podcasts) | NEVER ingest the file. Link + metadata + thumbnail caption; transcript free (YouTube caption track — residential box IP works; `<podcast:transcript>` RSS tag); Lite composes summary/chapters | ~free–5¢ | Long-form saves are "content & ideas" saves; text already exists |
| Escalation on demand | Full visual pass only when something asks: user question in chat, notebook admission, screenshot-marginalia | $0.10–0.70 / 20 min | Never by default |

- Reels/TikTok exceptions: no transcript exists (the Omni pass IS their
  index), fetch requires yt-dlp on the box (ToS-grey; residential IP is the
  best-positioned fetcher; what save-for-later products do). Optionally keep
  the ~5MB MP4 in the lake — most link-rot-prone content on the internet.
- No-transcript long video: cheap ASR fallback (Parakeet TDT v3 / Voxtral
  class) or metadata-only. Never silently escalate.
- **Screenshot-as-marginalia is a first-class primitive**: user screenshots
  a frame → it's a whisper (user-picked = significance-sourced), the visual
  index for the long video (one image enrichment instead of 20 minutes of
  video), and a timestamp deep link back (`?t=SS`, the AssetViewer `?page=N`
  pattern).

### The "why" (three mechanisms, layered by friction)

1. **Capture-time whisper** — optional one-line note (+ keyboard dictation)
   in owned doors (share sheet, in-app save, future extension). Save fires
   instantly on one tap; the note never blocks. For Instagram the share
   sheet is the ONLY capture path, so IG whispers live here by necessity.
2. **Containers are the native whisper for synced sources** — GitHub star
   lists, browser bookmark folder paths, X bookmark folders (Premium),
   Are.na channels, Raindrop collections. Harvest container membership as
   user-authored tags/why at ingest. Almost no source has a note field;
   almost every source has a container.
3. **Review-time elicitation, context-routed** — capture context (browsing
   trail, active app/notebook, time of day) is stamped deterministically at
   save. Its job is ROUTING, not answering: rich work-context → auto-
   attribute, don't ask; idle-scroll saves → the Inbox queue, where the
   model proposes 2–3 candidate whys from content+context as tappable
   suggestions. A tap/edit becomes the user-authored why; untaken
   suggestions are discarded, never stored (covenant: machine proposes,
   user disposes). Prefer cluster-level asks ("these 6 look related —
   what's the thread?"); a notebook assignment is the highest-fidelity why.

The why is no longer load-bearing for retrieval (the extraction record
carries findability); it drives triage, notebooks, and synthesis. When
present it embeds as its own aspect row — intent language matches future
query language. Writing/editing a why re-embeds.

Schema change: add a user-authored `note TEXT` column (not metadata; never
machine-written). Named `note`, not `why`: "why" is the product concept and
elicitation prompt, but the column holds general marginalia — reasons,
todos, pointers ("the chart at 12:30") — and the retrieval boost attaches
to user-authored text as such. Extend descriptor `embed_text_sql`
accordingly.

### Backpressure (the bulk-import cliff)

Steady-state capture (20–50/day) costs cents and kilobytes. The hazard is
first-sync backfill (5k X bookmarks, 10k browser export → ~$50–100 surprise
+ GB of media). Guards:

- **Ingest ≠ enrich, decoupled.** Sync writes cheap normalized rows only
  (already searchable). Enrichment is a separate queued sweep
  (embedding_index / document_extraction pattern), never inline.
- **Budgeted queue** — drains newest-first at a daily cap (count or spend;
  the prepaid-wallet ledger makes "$0.50/day" a natural Settings knob).
- **Lazy tail** — archive items enrich on touch (opened, clicked in
  results, notebook admission). Explicit bulk button with a price tag
  ("enrich all 8,000 (~$60)?") = informed consent, never a side effect.
- **Media storage opt-in and bounded** — enrichment reads remote media but
  stores text by default; thumbnails capped; snapshots/MP4-insurance are
  per-source toggles with size limits. Vectors are a non-issue (10k
  bookmarks × 5 aspects ≈ 80MB).
- Idempotency: `source_stream_id` UNIQUE (e.g. `github:star:<node_id>`,
  `x:bookmark:<tweet_id>`); enrichment stamped with model/version so
  re-runs are explicit.

### Surfaces

Free once rows+embeddings exist: chat citations (amber chip mapped in
`citations/mapping.ts`), semantic search, day timeline + Daily Office
(which is also where the review ritual surfaces: "3 of yesterday's 7 saves
have no why yet").

To build:
- **`/bookmarks` first-class route** — UniversalDataGrid (doctrine) plus a
  **bento-grid display mode** (mymind-style masonry wall — designers need
  the moodboard view). DECIDED: built INTO UniversalDataGrid as a display
  mode, but gated on for bookmarks only for now — not offered to other
  grids until it earns it. Extraction-record fields as grid facets
  (medium/source/style); route-driven SubNav: `Inbox` (triage:
  keep/dismiss + why elicitation) | `Library`. Status line: "212 awaiting
  enrichment".
- **Search-by-color** (Cosmos precedent: hex color picker) — the palette
  hex swatches make this a plain grid facet, no ML: store dominant colors
  as hex per bookmark, filter by nearest-color to a picked hex. Cheap,
  designers love it, and it's deterministic.
- **Generic `/api/ontologies/*` routes** (available/overview/{table}/data)
  — apps/web already calls them and 404s for EVERY ontology
  (`client.ts:1942`); one registry-driven handler fixes all ontologies and
  gives /bookmarks its data endpoint.
- Settings (one flat room): enrichment daily budget, per-source media
  toggles.
- Later: bookmark → notebook admission as sources; entity linking.

**Naming**: user-facing "Bookmarks" = this feature. Sidebar route pins stay
"Pinned". `docs/ui-overhaul-plan.md` item 8 (renaming `app_pins` →
`app_bookmarks`) COLLIDES with this and should be dropped/renamed.

## Build order

### Phase 0 — the spine (prerequisite)

Naming convention: ingest applets are SOURCE-named ({device}_ingest for
webhook push, {provider}_{stream}_sync for cron pulls) and fan out into
whatever ontologies their payload supports; pipeline applets are
function-named (embedding_index, document_extraction). So there is no
"bookmark_ingest" — bookmarks are a stream within source applets, plus one
pipeline applet:

1. Shared normalizer (helper crate fn): raw save → `data_content_bookmark`
   row + lake archive. Consumed by every door below.
2. **In-app save**: plain API endpoint (`POST /api/bookmarks`) → shared
   normalizer. No applet needed; loopback-authenticated.
3. `bookmark_enrichment` pipeline applet: queue, daily budget, lazy tail;
   Omni for pixels/audio, Lite for text composition; URL fetcher +
   readability on box (note: no URL-fetch capability exists today —
   `web_search` is Exa-search-only; this is a new native capability, and
   must be native Rust — the `virtues_applet_writer` role can't write
   `data_*`).
4. Aspect embedding rows: NOTE `EmbeddingConfig.embed_text_sql` is
   single-string-per-row today; multi-aspect means bookmark_enrichment
   writes aspect rows into the search index directly (or EmbeddingConfig
   grows multi-row support). Decide at build time; direct-write is the
   less invasive start.
5. `/bookmarks` view (grid + bento display mode + Inbox/Library) + the
   three generic ontology API routes.
6. `note` column migration (user-authored marginalia; the "why" concept
   writes here); descriptor `embed_text_sql` update; drop the dead
   `source_streams` vec or leave as-is (harmless).

### Sources, in order (each exercises a different door before OAuth
complexity; 1–4 are box-side only)
1. **In-app save** — URL box in the web app → `POST /api/bookmarks`. No
   auth story, no applet. The dev harness for the spine; proves
   save→enrich→embed→surface day one.
2. **iOS share sheet** — the highest-value door; only capture path for IG
   reels. A new `bookmark` stream arm in the existing **`ios_ingest`**
   fan-out (like healthkit/location) — no new applet. New work = Tauri
   share extension + optional note field (+ dictation free via keyboard).
3. **Mac browser bookmarks** — new streams through the existing
   **`mac_ingest`**: Safari `Bookmarks.plist` + Reading List + Chrome/Arc
   JSON (+ Firefox places.sqlite); folder paths → container whys. Also
   serves as the backfill-caps test case (thousands of rows) and the
   retrieval-tuning corpus.
4. **`github_stars_sync`** — `api_key` (PAT) applet, no proxy work (GitHub
   OAuth app is dead); `github:star:<node_id>` ids. NOTE (found at build
   time): star *lists* have no public API, so the container signal is repo
   topics + language → tags. Unstars are a known gap (needs a full
   re-walk); initial backfill covers the newest 10k stars (page cap,
   stated in the run summary when hit).
5. **`x_bookmarks_sync`** — LAST: only source needing virtues-api proxy work
   (`/x/start` provider) + a billing spike. X API as of 2026: free tier
   closed (Feb), Basic legacy-only, pay-per-use default;
   `GET /2/users/{id}/bookmarks` = "owned read" at $0.001/resource
   (~$1/1000) since April. Spike must verify proxied user-context reads
   bill as owned reads. **Known API limit: the bookmarks endpoint only
   reaches the most recent ~800–1,000 bookmarks** — the whole X-bookmark
   tool market splits on this (API tools = capped; extension-scrape tools
   = full history). So API sync covers ongoing capture + recent backfill;
   FULL-history import needs the extension/scrape path or a one-off
   export. Incremental sync: page newest-first, stop at known ids.
   Fallback if economics/classification sour: extension capture from the
   user's own logged-in session (Dewey/Tweetsmash model, ToS-grey).

Deferred: browser extension (blocked on the iroh-gate story — webhook auth
is proven-device-key only; relay via Mac collector native messaging is the
likely path); file importers (Netscape HTML / Pocket CSV / Omnivore JSON —
DECIDED 2026-07-28 to skip for now: most incoming users have no existing
bookmark system, and one-off import sources aren't the habit loop; revisit
by demand); Raindrop/Readwise/Reddit/Are.na/YouTube-playlists (add by
demand).

## Market notes (surveyed 2026-07-28)

Landscape: Pocket died 2025-07 (CSV-only export, no article content — users
with dead links lost everything); Omnivore died 2024-11. Survivors are
subscription-funded, self-hosted, or AI-pivoting. Leaders: mymind
(anti-folder visual AI, $8–13/mo), Raindrop (organizer + "Stella" AI
librarian — self-hosted GPT-OSS-120B, privacy-framed, MCP server, YouTube
transcription), Readwise Reader (widest capture funnel + spaced-repetition
Daily Review, ~$120/yr), Karakeep (self-hosted flagship, BYO-model/Ollama
tagging, yt-dlp video archiving).

What this validates in our plan:
- **Nobody does visual/video understanding.** Transcript-of-YouTube is the
  market ceiling (Raindrop/Recall/Glasp); only Karakeep archives files.
  The Omni extraction record exceeds shipped practice.
- **Privacy/local-AI is a live selling point**, not a niche (Raindrop
  self-hosts its LLM as a headline; Karakeep's Ollama support is its
  applause line). The appliance is on-trend.
- Auto-tag + summarize + semantic search + chat-with-citations = table
  stakes; we get all four from existing infra.
- mymind is dinged for AI-metadata lock-in on export; our extraction
  records live in the user's own Postgres — inherent win, worth saying in
  marketing.

What the market adds to our plan (triaged 2026-07-28):
- **ADOPTED — bento-grid view** (mymind's wall) as a UniversalDataGrid
  display mode, bookmarks-gated for now. See Surfaces.
- **ADOPTED — search-by-color** (Cosmos hex picker) via palette hex
  swatches. See Surfaces.
- **LATER — resurfacing** (mymind Serendipity, Readwise Daily Review):
  Daily Office review ritual is the natural home; "one from the archive"
  slot is a cheap future add.
- **REJECTED — email-in capture door** (no one emails assets).
- **SKIPPED — importers for dead apps** (Pocket CSV/Omnivore JSON): one-off
  sources, most incoming users have no bookmark system; revisit by demand.
- **MCP/assistant access is becoming table stakes** ("your AI must read
  your saves") — natively true here; chat IS the front door.

## Open questions / spikes

1. X billing classification (owned reads via proxy?) — blocks source #6 only.
2. Browser-extension → box auth path (iroh gate).
3. MobileCLIP2-S4 on Hexagon/QNN feasibility (v2 sidecar only).
4. Reel MP4 lake-insurance default: on or off?
