# Bookmarks — what is still open

The design and everything built is recorded in
[`../record/bookmarks.md`](../record/bookmarks.md). Section 1 was written
against the code on 2026-11-24; the rest was checked on 2026-09-29.

## 1. Redesign the room and the bookmark page

Before this redesign the room
([`BookmarksView.svelte`](../../apps/web/src/lib/components/tabs/views/BookmarksView.svelte))
paged 25 saves at a time and showed most synced posts as a host name over a
caption: an Instagram tile had no title and no `thumbnail_url`, so it read
"instagram.com", and an X tile read "x.com" beside the raw `social_post`
kind. One sync can bring hundreds of posts, so this is the common case.
The redesign was drawn in Claude Design on 2026-11-24 from an outside
proposal and checked against `agents/build/design.md`.

**What people come here to do:** find one thing they remember, recognize
things by sight while scrolling, and open the original. Search does most of
the work once a shelf has hundreds of saves.

### Who each persona gets

- Narrative seeker: their note shown as their own words, apart from the
  saved thing, and search that matches what they remember.
- Quantified self: little changes for them, and nothing gets worse.
- Tech prosumer: a picture on every card that has one, and posts that look
  like posts.
- Recovery: no counts on the filters, no "Not read yet" on cards, no backlog
  line.
- Data sovereign: no picture is loaded from a platform's servers, because
  every picture is a Drive copy.

### Decided on the board

- **One layout rule for every card.** The headline is the first of: title,
  post text, picture, address. A picture above a title is a cover. A
  picture under post text is an attachment. Synced posts whose picture is
  the content (Instagram) show author, picture, then caption; the owner
  confirmed this on 2026-11-24 ("confirm instagram picture first"). A picture that
  fails to load is hidden and the card lays out as if it had none. A domain
  is never a headline.
- **The room.** One sentence under the page bar, a search field as the
  widest control, a Wall and List segmented control, and source chips (All,
  Instagram, X, Browser, Saved by hand, iPhone) with no counts, since
  design.md bans counts. Browser covers Safari, Chrome, Arc and Dia. Saves
  are grouped This week, Last week, Earlier in [month], then months, then
  years, with weeks starting Monday. More load as you scroll. The Wall has
  three columns and places each card in the shortest one, left to right, so
  date order holds across a row.
- **The card.** A `Card`: the saved thing in serif (post text at
  `--text-body`, a title at `--text-lead`), everything else in sans at
  `--text-small`. The note is a `--fill-bg` block in sans. The footer says how
  it arrived and when ("Chrome · Nov 21"). No read-state label.
- **The bookmark page.** The saved thing on the left, a side column on the
  right that moves below it in a narrow pane. Side column order: the note,
  the one primary Button "Open in [site]" with "Saved [date] from [source]"
  under it, Mentions, then "How Virtues indexed this", collapsed, holding
  About, "Search finds it with" and "Read [date] by your Lite model". The
  last crumb names it by its headline: the title, "Post by [author]",
  "Saved screenshot" or the address. Short links (t.co) never show; a link
  preview shows where they lead. Tags stay read-only.
- **Removed:** the Table and Cards views, the kind, type and read-state
  filters, the shelf status line, the spine tile, and the "Your note"
  textarea that is always open (an empty note becomes an "Add a note"
  button; saving it blank deletes it).

### What is built, and what is left

Slices A to E were built on `wave` on 2026-11-24 and checked against a copy
of the dev database. They are not on a real server yet. Where the build
differs from the first version of this plan:

- **Pictures.** The API returns `picture_url`, a Drive route, and never a
  remote link (`picture_url_sql` in `api/bookmarks.rs`). Instead of a change
  to each sync, the enrichment applet ends each run with
  `bookmark_media::keep_remote_pictures`, which copies up to 25 remote
  `thumbnail_url`s a run into Drive and names each copy in
  `metadata.picture_id`.
  That one step covers X pictures, page preview images and every row saved
  before it existed. A link that cannot be fetched is marked
  `picture_failed` with the time and tried again a week later
  (`RETRY_AFTER_DAYS`), so a box that was offline for a run keeps those
  pictures.
- **Filters.** The chips filter by door (`DOOR_SQL`: Instagram, X, GitHub,
  iPhone, Saved by hand, Browser), since an iPhone share from Instagram has
  `source_platform = 'instagram'`. The list API's `bookmark_type`, `medium`
  and `state` filters and `ShelfCounts` are gone.
- **Save time.** `saved_at` is `created_at` for X, Instagram and undated
  rows, and the source's time for every other door (`SAVED_AT_SQL`). Ties,
  such as a first sync's hundreds of rows, break on the post's own time.
- **Search** also matches `author` and `extraction_text`.
- **Posts lead with their text** even when they have a title: enrichment
  fills a post's `title` from the page's title tag ("Name (@handle) on X").
- **List** is one line per save with no pictures.
- `Segmented` already existed and is used for Wall and List; `Chip` is new.
  design.md's Primitives table says so.

Slice F was built the same day:

- **X sync** keeps the author's display name and profile picture, each
  post's links (`short`, `expanded`, `display`) and the short link X appends
  for the post's own picture (`media_short_url`). The room shows where each
  link leads and drops the picture's link.
