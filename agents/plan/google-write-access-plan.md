# Google write access and Google's security assessment

**Written 2026-10-30. Nothing built.** Let the assistant act on the owner's
Gmail and Google Calendar: archive and label mail, write drafts, and create or
change events. To do that, our Google app asks for broader scopes and passes
Google's yearly security assessment (CASA) itself.

## Decided

These are the owner's calls from 2026-10-30:

- "Bite the bullet and get the assessment done and modify permissions in
  gmail and calendar." Virtues passes the assessment with its own Google app.
  No shared app from a vendor (Nylas, Composio, Pipedream) sits between the
  owner and their mail.
- "Just google for now." Outlook comes later, through Microsoft Graph and
  Microsoft's publisher verification, and is not in this plan.
- Drafting is wanted. It comes from `gmail.modify`; see "Scopes".

## Why we cannot skip the assessment

We looked for a way out and there is none for the app as it is built.

- **Google's rule covers any server.** The Gmail scopes page says "If you store
  restricted scope data on servers (or transmit), then you must go through a
  security assessment." The local-client exemption that older FAQs mentioned
  is gone from Google's current pages.
- **Local apps were made to do it anyway.** MailMate, a Mac client that keeps
  mail on the Mac, argued in 2022 that it could not access data through a
  server. Google still required the assessment by June 2024. Mimestream does
  it every year.
- **Our server can reach the mail.** The OAuth proxy redeems the code and runs
  every refresh with our client secret (`services/virtues-api/src/routes/oauth.rs`).
  Model calls go box → `api.virtues.com` → Vercel AI Gateway → provider
  (`routes/ai.rs`), and chat and suggestions put email subjects and previews in
  those calls.
- **We already need it.** The app asks for `gmail.readonly` and
  `drive.readonly` today, and both are restricted.

The assessment costs less than its reputation. MailMate's developer posted
Tier 2 quotes of about $540 to $1,800 a year at Google's negotiated rate, and
$1,800 to $6,300 without it.

MCP does not change any of this. Google reviews the OAuth app that asks for the
scopes, so a Virtues client of Google's own Gmail MCP server would need the
same assessment. MCP stays the way in for services that let the box register
itself (`agents/plan/mcp-plan.md`).

## Scopes

| Today | After | Google's class | Why |
|---|---|---|---|
| `gmail.readonly` | `gmail.modify` | Restricted | Google's table: "Read, compose, and send emails". That covers archiving, labels and drafts, so we do not also ask for `gmail.compose`. It cannot permanently delete. It does allow sending; we choose not to build a send tool (see "Tools"). |
| `calendar.readonly` | `calendar.readonly` and `calendar.events` | Sensitive | `calendar.events` covers events only. The sync walks `users/me/calendarList` and the account lookup reads `calendars/primary`, and neither is covered by it, so `calendar.readonly` stays. Sensitive scopes need ordinary review, not the assessment. |
| `drive.readonly` | dropped | Restricted | No applet syncs Drive. The only use is the account lookup in `virtues-core/src/api/source_auth.rs` (`account_lookup::GOOGLE`), which Gmail's profile and the primary calendar already cover. Google asks for a feature behind every restricted scope. |

Checked against Google's Gmail and Calendar scope pages on 2026-10-31.

## What exists today

- **Sign-in.** The proxy's Google entry asks for the three scopes above
  (`oauth.rs`, `provider_cfg`). The box pulls the grant through `/exchange` and
  refreshes it through `/refresh`.
- **Mail sync.** `applets/google_gmail_sync` lists message ids since a cursor
  and stores each message once in `data_communication_email`, labels included
  as they were at fetch time. Nothing updates a row after that, so a message
  archived in Gmail still looks like it is in the inbox on the box.
- **Calendar sync.** `applets/google_calendar_sync` keeps `data_calendar_event`
  current with a sync token.
- **Data triggers.** `trigger: new <table>` wakes an applet on new rows
  (`applets/AUTHORING.md`, `virtues-core/src/scheduler/watch.rs`).
- **Undo window.** `virtues-core/src/outbox.rs` holds an iMessage send for
  `UNDO_WINDOW` (5 seconds) so the owner can cancel it.
- **Morning brief.** `applets/morning-brief` already has a "Waiting on you"
  section for messages and emails that need a reply.

## Slices

Slice 0 is done. Slice 1 starts now. Slices 2 to 5 can be built in parallel. Slices 6
and 7 come last, and then everything goes to Google in one submission.

