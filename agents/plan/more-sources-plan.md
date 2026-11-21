# More sources

**Status:** proposed 2026-11-21. Slices 1 to 4 (Outlook, meeting notes,
photos, imports) are built on wave (2026-11-21); none has run against a
real account, phone or download. Google Contacts is
left to the iPhone (decision 6, settled). The owner picked the sources below; how each one
connects comes from a read of each vendor's public docs on 2026-11-21 and has
not been tried against a real account.

The owner, on 2026-11-21: "lets do favorites on photos from iphone since ALL
photos might be way too much right? but all photo metadata? thoughts? then
yes granola/fathom/fireflies/zoom if theyre easy. spotify yes. outlook mail
and calendar, we can use truto for now. google contacts, yes. slack and
discord, yes. amazon orders, yes."

Every new source lands on the Sources page built by
[sources-redesign-plan.md](sources-redesign-plan.md): a catalog row, a page
of its own, and the connect window. The rule from 2026-11-20 still holds:
build our own integration first, and use Truto only where the vendor's own
approval is the obstacle. Outlook through Truto is the owner's call for now.

## Who it is for

Checked against [personas](../build/personas.md), primaries first.

- **Narrative seeker.** Meeting notes, favorite photos and messages give day
  pages and answers more of what happened, in words and pictures.
- **Quantified self.** Listening history, orders and where you took photos
  are their own numbers, ready for charts in a reply.
- **Tech prosumer.** Outlook, Slack and the meeting tools are what their
  work runs on.
