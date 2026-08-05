# Bookmarks — capture, enrichment, and retrieval plan

Status: **CAPTURE SPINE BUILT 2026-08-04** (`2f5a5c94`, `c814dcb8`);
enrichment, IR, and the remaining doors unbuilt — see [Build plan](#build-plan-2026-08-05).
Owner table: `data_content_bookmark` (migration 0007) + `note` (migration 0073).

**Current reality in one line: bookmarks are storable but unfindable.** Three
capture doors land rows; `embed_text_sql` is still `title || description`, so a
saved Instagram post embeds as an empty document and the user's own `note` is
invisible to search.

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

## What shipped (2026-08-04)

Naming convention that shaped it: ingest applets are SOURCE-named
({device}_ingest for webhook push, {provider}_{stream}_sync for cron pulls)
and fan out into whatever ontologies their payload supports; pipeline applets
are function-named (embedding_index, document_extraction). So there is no
"bookmark_ingest" — bookmarks are a stream within source applets, plus one
pipeline applet.

Landed in `2f5a5c94` (sources spine) and `c814dcb8` (the room):

- **Shared normalizer** (`crates/virtues-helpers/src/bookmarks.rs`) — row
  shape, identity, and the snapshot-vs-event deletion split in one place.
  Snapshot sources reconcile via `tombstone_absent` scoped to one browser on
  one device; `note` and `is_archived` are user-owned and deliberately outside
  the upsert's update set, so a re-sync can never clobber the user's words.
- **In-app save** — `POST`/`GET /api/bookmarks`; identity is the canonicalized
  URL, so a re-save upserts rather than duplicating.
- **Mac browser bookmarks** — Safari/Chrome/Arc snapshots via `mac_ingest`.
- **`github_stars_sync`** — PAT applet. Found at build time: star *lists* have
  no public API, so the container signal is repo topics + language → tags.
  Unstars need a full re-walk (known gap); backfill caps at the newest 10k.
- **`note` column** (0073) and the `/bookmarks` room — a plain server-paginated
  grid, deliberately without facets over columns nothing writes yet.

## Build plan (2026-08-05)

Ordered so each step is useful the day it lands, and the riskiest work
(the iOS extension, the X proxy) blocks nothing ahead of it.

### 1. Note and tags into the embed text — hours, do first

`embed_text_sql` for `content_bookmark` is still `title || description`. Extend
it to include `note` and `tags`. This is the highest ratio change in the
document: it makes the manual-save door retrieve on the user's own words, which
migration 0073's own comment says is where the retrieval boost belongs.

**Verified**: no reindex plumbing needed. The indexer selects on
`se.doc_hash IS DISTINCT FROM md5(embed_text)` (`search/indexer.rs`), so
changing the expression self-invalidates every bookmark row and the next
`embedding_index` cron re-embeds them. Nothing to migrate, nothing to backfill.

### 2. URL fetcher — native first, Parallel Extract as escalation

Bookmarks need something narrower than a search product: *fetch this one URL I
already have, give me readable text* — `<title>`, og:image, article body. Two
tiers, matching the escalation philosophy the video tiers already use:

- **Native default** (`reqwest` + a Mozilla-Readability port; `readability-rust`
  and `readable-readability` are the live candidates, and `lol_html` /
  `fast_html2md` are the recommended pair for LLM-bound extraction). Free, keeps
  the URL on-box, and uses the **residential IP** — which is the whole reason
  YouTube caption tracks and paywall-lite pages work from here at all.
- **Parallel Extract on failure** (JS-heavy SPAs, bot walls). Per-source
  toggle; it sends the URL off-box, so it is opt-in, never the default.

Must be native Rust in core, not applet code — the `virtues_applet_writer` role
can't write `data_*`.

### 2b. Exa → Parallel (the search leg)

Same vendor surface, so it rides along here. `web_search` is Exa-only
(`tools/web_search.rs` → `api/exa.rs`), and Exa is already **proxied through
virtues-api** via `BearerClient` for budget enforcement, with `Service::Exa` as
its own line in the wallet ledger (`api/usage.rs`).

**Integration shape — the non-obvious part.** Parallel on the Vercel AI Gateway
is exposed as `gateway.tools.parallelSearch()`: an *inference-time* tool the
gateway executes during a model call, in the AI SDK's tool-calling model. Core
does not work that way — it runs its own executor (`tools/executor.rs`) and
returns its own `ToolResult`. So the gateway form does **not** drop in, and the
"centralized billing, one key" pitch doesn't apply to us.

The actually-small change is swapping the upstream behind the proxy we already
have: `api/parallel.rs` replacing `api/exa.rs` on the same `BearerClient` path,
`Service::Exa` → `Service::Parallel`, `web_search.rs` mapping arguments to the
new shape. Keeping the distinct ledger line matters — search spend rolled into
`ai_gateway` would make the Usage view unable to say what web search cost, and
that view was just rebuilt to report honestly (`e16312d5`).

**Parameter mapping** (Parallel takes `objective` + up to 5 `search_queries` of
≤200 chars, `max_results`, `max_chars_per_result`, `processor: base|pro`):

| Exa today | Parallel | Note |
|---|---|---|
| `query` | `search_queries` | plus `objective` — a genuine gain; the tool can pass intent, not just keywords |
| `deep: true` | `processor: "pro"` | same escalation semantics |
| `search_type` auto/keyword/neural | — | no equivalent; drop the argument |
| `num_results` | `max_results` | 10/request default |
| `category`, domain and date filters | partial (`search_domain_filter`) | **hardcoded `None` in `web_search.rs` today** — dead arguments, no real loss |
| `max_age_hours` | unclear | the one live argument with no documented equivalent — **spike it** |

Cost lands about where Exa is: $0.005 base + $0.001 per additional result.

Requires a virtues-api leg (a `/parallel/*` proxy route beside the Exa one), so
it is small but not purely local — that repo deploys separately.

### 3. `bookmark_enrichment` applet — the queue

Copy `document_extraction`'s shape exactly, because it already solved this:
cron applet, drain a status column, claim one item at a time, commit per item,
stale claims recover by age, generous `timeout_s` because a first-run backfill
is legitimate work and not a hang.

- Migration: `enrichment_status`, `enriched_at`, and the extraction record
  (JSONB on the row, or a derived table — decide at build time; JSONB keeps the
  re-runnable-derived-data property either way).
- Omni slot for anything with pixels or audio, resolved via
  `default_model_for_slot(ModelSlot::Omni)` the way `transcription_resolution`
  does it — never a literal.
- Lite slot for text composition (summaries, chapters, `likely_queries` from
  fetched page text).
- Budget: daily cap in Settings, spend read from the prepaid-wallet ledger.
  Newest-first drain, lazy tail on touch, explicit priced bulk button.

### 4. Aspect embedding — start concatenated

**Decision (revised from the original plan's "direct-write aspect rows"):**
concatenate the extraction prose into `embed_text_sql` first. The chunker
already windows at 96 words with 14 overlap, so concatenation yields
pseudo-aspects for free, with no indexer surgery and no bypass of the normal
pipeline. The cost is blurred boundaries — a window that is half palette, half
OCR text.

Build real aspect rows only when per-aspect **attribution** is wanted ("matched
on palette"), not merely per-aspect recall. That defers the invasive change
until a surface actually needs it.

### 5. iOS share sheet — spike before committing

The highest-value door (the only capture path for Instagram) and the riskiest
work in this plan. Shape: a Swift share extension writes to a shared App Group
container and wakes the app via `openURL`; the app posts to the existing
`ios_ingest` webhook with `stream: "bookmark"` — a new arm in the dispatcher
alongside healthkit/location, no new applet.

**Research gap, and it is a real one.** A share extension is a *second Xcode
target*. Tauri supports overriding the XcodeGen `project.yml` to add one — but
this repo's `project.yml` is inert post-init (the standing trap: iOS config
changes go through `Info.ios.plist`), and there is a known upstream issue where
`cargo tauri ios dev` fails to build once an app extension is added
(tauri-apps/tauri#10074). Spike this before scheduling it; the fallback is a
plain iOS Shortcut posting to the webhook, which is uglier but unblocks the
Instagram story immediately.

**Schema pressure to resolve here**: `url` is `NOT NULL`. A share-sheet save
carries the post URL, fine — but a raw camera-roll screenshot has none, and
today has nowhere to live.

### 6. X — bookmarks, likes, and own posts

Needs virtues-api proxy work (`/x/start`) plus the billing spike. Researched
2026-08-05:

- **Owned reads are $0.001/resource** (~$1/1000) across ~12 endpoints —
  bookmarks, posts, likes, followers, lists — when pulling *your own*
  authenticated account. Pay-per-use is the default for new developers; there is
  no meaningful free tier.
- **Resources dedupe within a 24h UTC window.** Re-requesting the same post the
  same day is not charged again — so frequent incremental polling is
  effectively free, and the sync can be aggressive.
- **The ~800-bookmark ceiling is real and worse than documented**: pagination
  frequently dies after 2–3 pages with no `next_token`. API sync covers ongoing
  capture and recent backfill only; full history needs an export or the
  extension path.
- **Bookmark *folders* are dead as a signal** — that endpoint returns only 20
  IDs and rejects pagination. This kills mechanism #2 (containers-as-native-why)
  for X specifically. Folders survive as a why source for browser bookmarks and
  Are.na/Raindrop; for X, the whisper has to come from capture-time or review.

**Ontology mapping — the split that matters:**

- A **like or a bookmark is a save** → `data_content_bookmark`, discriminated by
  the existing `bookmark_type` (`bookmark` | `like` | `star` | `save`). No new
  table, and they inherit enrichment and IR for free.
- An **own post is authored content**, not a save. It needs a new ontology.
  `data_content_conversation` is the tempting reuse and it is wrong — its `role`
  is `CHECK`-constrained to `user`/`assistant`/`system` for AI chat logs.
  `data_communication_message` is 1:1/group messaging; a public post is
  addressed to no one.

**Proposed `data_content_post`** — things I published, anywhere: `post_type`
(`post` | `reply` | `repost` | `quote`), `text`, `url`, `conversation_id`,
`in_reply_to_id`, engagement counts in `metadata`. Replies and comments are the
same table under a different `post_type`. Embeds trivially (the text is prose),
and it is the table future Bluesky/Mastodon/LinkedIn streams land in — worth
designing once, deliberately, rather than bending an existing table now.

### 7. UI — after enrichment writes something

Inbox/Library SubNav, extraction fields as facets, search-by-color off the
palette hex swatches (deterministic, no ML), status line ("212 awaiting
enrichment"). Bento becomes a **third** `UniversalDataGrid` display mode
alongside table and card — note the grid once had a third mode (Board) that was
removed on principle, so this one needs to earn its place; bookmarks-gated
until it does.

Also still owed: the generic `/api/ontologies/*` routes (available / overview /
{table} / data), which apps/web already calls and which 404 for **every**
ontology today.

Deferred: browser extension (blocked on the iroh-gate story — webhook auth is
proven-device-key only; relay via Mac collector native messaging is the likely
path); file importers (Netscape HTML / Pocket CSV / Omnivore JSON — DECIDED
2026-07-28 to skip: most incoming users have no existing bookmark system, and
one-off imports aren't the habit loop); Raindrop/Readwise/Reddit/Are.na/
YouTube-playlists (add by demand).

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

Live (blocking something):

1. **Tauri iOS share extension** — can a second Xcode target be added given an
   inert `project.yml` and tauri#10074? Blocks step 5; fallback is a Shortcut.
2. **X billing classification** — do proxied user-context reads through
   virtues-api bill as *owned* reads at $0.001? Blocks step 6 only. The
   per-resource rate, the 24h dedup window, and the endpoint list are settled;
   the proxy's effect on classification is not.
3. **`data_content_post` shape** — confirm before writing the migration, since
   it is the table every future social source lands in.
4. **URL-fetch failure rate** — what fraction of real saved URLs the native
   readability path actually handles, which sets how often Parallel Extract
   gets invoked and therefore whether the escalation tier is worth building.
5. **`max_age_hours` on Parallel** — the one live `web_search` argument with no
   documented equivalent. If freshness/recrawl control is genuinely absent, the
   "use `1` for news/sports/live data" instruction in the tool description has
   to go, and the model loses a lever it currently has.
6. Browser-extension → box auth path (iroh gate).

Resolved since 2026-07-28:

- ~~Does changing `embed_text_sql` need a reindex?~~ No — the indexer keys on
  `md5(embed_text)` and self-invalidates.
- ~~Is there a URL-fetch capability?~~ No, and Parallel Extract on the Vercel
  gateway is now the escalation tier rather than a second vendor integration.
- ~~X bookmark folders as container-why?~~ Dead — 20 IDs, no pagination.

Deferred (v2 or later):

- MobileCLIP2-S4 on Hexagon/QNN feasibility (similarity-only sidecar).
- Reel MP4 lake-insurance default: on or off?
