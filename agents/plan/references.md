# References — one primitive for @, peek, and open

**Status:** Design (2026-07-07). Narrative only; no implementation yet.

## Problem

Everything that can be pointed at — a person, a place, an org, a thing, a file,
a photo, a PDF, an audio clip, an external URL, a page — is currently rendered
by a *different* code path. Inline, the same three references look like three
unrelated widgets:

| Rendered | Path | Look |
|---|---|---|
| `@Paul Henry Flynn` | `EntityChip` (isEntity branch) | gray pill + `@` |
| `@virtues.com` | `EntityChip`, no pill background applies | icon + text |
| `Server.jpg` | bare `<a target="_blank">` (`:else` branch) | plain blue link |

Dispatch lives in [`CitedMarkdown.svelte`](../apps/web/src/lib/components/CitedMarkdown.svelte)
(`link` snippet: entity → `EntityChip`, else → `<a>`). Citations use a *fourth*
renderer (`InlineCitation.svelte`); Drive files a *fifth*. Five components each
decide independently what a reference looks like. There is no reference
primitive — only five things that happen to be references.

## The rename (the actual fix)

The word `@` produces is a **Reference** (`Ref`). Not an "entity," not a
"backlink" — those words leak implementation (wiki-entity, page-backlink) into
what is one idea: *a pointer from here to a target.*

- A **Ref** is a typed pointer: `{ target }` where target is an id or URL.
- A **Target** is anything a Ref can point at. People, places, orgs, things,
  files, photos, PDFs, audio, pages, external links — all just targets.
- **Backlinks** become **inbound references** — "what refers to X" — the same
  edge read the other direction. No separate concept.

Once "Reference" is the noun, the five special cases collapse: a person and a
PDF are not different widgets, they are the same widget pointing at different
targets.

## Three densities, not three components

A Ref renders at one of three densities. The seam between them matters:

- **Pill** — inline, in a line of text. Icon + name. Target-agnostic shape.
  *Always identical* whether it points at a person or a PDF. (`@Paul`, `@Server.jpg`)
- **Preview** — a hover/focus card (rename from "lite"/"peek"). Bordered:
  thumbnail + title + 2–3 target-specific facts + primary action. A person shows
  relationship + last-seen; a PDF shows a page-1 thumbnail; audio shows a mini
  waveform. In flow, not inline. ("Preview" is the density; "card" is its visual
  form — a Preview card.)
- **Open** (full) — the target rendered at full fidelity. A route/tab, not an
  inline component.

**The load-bearing distinction:** pill and preview are *genuinely universal* —
one component, target-agnostic, same interaction everywhere. Open is *not*
universal — a person's full view and a PDF's full view share nothing. The seam
falls exactly on the inline-component / route boundary that already exists.

> One `<Ref>` component owns pill + preview. **Open** is a route that dispatches
> by target type — which the tab registry already does.

This resolves the "should it be one universal component?" tension: yes for
pill/preview, no for open. Stop forcing one component across the preview→open seam.

### Interaction

- Default inline = **pill**.
- **Hover / focus** (no right-click, no wait) reveals **preview** — a floating card.
- Preview offers **Open** (full route) and a **"turn into…"** affordance to swap the
  inline density: pill ⇄ preview-embed ⇄ full-embed. "Turn into" writes the chosen
  density back into the source (page markdown / chat), so an embed is a Ref with a
  density attribute, not a different token.

## Open: the universal asset route

"Open" for a file target is a new tab type — call it **AssetView** — registered
in [`registry.ts`](../apps/web/src/lib/tabs/registry.ts) exactly like
`person_ → WikiDetailView`. It dispatches by MIME to a per-kind surface:

- **image** — lightbox (reuse `MediaLightbox`), zoom, metadata.
- **audio** — scrubber + waveform, transcript, timestamped notes.
- **video** — player + chapter/scene notes.
- **pdf** — reader with highlight, note-in-margin, OCR-to-text.
- **link** — readable snapshot / oEmbed card + original.
- **generic file** — preview + download.

People/places/orgs/things keep their *own* full views (`WikiDetailView`) — they
are targets whose "open" is a profile, not a file surface. That is fine and
expected: open is the one density that is allowed to differ by type.

**Decided — canonical surface per type.** Identity ≠ representation: a person is
one identity that may have a record, a page, and files, but Open lands on *one*
canonical surface per target type (person → profile, PDF → reader). The
canonical surface aggregates the rest. This keeps structured profile affordances
and means `WikiDetailView` survives — we are *not* converging everything into an
editable page.

## What already supports this

Closer than it feels — this is mostly *removing* special cases, not adding
architecture:

1. **Tab registry is already a type→view dispatcher.** Adding `file_`/`media_`
   → `AssetView` is the same pattern, not a new one. The asset route = one more
   tab type.
2. **`routeToEntityId` / `parseEntityRoute` already normalize refs to
   `{type, id}`.** That *is* the Ref addressing scheme. A Ref is an
   id-or-URL; both entities and assets already resolve via
   `/api/wiki/resolve/:id` and `/api/media/:id`.
3. **`@`-search already exists.** `search_entities` autocompletes people/
   places/orgs/things. Adding files to it is a widening, not a new system.

## The data-model bridge (decided: assets ARE refs)

The one non-cosmetic change. Today `Server.jpg` is an `app_drive_files` row, not
a ref target, so it cannot be `@`-mentioned the way a person can. Two small
additions make assets first-class:

- Give drive files a **ref-addressable id** that `parseEntityRoute` / `resolve`
  accept (`file_<hash>` or reuse the media id).
