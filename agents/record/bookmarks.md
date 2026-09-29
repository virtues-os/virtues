# Bookmarks

Written 2026-09-29. v1 built 2026-08-04/07; X and Instagram sync and the image
pass added 2026-09-22. How saves are captured, made findable, and kept honest
about whose words are whose. The code is `crates/virtues-helpers/src/bookmarks.rs`
(the shared normalizer), `virtues-core/src/api/bookmarks.rs`,
`bookmark_enrichment.rs`, `bookmark_media.rs`, `fetch/`, and the source
applets. What is still open is `agents/plan/bookmarks-plan.md`.

## Why it exists

`data_content_bookmark` had a complete ontology descriptor — embedding config,
day-source config, a citation chip — and zero producers. Meanwhile "save the
twenty interesting things I hit today and find them again later" was a top
request, especially from designers.

## The paradigm

**Everything becomes text; the Omni slot is the eye; one index.** A save is
normalized to a row immediately — free, instant, searchable by title. A
budgeted background sweep then writes an *extraction record* into the row's
embed text. Machine text and user text stay segregated, and the user's reason
for saving is never inferred.

## Capture

Ingest applets are named for their *source* and fan out into whatever
ontologies the payload supports; there is no `bookmark_ingest`. Doors: in-app
save (`POST /api/bookmarks`, identity = canonicalized URL, so a re-save
upserts), Mac browser bookmarks via `mac_ingest`, `github_stars_sync`,
`x_bookmarks_sync`, `instagram_saves_sync`, and an iOS share sheet whose box
and app halves are built.