- **Recovery.** Nothing here shows counts on a page ("you took 2,000
  photos", "you played this 300 times"). Photos never read the Hidden album.
- **Data sovereign.** Photos are read on the phone and go only to your
  server. Most others use a key you paste or a file you upload, so no third
  party sits in between. Outlook passes through Truto, which the manual and
  [privacy-boundary.md](../build/privacy-boundary.md) must say.

## Photos from the iPhone

The owner asked whether favorites only is right, since all photos might be
too much, and whether all metadata works. Proposal: **metadata for every
photo, and a picture for favorites only.**

- **Metadata for every photo** is small. Each photo's when, where, kind
  (photo, video, screenshot, Live Photo), size and favorite flag is a few
  hundred bytes, so a library of tens of thousands is tens of megabytes
  (an estimate, not measured). It answers "where was I", "when did I take
  photos of the kids", and lets day pages mark the hours you took photos.
- **A picture for favorites** is a copy about 1024 pixels wide, kept in
  Drive the way a saved post's image is. A few hundred kilobytes each, so
  a thousand favorites is a few hundred megabytes (also an estimate). The
  pictures let day pages show a photo you chose, and a model can describe
  them.
- **Originals stay on the phone.** Copying every original would be many
  gigabytes and duplicate iCloud Photos.
- **Never read:** the Hidden album, and what iOS keeps behind its lock. A
  photo you hide never reaches your server.
- **How:** a new iPhone plugin on PhotoKit, beside `health`, `contacts` and
  `eventkit` in `apps/web/plugins/`. It needs full photo library access.
  The first pass sends metadata in batches in the background, and
  PhotoKit's change history sends only what is new after that. Favoriting a
  photo later sends its picture; unfavoriting leaves the copy until you
  delete it.
- **Record:** a new `data_content_photo`, one row per photo, with the
  picture as a Drive file on favorites. Needs a migration number when built.

## How each source connects

| Source | How | What you get | Obstacle | Effort |
|---|---|---|---|---|
| Outlook Mail and Calendar | Truto, like Google | Mail, calendar events | Truto in the path, temporary | Easy |
| Google Contacts | The same Google sign-in, one more permission | Names, emails, phones, birthdays | Truto's page is unclear whether contacts work; test. On our own Google app, `contacts.readonly` is reportedly a sensitive scope (short review), not restricted | Easy, test first |
| Fathom | A key you paste | Meetings, transcripts, summaries, action items | None found; the API is on every plan. Webhooks need a public address, so the box polls | Easy |
| Fireflies.ai | A key you paste | Transcripts, summaries | Free plans allow 500 requests a day | Easy |
| Granola | A key you paste | Notes, transcripts, attendees | The key needs a Business or Enterprise plan; free users have no API | Easy for paid users |
| Zoom | Sign-in through our app, or your own Zoom app | Cloud recordings, transcripts | Our app needs Zoom Marketplace review (two phases, reported about four weeks). Cloud recording likely needs a paid Zoom plan | Medium to hard |
| Spotify | Import of your full listening history | Every play: when, how long, what, skips | The live API serves 50 recent plays, and Spotify no longer lets one shared app serve many people (2025-2026 rule changes). The export takes up to 30 days to arrive | Easy as an import |
| Amazon orders | Import of "Request your data" | Every order: items, prices, dates | Arrives in hours to weeks; the file's columns have changed over time, so the parser must tolerate that | Medium |
| Discord | Import of "Request all of my data" | Messages you sent | Reading messages with a personal account through the API breaks Discord's terms, so import is the only way. The package has only your side | Easy, one-sided |
| Slack | Sign-in to each workspace | Messages in your channels and DMs | Since 2025-05-29 Slack limits apps that are not in its Marketplace to one history request a minute, fifteen messages each, which makes a sync unusable | Hard without Marketplace |

## Slices

1. **Outlook and Google Contacts.** Outlook Mail and Calendar through
   Truto, routed box to api.virtues.com to Truto as Google is, writing the
   existing email and calendar records.
   - **Built on wave:** virtues-api's `routes/vendor.rs` serves Google and
     Outlook from one table (`vendor_connections`, migration 0017), with a
     read-only allow-list for Outlook (`/me`, folder message deltas,
     calendars, calendar views). Truto's `outlook` and `outlookcalendar`
     integrations share one Microsoft app on `graph.microsoft.com`, so one
     connection through `outlook` asks for `Mail.Read` and `Calendars.Read`
     together. The box's `api::vendor_api` routes both sources; the
     `outlook-mail-sync` and `outlook-calendar-sync` applets write the
     existing email and calendar records with `source_provider = 'outlook'`.
   - **Before it can be used:** install the `outlook` integration in Truto's
     Production environment (it is in neither environment today), deploy
     virtues-api, ship the box side, then sign in a real Outlook.com and a
     real Microsoft 365 account. Untested: whether Truto forwards the
     `Prefer` header (the applets cope without it), what Truto records as the
     account's address, and whether a work tenant lets a person consent.
   - **Contacts: not built.** One more scope on the existing Google
     connection does not work. Truto's custom call is tied to the
     integration's base address, `www.googleapis.com` for the Google
     connection, and Google serves the People API only at
     `people.googleapis.com` (checked 2026-11-21). The owner left contacts
     to the iPhone (decision 6).
2. **Meeting notes: Fathom, Fireflies.ai, Granola.** Three `api_key`
   sources with polling syncs, one row per meeting in the transcript record
   (decision 3).
   - **Built on wave:** catalog rows, the `fathom-meetings-sync`,
     `fireflies-meetings-sync` and `granola-meetings-sync` applets with one
     shared row writer, labels and a "where to make the key" line in the
     paste form (`apps/web/src/lib/sources/keys.ts`), and readers taught
     that a meeting row is not audio. Each run rereads three days before its
     last mark so a meeting whose transcript finished late is picked up; the
     first run reads 90 days.
   - **Whose meetings:** Fathom's key also returns meetings shared with the
     person's team, so Fathom asks for their email and keeps meetings they
     recorded or were invited to. Fireflies.ai is asked for meetings the
     key's own user organized or attended. Granola's key is made with only
     Personal notes, which still includes notes shared directly with them.
   - **Terms to read before shipping** (from each vendor's terms on
     2026-11-21): Fathom's general terms forbid "any automated tool" and do
     not except its own API. Fireflies.ai's terms ask for written consent
     before building software that interacts with the service, which
     conflicts with its public API. Granola's user terms forbid storing "any
     significant portion" of content, and its platform terms forbid building
     a competing product. None forbids a person sending their own data to a
     model.
3. **Photos from the iPhone,** as above.
   - **Built on wave:** migration 0099 adds `data_content_photo` (ontology
     `content_photo`, no lane and no day source, so no day shows a count of
     photos); the `photos` stream in `ios_ingest` keeps each favorite's
     picture in Drive under `.photos/favorites` and upserts a row per photo;
     a deletion on the phone sets `deleted_at_source`; unfavoriting keeps
     the copy. The iPhone plugin `apps/web/plugins/photos` reads PhotoKit
     with hidden assets excluded, sends metadata on a first pass that
     resumes where it stopped, then only changes from PhotoKit's persistent
     change history.
   - **Not done:** day pages marking the hours you took photos, and a model
     describing favorites. Neither was asked for in this slice.
4. **Imports: Spotify, Amazon orders, Discord.** Three one-time imports
   shaped like Past chats: upload the file, a newer export adds only what is
   new. Each needs its record (listening, orders; Discord's messages fit
   the existing message records).
   - **Built on wave:** migration 0100 adds `data_activity_listening` and
     `data_financial_purchase` (ontologies with no lane and no day source,
     so nothing shows a count); `POST /api/imports/:source/upload`
     (`server::api::imports`) stages the file and starts
     `spotify-import`, `amazon-orders-import` or `discord-import`; Past
     chats now stages through the same code. One import card in the app
     (`apps/web/src/lib/imports.ts`, `ImportCard.svelte`) serves all four.
     Discord's messages go into `data_communication_message` as the
     owner's own.
   - **Not kept from the downloads:** Spotify's IP address and country per
     play; Amazon's addresses, payment details, gift messages and tracking
     numbers.
5. **Slack,** once decision 1 is made.
6. **Zoom,** once decision 2 is made.
7. **The manual.** A line per source in `docs/setup/sources.md`, and what
   passes through Truto. Delete this plan when the last slice ships.

## Decisions

Open, each needs the owner's words:

1. **Slack:** submit our app to the Slack Marketplace (review, terms,
   security questions), or start with importing a Slack export (only a
   workspace's admins can make one), or wait.
2. **Zoom:** submit to Zoom's Marketplace review, ask each person to make
   their own Zoom app and paste its keys, or leave Zoom to the meeting tools
   that already record Zoom calls (Fathom, Fireflies.ai, Granola).
5. **Spotify live plays:** import only, or also let someone paste their own
   Spotify app's keys for the 50 most recent plays between imports.

Settled:

3. **Where meeting transcripts live: the transcript record.** The owner,
   on 2026-11-21, chose "Transcript record (Recommended)" over a new meeting
   record and the past-chats record. A meeting is one row in
   `data_communication_transcription` with `source_table = 'meeting'` and
   the tool in `source_provider`, beside the iPhone's audio chunks. Day
   pages call it a meeting; recorded minutes, audio sittings and session
   titles leave meeting rows out so nothing counts twice.
4. **Photos: metadata for every photo, a picture for favorites.** The
   owner first asked "lets do favorites on photos from iphone since ALL
   photos might be way too much right? but all photo metadata? thoughts?",
   and on 2026-11-21 answered the slice as proposed with "Yes, slice 3".
6. **Google Contacts: left to the iPhone.** The owner, on 2026-11-21:
   "Leave to iPhone". The iPhone already sends the contacts on the phone,
   Google's included when the phone syncs them. The options passed over
   were a second Truto connection through `googlecontacts` (a second
   sign-in and fee) and a custom Truto resource pointing at
   `people.googleapis.com`.
