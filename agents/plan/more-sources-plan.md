# More sources

**Status:** proposed 2026-11-21. Slices 1 to 5 (Outlook, meeting notes,
photos, imports, Slack) are built on wave (2026-11-21); none has run
against a real account, phone or download. Zoom is left to the meeting
tools (decision 2), so slice 6 has nothing to build. Google Contacts is
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

Built as **photo favorites**, on 2026-11-25. The owner: "I think too that we
start with favorites right now for photos and call it photo favorites, and
then get to the entire corpus of photo library later on once we have more
understanding of how to work with this."

- **Only favorites leave the phone.** Nothing is sent about any other photo,
  its place included. iOS offers no favorites-only permission, so it still
  asks for the whole library.
- **Postgres holds the details and a pointer; the picture is a file** under
  `.photos/favorites` on the server's disk. It is a JPEG at most 2048 pixels
  on its longest side. Originals stay on the phone.
- **Each picture is described** by the Omni slot, through the code bookmarks
  use (`image_reading.rs`): what it shows, things in it, colors, setting,
  readable words, and phrases a person would type to find it. At most 100 a
  day, newest first (`photo_enrichment.rs`).
- **Unfavoriting, hiding or deleting on the phone** removes the picture, the
  place and the description from the server and keeps the owner's note.
- **On the page:** a wall of pictures at `/photos` with one search, and a page
  per photo with the note, the date and place and what is in it.
- **How the phone decides:** each pass compares the favorites in the library
  with a ledger of what it sent to this server, so a new server, a missed
  change and a picture that was only in iCloud are all the same case.
- **Open:** what describing a picture costs (not measured), the whole library,
  day pages showing a favorite, and one photo arriving twice from an iPhone
  and an iPad on one iCloud library, whose local identifiers differ.

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
     Outlook from one table (`google_connections` with a `provider` column, migration 0017; the name stays for rollback), with a
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
   - **Built on wave:** migrations 0099 and 0111, the `photos` stream in
     `ios_ingest`, the iPhone plugin `apps/web/plugins/photos`, the
     `photo-enrichment` job, `/api/photos`, and the two views. Not run on a
     phone.
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
   - **Spotify recent plays (decision 5), built on wave:** each person makes
     a Spotify app and pastes its Client ID; the box signs in with PKCE
     (`virtues_helpers::auth::own_app`), the vendor returns through
     virtues-api's `/byo/spotify/callback`, which only bounces the browser to
     the box, and the box exchanges and refreshes the token itself.
     `spotify-recent-sync` writes plays with the import's key, and an import
     replaces a recent play with its fuller row where the two match.
5. **Slack,** as an import of a workspace export (decision 1).
   - **Built on wave:** the `slack-import` applet through the imports route
     (`/api/imports/slack/upload`), which also carries the owner's email in
     Slack so the import can find them in `users.json` and keep only
     conversations they are in. Messages go into
     `data_communication_message` with channel `slack`. File links are not
     kept: they need a Slack login and expire.
6. **Zoom: not built.** Left to the meeting tools (decision 2).
7. **The manual.** A line per source in `docs/setup/sources.md`, and what
   passes through Truto. Delete this plan when the last slice ships.

## Left after the 2026-11-21 audit

Three reviews read every slice; their serious findings are fixed (commit
after 99bea44ef). Left, none blocking a first real run:

- **Not run anywhere real.** No slice has run against a real account,
  phone or download. Outlook also needs Truto's `outlook` integration
  installed in its Production environment and virtues-api deployed.
- **Return addresses.** virtues-api accepts a return address only on a
  private network, `localhost`, `.local` or `virtues.com`, the same rule
  Google's sign-in has. A box reached at its own domain or a tailnet name
  cannot sign in to Outlook or Spotify, and Outlook then says to link the
  server, which is the wrong reason.
- **Spotify's redirect address** in the app window is written into the app
  (`auth.virtues.com`); a box pointed at another OAuth proxy would show the
  wrong one. Signing in again asks for the Client ID again although the box
  has it.