- **`@`-search** (`search_entities`) includes files alongside entities, so `@`
  surfaces a photo the same way it surfaces a person.

Inbound references for files ("3 pages reference Server.jpg") can piggyback on
the existing `get_page_backlinks` content-scan once it recognizes file ids — no
new graph table required.

Vocabulary sweep (authored-@ layer only): `EntityChip` → `Ref`, `EntityPicker` →
`RefPicker`, `entityRoutes` → `refRoutes`, `search_entities` → `search_refs`. The
codebase should stop saying "entity" where it means "reference target" — `@`
picks a *reference*, and a reference can be a file or link, not just an entity.

## Out of scope (leave running systems alone)

Two other subsystems already produce pointers and **work today — do not
refactor them here:**

- **Entity resolution** (`entity_resolution/people.rs`, `places.rs`) auto-detects
  entities in your data and writes `wiki_entity_refs`. This powers backlinks/
  search. It is a *derived* pointer, live in production.
- **AI citations** (`citations/builder.ts`, `InlineCitation`) point an answer's
  sentence at its source. Live in production.

Both *could* one day render through the same universal pill, but unifying them is
elegance we don't need now. This doc covers the **authored `@`** path only.

Also deferred:
- **Density persistence format** — how "turn into embed" serializes into page
  markdown / chat needs its own spec.
- **Preview data-fetch strategy** — prefetch on hover-intent vs on-open.

## Sequenced next steps (when we build)

1. `<Ref>` component (pill + preview), route the inline paths in `CitedMarkdown`
   through it — pure UI unification, closes the screenshot inconsistency with no
   data change. **Shippable on its own.** — DONE
2. `AssetView` tab type + registry entry; wire image/audio/pdf surfaces. — DONE
3. Data bridge: file-as-ref-target id, `@`-search includes files, file backlinks
   via existing content-scan. — (files already in `@`-search + route)
4. Vocabulary sweep (authored-@ layer): `EntityChip`/`EntityPicker`/
   `entityRoutes`/`search_entities` → `Ref`/`RefPicker`/`refRoutes`/`search_refs`.
   — DONE

---

# Phase 5 — Embeds (the block density)

Field feedback (a `@Linda` sitting alone on its own line) showed the gap: a pill
is built to sit *inside a sentence*; alone on a line it reads as "…is that it?".
The missing density is the **embed** — the persistent inline card. Four densities
total:

| Density | Trigger | Body |
|---|---|---|
| **Pill** | inline, mid-sentence | icon + @name |
| **Preview** | hover / focus | floating card (Phase 2) |
| **Embed** | **auto when a ref is alone on a line** | persistent card, per-type body |
| **Open** | **⌘/Ctrl-click** (plain click → preview) | full tab (AssetView / WikiDetailView) |

Decisions (locked):
- **Promotion = automatic.** A line whose entire content is one ref link renders
  as an embed; mid-sentence refs stay pills. No manual "turn into" needed for the
  common case (the menu affordance can still force either density later).
- **Click model = preview-first.** Plain click shows the preview; ⌘/Ctrl-click
  opens a tab. A click never yanks you out of writing. Unify chat + editor (today
  the editor dispatches `page-navigate`, the chat pill opens a tab — inconsistent).

Per-type embed bodies (the card differs by target — same principle as "open is
allowed to differ by type", one density down):
- **person** → avatar + name + relationship + last-seen
- **place** → map (see below); until then a schematic (name + address + coords)
- **file (image)** → thumbnail · **file (other)** → icon + name + size
- **link** → favicon + title/domain (OG later)

### Place maps — cache warmed at resolution, private by default

The name and coordinates are **free** — entity resolution already writes
`wiki_places.name/latitude/longitude`. No lookup needed. The *only* thing that
ever leaves the box is the map **imagery** (pixels), and only if we want a real
map vs. a schematic dot.

- **Cache unit = a coarse tile bucket** computed by rounding lat/long (pure math,
  offline). Nearby places hash to the same bucket → one cache entry per region,
  not per place. A person frequents ~5–15 regions ⇒ **single-digit MB, forever.**
  (Bundling the planet would be ~100 GB — a non-starter; we never do that.)
- **Warm at resolution time:** when a place is resolved, enqueue a one-time fetch
  of its region's imagery into the local cache if absent — background, through a
  **box proxy** so the client never talks to a third party directly.
- **Adaptive default:** because the cache is warmed at resolution, by render time
  the map is usually already local → show it immediately, **offline, private**.
  Un-warmed places fall back to schematic + tap-to-fetch (which warms it).
- Tile provider + dark-theme styling are downstream sub-decisions (a
  self-hostable source like PMTiles/Protomaps avoids keys and leaks entirely).

### Build shape

- **`RefCard.svelte`** — the shared card *body*, per-type; fetched summary
  normalized via `refSummary.ts` (cached). Used by both Preview (floating shell)
  and Embed (block shell) so they never diverge.
- **`RefEmbed.svelte`** — block wrapper around `RefCard`.
- **Editor** (`ref-links.ts`) — detect a line that is only a ref link → render a
  block embed widget (Svelte `mount()` into the CM widget) instead of the pill.
- **Deferred to later slices:** the preview-first *click model* change (its own
  behavior slice across surfaces); chat-markdown block embeds (ref-only
  paragraph → embed); real map imagery + the box tile proxy/cache; the
  editor-pill missing-icon bug (`<iconify-icon>` vs the Svelte icon registry).