The normalizer owns the row shape, identity and the deletion split: snapshot
sources (a browser's bookmark file) tombstone what went missing, scoped to one
browser on one device; event sources (stars, X bookmarks) cannot, because
absence from a page means nothing. `note` and `is_archived` are outside the
upsert's update set, so a re-sync can never clobber the user's words. `tags`
is inside it — **tags are source-owned**, and the UI must never offer tag
editing, because the next sync would destroy it. User filing goes through
projects.

X and Instagram sync by replaying the web client's own request from the box
with the owner's session cookies, not through an API. X's v2 bookmark endpoint
caps around 800 and its pagination dies after a few pages; its folders endpoint
returns 20 ids and refuses to paginate. Neither platform offers a read API a
personal appliance can use for its own saves.

**The share sheet: the extension writes, the app sends.** A share extension
is a separate process with a small memory ceiling and a short life, and on iOS
the box connection is served inside the app's own process — the extension
cannot reach it, and giving it its own endpoint would mean hole-punching inside
a share sheet with a duplicate identity. So the extension copies the link,
the raw picture bytes (never decoded there) and the note into an App Group
folder and returns. The app drains that folder on foreground and on background
wakes, downscales the picture, and enqueues a `bookmark` record onto the same
outbox road audio uses. The box's `ios_ingest` arm keeps the picture in Drive
for the image pass. An iOS Shortcut was rejected as a stopgap: the webhook
wants the paired device's credential, which lives inside the app.

**What `url` holds for a screenshot** — decided 2026-08-05: where the thing
*is*, not where it came from. A screenshot with no source gets the in-app
viewer route `/drive/file_{id}`; a share with a source URL keeps that URL *and*
the image as a Drive asset named in `metadata.asset_id`. Provenance goes to
`source_platform`. Identity is the image's content hash. Two alternatives were
rejected: letting a model guess the URL (a wrong link sends the user somewhere
they never saved, and the fetcher would then enrich the wrong page), and a
nullable `url` (every consumer assumes one). Signed CDN image URLs expire
within hours, so saved-post images are copied into Drive at ingest; the sweep
would otherwise always arrive to a dead link.

## Retrieval: captions into the text stack, not a multimodal space

Researched 2026-07-28. Caption-plus-hybrid retrieval lands within about 3 nDCG
of full visual retrieval in the best head-to-head; Pinterest routes VLM
captions into production search; no vector vendor recommends one shared
text-and-image space, because it degrades text geometry. And here BM25 and the
reranker are text-side regardless, so a text representation must exist anyway.
A second engine is the magnet disease (`agents/record/ir-notes.md`). A
multimodal embedder was evaluated and rejected: multi-billion parameters,
cloud-or-GPU only, forks the index, marginal measured gain. A similarity-only
CLIP-class sidecar for "more like this" stays the v2 hedge, never fused into
search.

Aspect embedding was planned as one row per aspect and built as concatenation:
the extraction prose joins `embed_text_sql`, and the chunker's overlapping
windows give pseudo-aspects for free with no indexer surgery. Real aspect rows
wait until something needs per-aspect *attribution* ("matched on palette").
Changing the expression needed no reindex — the indexer keys on
`md5(embed_text)`, so every row self-invalidated. Tags needed a
`jsonb_typeof = 'array'` guard, because the expression runs inside the
indexer's scan and one malformed value would abort indexing for every
bookmark.

## The extraction record

One structured pass per item, low temperature, free-prose `description` first
so the model looks before filling constrained fields, then `medium`,
`subject`, `entities`, `style` and `likely_queries` — 3–6 strings of what the
owner would type to find it. Query-shaped captions were the single
highest-leverage practice found (human-rated 4.15 vs 2.21 for literal
captions). Pages go to the Lite slot, images to the Omni slot; both write the
same record, so an image is found by the same text search as a page.

**There is no `why` field in the machine schema**, by design: significance is
user-sourced, enforced by schema shape. The record is derived and disposable —
`enrichment_model` names what produced it, and it can be thrown away and re-run
when models improve.

**The fetcher is native** (`fetch/`): the box's residential IP is why caption
tracks and paywall-lite pages resolve at all, and a fetch that stays on the box
tells no third party what the owner saved. A paid extraction escalation tier
was planned and then measured out on 2026-08-07: of 34 URLs spanning docs,
news, paywalls, SPAs, video, social and commerce, 14 gave full text, 15 gave
metadata only, 4 were bot walls or 401s, 1 was a refused PDF — and **zero came
back empty**, which was the escalation's supposed target. JS-rendered pages
return og tags, which is enough for a real record. The remaining failures are
paywalls and bot walls, which defeat a paid extractor too.

## Video and audio: split by duration

Decided, and not yet built (video and audio rows stay pending and unclaimed).
Short-form under about three minutes gets a full audio-and-visual pass,
because a reel's content is visual and musical and a transcript indexes the
wrong thing. Long-form is never ingested: link, metadata, thumbnail caption and
the free transcript, composed by the Lite slot, since long-form saves are
saves of ideas whose text already exists. A full visual pass happens only on
demand. A screenshot of a frame is first-class marginalia: user-picked,
one image instead of twenty minutes of video, and a timestamp link back.

## The note, in three layers of friction

1. **Capture-time note** in owned doors. The save fires on one tap; the note
   never blocks it.
2. **Containers are the native note for synced sources.** Almost no source has
   a note field; almost every source has a container. Browser folder paths and
   GitHub topics and language are harvested as tags. X folders are dead as a
   signal (above); GitHub star lists have no public API.
3. **Review-time elicitation.** An Inbox room was designed and rejected on
   2026-08-07: an unread count nobody clears, existing mainly to host a prompt.
   The note is simply present on the detail view, which leads with it and
   separates the owner's words from the model's under a hairline. Resurfacing
   belongs in the Daily Office.

"Why" left the vocabulary the same day. It is interrogative, presumes one
reason, and does not describe what people write — todos, pointers, fragments.
The column is `note`; the prompt is "What's this for?".

## Backpressure

Steady-state capture costs cents. The hazard is a first-sync backfill —
thousands of browser bookmarks at once. So ingest and enrichment are decoupled:
sync writes cheap rows only, and enrichment is a queued sweep that drains
newest-first under a per-run and daily cap (200 by default), with bounded
retries so a permanently broken URL is not re-fetched and re-billed forever.
The cap's job is less to save money than to make a first sync visible and
interruptible. Asset-backed rows are held out of the page claim entirely, so
they burn no retries on a pass that cannot read them.

## Market notes (surveyed 2026-07-28)

Pocket (2025) and Omnivore (2024) died; survivors — mymind, Raindrop,
Readwise Reader, Karakeep — are subscription-funded, self-hosted or
AI-pivoting. Nobody does visual or video understanding beyond YouTube
transcripts; privacy and local AI are a selling point, not a niche; auto-tag,
summary, semantic search and chat with citations are table stakes, all from
existing infrastructure here. Rejected: an email-in door, and importers for
dead apps — most incoming users have no bookmark system.
