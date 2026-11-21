# Sources redesign

**Status:** drawn on 2026-11-20 and liked by the owner ("This looks
fantastic!"). He then said "lets build", and slices 1 to 4 are built on wave
with the proposals below as written, not yet run on a server. The decisions
at the end are still open and can change what was built.

The design is four boards on one Claude Design canvas, private to the owner:
<https://claude.ai/artifact/3fKB4LyFaS4qXwdT3ApAsK>. The boards are the
Sources page, one account source (Google), one device (iPhone), and the
connect window.

## Who it is for

Checked against [personas](../build/personas.md), primaries first.

- **Narrative seeker.** Each source says in a sentence what it brings, and the
  state of a source is a sentence ("Strava needs you to sign in again").
- **Quantified self.** Health, places, money and runs lead the list. Each
  stream says how far back it goes and has a small line of arrivals.
- **Tech prosumer.** A short, finished list, one connect window for every kind
  of source, and switches for what the assistant may do.
- **Recovery.** No counts, no "0 connected", no progress bars, no alarms. One
  sentence when something needs you. The audio schedule and muted places are
  visible on the device page.
- **Data sovereign.** Each source page lists exactly what your server reads
  and what your assistant may change. Disconnect asks whether to keep or
  delete what your server already has. Streams and SQL stay one click away.

No page marks anything as on or off your server.

## What exists today

- `applets/sources.toml` lists 11 sources: iPhone, Mac, Google, Notion,
  Strava, Plaid, GitHub (stars), X (bookmarks), Instagram (saves), Grok
  (chats) and Chat History (a one-time import).
- `virtues-core/src/tools/connection_registry.rs` `FEATURED` lists 9 MCP
  servers shown under them: Zapier, Notion, Todoist, Linear, Airtable, Canva,
  Atlassian, monday.com and Stripe. The catalog also searches the MCP
  Registry and takes a pasted address.
- `SourcesView.svelte` has five sections: Overview, Catalog, Activity,
  Streams and Query. `SOURCES_SECTIONS` in `tabs/registry.ts` reserves their
  words in the route.
- One source has two pages: `SourceDetail.svelte` lists its connections, and
  `CredentialDetailView.svelte` shows one connection (device info, the
  applets that use it, Disconnect).
- Connecting goes through `connectFlow` and `connectDispatch.ts` into four
  kinds: `oauth` (through the proxy), `api_key` (with an in-app login window
  where `login` is set in the catalog), `pair` (`DevicePairModal.svelte`,
  a handoff scan code or an address and code) and `chat_import`.
- `virtues-core/src/api/capabilities.rs` already turns assistant tools on and
  off by capability (web search, browser, applets, pages), enforced both when
  the chat offers tools and in the tool executor.
- The iPhone's audio schedule lives on the phone and is edited only in the
  phone's own settings (`MobileAudioSettings.svelte`). Muted places are
  `wiki_places` rows and are edited from a place's page.

## What changes

### The catalog

| Entry | Proposal | Why |
|---|---|---|
| iPhone, Mac, Google, Strava | Keep | |
| Plaid | Keep, shown as "Banks" | Says what you get instead of the vendor |
| Notion | Keep one row | It is listed twice today, as a sync source and as a featured tool |
| X | Keep | Signs in through the app's own window |
| Chat History | Keep, shown as "Past chats" with Import | It is a one-time import. After an import it says when, and Import becomes "Import again" |
| Todoist, Linear | Keep, as tools | |
| Instagram | Cut | Session cookies, fragile and hard to explain |
| Grok | Cut | Session cookies, same reason |
| GitHub stars | Cut | A niche way to get bookmarks |
| Zapier | Cut | It was there as the way to Gmail and Calendar. Google connects directly and the assistant has its own mail and calendar tools |
| Airtable, Canva, Atlassian, monday.com, Stripe | Cut | Business tools, not parts of a person's life |
| MCP Registry search | Cut | Replaced by one link, "Add it by its address" |

### The Sources page

One page. The section strip goes away.

- A title and one line: what your server reads from, and the tools your
  assistant can use for you.
- A sentence at the top only when a source needs you, with the button that
  fixes it. This replaces Overview.
- **Connected:** one row per source with what it brings and when it last
  sent something. A broken one says so in the row.
- **Add a source:** two columns of icon, name, one line and Connect.
- **Tools your assistant can use:** the same rows, with the line "Your
  assistant works in these when you ask. They aren't synced." It does not
  say nothing is copied: a tool's result is kept in the chat's history on
  your server.
- No search until the list passes about 20 entries.
- A new box with nothing connected leads with iPhone and Mac.

Activity and Streams move to each source's page. Query and the full list of
every stream move to Settings › Data, so the view of everything your server
keeps is still one place, and `/sources/query` and `/sources/streams`
redirect there.

### One page per source

`SourceDetail` and `CredentialDetailView` become one page at
`/sources/<id>`. An account source (Google board):

- Header with the account it reads ("Mail and calendar for …").
- A status sentence: up to date, or what broke and the fix.
- **What your server reads:** one row per stream, with how often it is read,
  how far back it goes and a small arrival line. Each row opens the stream.
- **What your assistant can do:** one switch per capability of this source.
  For Google: archive and label mail, write drafts, add and change events.
  The page says it never sends mail.
- **Recent runs:** the last three in plain words, and "All runs".
- **Accounts:** each account, Rename, and "Add another … account". A second
  account is a row here instead of a second page.
- **Disconnect** at the bottom, which asks whether to keep or delete what
  your server already has.

A device (iPhone board) differs in three places: the streams have no switches
because you allow each on the phone, and a stream that is not allowed says
where to allow it; a section shows when audio is off (the schedule and the
muted places); and a section shows the device itself (model, system, whether
the app is up to date). Unpair takes Disconnect's place.

### The connect window

Every Connect button opens one window (`Modal`), whatever the kind:

- **Account:** what your server will read and what your assistant may do,
  then "Continue to …"; a waiting step while the provider's page is open in
  the browser, with a link to open it again; then "… is connected" and a
  button to the source's page.
- **Sign in here (X):** opens the site in the app's own window. "Paste your
  session cookies instead" stays as a link for the browser, where that window
  does not exist.
- **Pair (iPhone):** the scan code, then the address and code to type.
- **Import (Past chats):** where to get the export, and a drop area.
- Every kind needs an error state that says what happened and offers Try
  again. It is not drawn.

## Slices

1. **Trim the catalog.** Remove the three sources from `sources.toml` and
   their applets, and cut `FEATURED` to Notion, Todoist and Linear. A box
   that already connected a removed source keeps a connection whose source no
   longer exists; the catalog's orphan rows already show those ("no longer
   installed") so they can be disconnected. Update
   [sign-ins-plan.md](sign-ins-plan.md), which plans session upkeep for
   Instagram and Grok. Needs decisions 1 and 2.
2. **One Sources page.** Delete the section strip and `SOURCES_SECTIONS`,
   add the attention sentence from the store's broken connections, rename
   Plaid and Chat History in the catalog, and redirect `/sources/catalog`
   and `/sources/activity` to `/sources`. Query and the full streams list
   move to Settings › Data, with `/sources/query` and `/sources/streams`
   redirecting there.
3. **One page per source.** Built 2026-11-20: `SourceDetail` is the one
   page and `CredentialDetailView` and `SourcesActivity` are deleted;
   `/sources/cred_…` forwards to its source. Streams come from
   `/api/streams/days` filtered to the source (a line of the last twelve
   weeks each) and the earliest date from `/api/lake/streams`. Runs are the
   last runs of the source's applets, with "All runs" opening Jobs. A device
   row opens its page in Settings. Not built: Disconnect's "delete what it
   has", which needs a deletion scoped to one source through
   `api::deletions` and the owner proof; today Disconnect says the data
   stays.
4. **Assistant switches per source.** Built 2026-11-20: `capabilities.rs`
   has `email` (`email_archive`, `email_label`, `email_draft`) and
   `calendar` (`calendar_event`), with Settings draft fields and refusal
   wording. The Google page shows those two switches with `SwitchList`, the
   same component and API as Settings' Tools list, which shows all six. The
   words live in `lib/sources/capabilities.ts`. A source's switches show only
   while one of its accounts is connected.
5. **Device pages.** The iPhone and Mac pages, with each stream's
   permission state as the phone reports it. Muted places link to their
   place pages. The schedule shows read-only until the box holds it (the
   deferred schedule-on-the-box item in
   [audio-schedule-places-plan.md](audio-schedule-places-plan.md)). Needs
   decision 6.
6. **The connect window.** One window and its states for the four kinds,
   including errors, replacing `ApiKeyConnectModal`, `DevicePairModal`'s
   frame and the chat import modal.
7. **Manual and cleanup.** A manual page for Sources, then delete this plan
   and leave a record if the reasoning is worth keeping.

## Decisions

Open, each needs the owner's words:

1. **The cut list** in the catalog table above, as a whole or entry by entry.
2. **Data of a cut source** on boxes that connected it (saved posts, Grok
   chats, starred repos): keep the rows, or offer to delete them through
   `api::deletions`.
3. **Notion:** the sync source, the MCP tool, or one row that sets up both.
   They sign in separately today.
4. **Brand marks:** in each brand's color (as drawn) or all in ink.
5. **Query and the full streams list:** Settings › Data, as proposed, or
   somewhere else.
6. **Device pages:** the Sources device page and Settings › Devices show the
   same phone. Pick one home and link the other to it.
7. **Audio schedule on the web:** read-only for now, or build the schedule on
   the box first so the Change link works.
8. **Wallet row:** shown only once FinanceKit ships on the phone.

## Not changed

How each provider signs in (the proxy, Truto for Google, the in-app login
window, pairing) stays as it is. Where data goes stays written in the manual
and [privacy-boundary.md](../build/privacy-boundary.md), never on these
pages.