- **Page facts.** The fetcher reads `og:site_name`, a video's duration and
  the site icon. The enrichment pass writes them with a reading time as it
  reads a page (`bookmark_facts::page_facts`): a reading time only for an
  article and a length only for a video, by the record's `medium`. And
  `bookmark_facts::fill_page_facts` fills pages read before that, one fetch
  each and no model call.
- **Link previews.** `bookmark_facts::fill_link_previews` reads the first
  page an X post links to, off X, into `metadata.link_preview`.
- **Pictures.** `keep_remote_pictures` copies four kinds into Drive: the
  bookmark's picture, the author's picture, the site icon and the link
  preview's picture. No SVG is kept, since an SVG served from the box's own
  address could run script.
- **Mentions** link to a Wiki page of exactly the same name
  (`api::bookmarks::link_mentions`): only when one page has that name, and a
  person only for a name of two or more words. Repeated names are shown
  once.
- **Open** goes through `openWebLink`, and only for an http or https
  address: a browser can sync a `javascript:` bookmarklet, which the old
  page would have run.
- The API returns all of this as one `details` object of Drive routes, with
  absent facts left out. None of it needed a migration: every new field is
  in `metadata`.

Left:

- **Posts saved before this sync change** have no links, picture or name in
  `metadata`, because the sync stops at the newest post it saw last time. The
  room reads their author's name from the page title enrichment kept. To fill
  the rest, clear `newest_id` in the X source's config so the next run walks
  the timeline again (up to X's cap of about 800).
- **Instagram authors** show the handle only: `instagram_saves_sync` keeps
  the username and not the full name or picture.
- Favicons that are SVG are skipped; most sites also offer a PNG.
- **Search.** Bookmarks in the search palette are another session's
  uncommitted work in `search/results.ts`, `SearchList.svelte` and
  `SearchPreview.svelte`. Its rows now use `bookmarks/shape.ts` (`rowOf`), so
  a post reads by its text, a page by its title and site, and never by a
  domain. Committing it means committing that session's feature with it.
- **`agents/build/privacy-boundary.md`** has no row for what bookmark
  enrichment sends or fetches: the image pass sends pictures to the Omni
  slot, and the box fetches saved pages, the pages posts link to, author
  pictures and site icons from their own servers. Another session had the
  file open on 2026-11-24.
- `DOOR_SQL` files any row whose platform it does not know under Browser. A
  new door needs its own case there.

### Still open

- **Duplicates.** A post saved in the browser and the same post from X sync
  have different addresses and show twice. Merging them is a data decision.
- **Video and audio enrichment** (section 4) would fill the duration and
  thumbnail facts for saved videos.

## 2. Wire the iOS share extension

The extension (`apps/web/src-tauri/gen/apple/ShareExtension/`), the app-side
drain (`ShareInbox.swift`) and the box arm (`ios_ingest/bookmark.rs`) are
built. The extension is in no Xcode target. In order, each step depending on
the one before:

1. ~~Register the App Group and `com.virtues.app.share` with it.~~ Done
   2026-10-07, for the Control Center control
   ([record/audio-schedule-places.md](../record/audio-schedule-places.md)),
   which needed the same group.
2. ~~Add the application-groups entitlement to the app.~~ Done the same day.
   `ShareInbox` now finds the shared container; until the extension writes
   to it, there is nothing to drain.
3. Add the target by editing the project in place — **never by running
   `xcodegen generate`**. Measured 2026-09-23: regenerating drops five
   Tauri-merged `Info.plist` keys (Bluetooth — BLE onboarding crashes without
   it — HealthUpdate, camera, photo library, icons) and rolls the version back.
   Review a diff containing the extension and nothing else; mirror it into
   `project.yml` only so the spec stays accurate.
4. Stamp the extension's version from the app's at build time. As written it
   reads `$(MARKETING_VERSION)`, which the project does not define, and it must
   equal the app's or upload fails.
5. Release the box first. An app with the extension talking to a box without
   `externalize_images` loses screenshot-only shares while reporting success.
6. On a real phone, measure how long a shared link and a screenshot take to
   reach the box. That number decides whether any wake-the-app workaround is
   warranted.

## 3. `data_content_post`

Own posts are authored content, not saves, and fit neither
`data_content_conversation` (role is constrained to AI-chat values) nor
`data_communication_message` (a public post is addressed to no one). Proposed:
`post_type` (post | reply | repost | quote), `text`, `url`, `conversation_id`,
`in_reply_to_id`, engagement in `metadata`. Settle the shape before the
migration; every future social source lands in it.

## 4. Smaller items

- **Video and audio enrichment.** The duration tiers in the record are decided;
  nothing claims video or audio rows yet.
- **Colour search.** Needs dominant colours stored as hex per bookmark
  (deterministic, no model). The image pass writes `style` prose only.
- **Enrichment budget in Settings.** The daily cap is
  `VIRTUES_BOOKMARK_ENRICH_DAILY_CAP` only; no Settings knob, no priced
  "enrich all" button.

## 5. Browser extension auth

Webhook auth accepts only a paired device's key, which a browser extension does
not hold. The likely path is to hand off to the Mac collector over native messaging
instead of giving the extension its own identity. The share sheet uses the same
approach: it hands data to the app, and the app talks to the box.