- **Spotify's two clocks.** A recent play and a download play merge only
  when Spotify's `played_at` and the download's stop time agree to the
  second, which Spotify does not document.
- **Imports** check a staged file by its folder's name, not its full path.
- **Imported sources stay under "Add a source"** in the Sources list,
  because the list sorts by sign-ins and an import has none. Each one's
  page says when the last import ran and what it brought in.

## Checked on 2026-11-22

A clean clone of `wave` ran every suite, the CI lints, and a scratch server
on a fresh database (every migration applied). Through the real upload
route, invented Spotify, Amazon, Discord and Slack downloads each wrote the
expected rows; a second upload of each added nothing; a wrong file, a Slack
email missing from the export, and a second upload while one runs each gave
one sentence. The Spotify own-app sign-in built Spotify's address with PKCE
and the bounce state, and a made-up code came back as a failed sign-in, then
as an expired one. The Sources pages and the import and Spotify windows were
looked at in the app.

Fixed from that check and a fourth review:

- **Outlook connections have their own table** (virtues-api migration
  0018). In `google_connections` the image before Outlook would delete them
  when it listed Google, and a roll forward would then ask every Outlook
  account to sign in again.
- **A bare date in an import is noon UTC**, so an older Amazon download's
  order lands on its own day in the Americas.
- **The Spotify sync passes over a play a download already holds**, so a
  download imported before the sync first ran is not listed twice.
- **A first photo pass that ran partly under Limited access** reads the
  whole library again once access is Full.
- **Fathom's wrong-email failure** does not fire when a teammate at the
  email's own domain recorded the meetings.
- **The iOS sheet closes** when saving a Spotify sign-in fails.
- **An import source's page** says when the last import ran and what it
  brought in, or why it stopped.

Not covered: photos from a phone, the three meeting tools, Outlook, and a
real Spotify app, which all need a real account or device. One core test,
the size limit on the `applet` tool's description, fails on `wave` from the
sites work, not from these sources.

## Decisions

Open, each needs the owner's words:

None open.

Settled:

1. **Slack: import a workspace export.** The owner first chose "Apply to
   Slack's directory" on 2026-11-21. Slack's own pages showed a listing
   would very likely be returned: the guidelines list apps that "export or
   backup message data" and apps with nothing inside Slack as unsuitable,
   submitting needs 10 active workspaces, the API terms forbid bulk export,
   and an app outside the directory reads 15 messages a minute. With that in
   front of them the owner chose "Import an export". Only a workspace's
   admins can make one.
2. **Zoom: left to the meeting tools.** The owner, on 2026-11-21, chose
   "Leave to meeting tools (Recommended)" after a read of Zoom's own pages:
   anyone outside our Zoom account can sign in only after Marketplace review
   (security test results, a design document, a test plan); the API terms
   forbid creating copies of data read through the API; and the API reaches
   only meetings the person hosted, on Pro plans or above. Fathom,
   Fireflies.ai and Granola record Zoom calls, including ones the person
   only attended. No Zoom source is built.
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
   and on 2026-11-21 answered the slice as proposed with "Yes, slice 3". On
   2026-11-25 they narrowed it to favorites, "Yes, build it" (above).
5. **Spotify recent plays: built, with each person's own Spotify app.**
   The owner first chose "Add recent plays". Spotify's Developer Policy
   (2025-05-15) says not to ingest Spotify Content, which includes user data
   read through the API, into an AI model, not to analyze it, and not to
   store it indefinitely; a development-mode app needs its owner on Premium
   and a new sign-in every six months. With that in front of them the owner
   said "build it anyway. we're not trainign etc".
6. **Google Contacts: left to the iPhone.** The owner, on 2026-11-21:
   "Leave to iPhone". The iPhone already sends the contacts on the phone,
   Google's included when the phone syncs them. The options passed over
   were a second Truto connection through `googlecontacts` (a second
   sign-in and fee) and a custom Truto resource pointing at
   `people.googleapis.com`.