### 0. Where our app stands

Read in Google Auth Platform on 2026-10-31. The project is "Virtues", and its
one web client redirects to `https://auth.virtues.com/google/callback`, so it is
the proxy's client.

- **Unverified, in production, external.** Users see Google's "unverified
  app" screen. The app was submitted once and not approved.
- **Branding is verified and published (2026-10-31).** An earlier pass
  lapsed because it was not published within 7 days. The consent screen now
  shows the Virtues name and logo, but users still see Google's "unverified
  app" warning until data access is approved.
- **7 of the 100-user cap are used.** Google counts the cap over the project's
  lifetime and never resets it.
- **Data Access declares only `calendar.readonly`**, with no justification
  and no video. `gmail.readonly` and `drive.readonly` are requested in
  `oauth.rs` but not declared, which is why every Gmail sign-in counts against
  the cap.
- **The production client also lists `localhost` redirect URIs** (ports 5173,
  8000 and 9000). Move them to a separate development client before
  submitting.
- **The privacy policy describes Calendar read-only access only.** It does
  not mention Gmail, and it names hosting and model providers that may be
  out of date. Its source is `src/legal/google/privacy.md` and
  `src/legal/website/privacy.md` in the website repo. Slice 6 rewrites it.

### 1. Harden the proxy

An assessor tests the servers that handle Google data, which here means
`virtues-api`. The two proxy items already listed in
[account-plan.md](account-plan.md#oauth-proxy-and-entitlement) come first:
require `X-Virtues-Api-Key` on `/refresh`, then on the other routes, and bind
an OAuth session to the box that started it. Then review the rest of
`virtues-api` for what a Tier 2 scan looks for.

Progress:

- **No box refreshed without a key from 2026-10-26 to 2026-11-01.** The
  deployed build (`032c03a12`) logs a keyless refresh at info, production
  keeps `virtues_api` info lines, and the log held none.
- **`/refresh` requires a known box key** (written 2026-11-02, not deployed).
  It answers a missing or unknown key with a 401 whose `code` is
  `missing_key` or `unknown_key`. A box from current code reads `code` and
  does not ask its owner to reconnect for those. Released boxes read any 4xx
  as a refused grant, so a box that somehow refreshes without a key would
  show its owner a reconnect prompt that does not help.
- **Box keys outlive wallets** (`bbe20e8e1`, 2026-11-02, not deployed). The
  sweeper used to delete a lapsed wallet's keys, so every free owner lost
  theirs 37-68 days after linking, and the `/refresh` check above would have
  stopped their Google sync. `/refresh` now needs only a known key, not a
  wallet. Production count on 2026-11-02: no key lost yet; one free owner's
  wallet expired 11-01 and its key is swept around 11-08. **Deploy before
  2026-11-08.**
- **Failed authentication is logged, ordinary use is not** (written
  2026-11-02, not deployed). `/refresh`, `/exchange`, the bearer routes and
  the internal routes each log a refusal at warn with the reason only: no
  key, no account, no token, no IP. A successful call leaves no log line.
  Every request span carries the method and route pattern, never the URI,
  because `/callback`'s query holds the OAuth code.
- **Three requests fail with a 502 every 15 minutes**, at :00, :15, :30 and
  :45, in under 130 ms, without a break since 2026-10-27. The route was not
  logged. After the next deploy, the 502 lines name their route.
- **Not done:** require the key on `/exchange` (check first whether setup
  can connect Google before the box is linked), bind an OAuth session to its
  box (needs a box release), delete expired `oauth_session` rows on a timer
  instead of only when the next sign-in starts, and review the rest of
  `virtues-api` against the assessment's requirements.

### 2. Scopes and re-consent

- Change the Google entry in `oauth.rs` to the "After" column.
- Drop the Drive lookup from `account_lookup::GOOGLE`.
- Owners who connected with the old scopes keep working. Ask them to grant the
  new ones with Google's incremental consent (`include_granted_scopes=true`),
  from the Google source's page, worded around what they get ("Let Virtues
  archive mail and write drafts").
- A tool that needs a scope the grant lacks says which permission is missing
  and where to grant it.

### 3. Keep mail state current

Triage needs to know whether a message is still in the inbox. Read Gmail's
history (`users.history.list` from a stored `historyId`) on each sync tick and
update `labels` on rows that changed. When our own tool archives a message, it
updates the row at the same time.

### 4. Tools

| Tool | What it does | Undo |
|---|---|---|
| `email_archive` | Removes the `INBOX` label from a thread | `email_unarchive` puts it back. Every archive is logged with the reason. |
| `email_label` | Adds or removes a user label | The reverse call |
| `email_draft` | Creates a Gmail draft, or replaces one the assistant wrote | The owner deletes the draft in Gmail |
| `calendar_event` | Creates, changes or deletes an event | An undo window like `outbox.rs` before the call goes to Google |

There is no send tool. The owner sends from Gmail. That keeps a
prompt-injected email from getting the box to send mail as the owner. Google's
own Gmail MCP server makes the same choice.

Names are not settled. Check `crates/virtues-registry/src/tools.rs` for the
naming that is there before adding these.

### 5. The triage applet

- `trigger: new communication_email`.
- It sorts each new message as needs you, for your information, or junk. This
  is the model's judgment; there are no keyword rules.
- `allowed-tools` holds only `email_archive` and `email_label`. It gets no web
  search, no browsing and no sending, so an email that carries instructions can
  at worst archive mail, and every archive can be undone.
- It starts by suggesting. The owner approves archives, and once they have
  approved a sender's mail a few times, the applet archives that sender's mail
  on its own.
- What needs the owner goes into the morning brief's "Waiting on you", written
  as sentences: who wrote, what they want, and by when.
- It shows no unread counts, no badge and nothing marked overdue.

### 6. Disclosure

- The privacy policy names the model path (box → `api.virtues.com` → Vercel AI
  Gateway → provider) and says no one trains models on the data. Get that in
  writing from Vercel and each provider in the slots first.
- The status label from [watch-it-work-plan.md](watch-it-work-plan.md) shows
  when an email went to a model, and which one.

### 7. Submission (owner, with help)

- A demo video on the owner's own account that shows triage, an archive and its
  undo, a draft, and a calendar event, one per scope.
- Justification text for each scope, matched to the video.
- The self-assessment answer on logging. OWASP ASVS 7.1.3, which the
  assessment draws on, asks that successful and failed authentication be
  logged. We log every failure and no successes, and say why: "Virtues keeps
  no record of how an owner uses their box. The servers log every refused
  key, token or secret, with the time, the route and the reason, which is
  enough to see and investigate an attack. A successful call is a box using
  its own key, and logging it would build a per-owner activity history that
  the product promises not to keep." If the assessor still requires
  successes, the fallback is an hourly count with no account, time of day or
  route.
- A Tier 2 assessor from Google's list, at the negotiated rate. Book it once
  slices 1 to 6 are done.

## What each persona gets

- **Narrative seeker:** important mail arrives as sentences in the brief, and a
  draft reply that knows who the person is and what you last talked about.
- **Quantified self:** the same sorting and archiving. Numbers about mail come
  from asking chat, which can query the synced mail.
- **Tech prosumer:** an inbox that sorts itself and drafts in your voice, on
  your own box, after one "Sign in with Google".
- **Recovery:** no counts, no streaks, no "you haven't replied" reminders. The
  brief says what needs you and nothing else.
- **Data sovereign:** mail goes from Google to the box with no third party in
  between. The status label shows each time an email goes to a model.

## Open questions

- Should calendar writes ship first in their own submission? They need only
  ordinary review, but a second submission repeats the paperwork.
- Should triage run on a local model for owners who want no mail to leave the
  box? The private-mode spike would decide it.
- Ask Google's review team in writing whether a box the owner runs counts as a
  server? Several 2026 threads on Google's developer forum ask this and none is
  answered, so nothing here waits on it.

## Sources

- [Google: Gmail API scopes](https://developers.google.com/workspace/gmail/api/auth/scopes)
- [Google: Restricted scope verification](https://developers.google.com/identity/protocols/oauth2/production-readiness/restricted-scope-verification)
- [Google Cloud verification FAQ](https://support.google.com/cloud/answer/13463817)
- [MailMate list, May 2022](https://lists.freron.com/mailmate/2022-May/015462.html),
  [June 2024](https://lists.freron.com/mailmate/2024-June/017271.html),
  [April 2025](https://lists.freron.com/mailmate/2025-April/018273.html)
- [Mimestream: CASA verified](https://mimestream.com/blog/casa-verified)
- [Nylas Google Shared App](https://developer.nylas.com/docs/provider-guides/google/shared-gcp-app/)
- [Composio managed Gmail auth](https://composio.dev/content/ship-gmail-integration-in-minutes)
