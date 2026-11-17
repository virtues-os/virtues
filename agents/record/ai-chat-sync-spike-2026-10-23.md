# AI chat sync spike, 2026-10-23

Whether the box can keep a live copy of the owner's chats on claude.ai,
ChatGPT and Grok the way `x_bookmarks_sync` keeps X bookmarks: replay the web
client's own requests with the owner's session cookies on a schedule. Today
the only path is `applets/chat_import`, which takes an export file once.

Method: each site's internal API was mapped from inside a signed-in page in a
real browser, recording only field names, paging and auth. Then the same
endpoints were called with curl, without cookies, from the Mac and from a box,
to see whether a non-browser client reaches the API at all.

## Result

| Site | A plain HTTP client gets | Can the box replay it? |
|---|---|---|
| claude.ai | `403` with `cf-mitigated: challenge` (Cloudflare's JS challenge page) | No |
| chatgpt.com | `403` with `cf-mitigated: challenge`, on `/backend-api/*` and on `/api/auth/session` | No |
| grok.com | `401` JSON: `No credentials presented` | Yes, as far as tested |

The challenge was the same from the Mac and from the box, so it is not about a
residential or datacenter IP. Cloudflare serves the challenge before the
request reaches the site, and it does so without looking at cookies. Getting
past it from a script means imitating a browser to defeat bot detection. We
do not build that.

Grok's API answered with an ordinary auth error, which means the request
reached the origin. A replay with real session cookies was not run, so the
cookie names and their lifetime are still unverified.

## A real browser on the box

The same day, a headed Chrome ran on the main Dragon box to see whether a real
browser on the server can read claude.ai and ChatGPT where curl could not.

Setup, all in a scratch directory under the `radxa` home, with nothing
installed system-wide:

- Chrome for Testing 156 for linux-arm64, from Google's
  `chrome-for-testing-public` bucket.
- `Xtigervnc` (a virtual X display with a VNC server built in) and
  `X0tigervnc`, unpacked from Ubuntu's own `.deb` files with
  `apt-get download` and `dpkg -x`. Every library both need was already on the
  box image.
- Each ran as a `systemd-run --user` unit: Chrome with `MemoryMax=1500M`,
  `MemorySwapMax=0` and `CPUQuota=300%`. DevTools and VNC listened on loopback
  only and were reached over an SSH tunnel.
- The owner signed in to both sites through macOS Screen Sharing. Chrome kept
  its own user agent; nothing was overridden or hidden.

Results:

| | claude.ai | ChatGPT |
|---|---|---|
| Cloudflare challenge on page load | none | none |
| Challenge on the API calls | none | none |
| List the chats | 0.4 s, one page | 0.9 s, one page, plus the archived list |
| Fetch one chat | about 0.2 s | about 0.4 s |
| Failed chat fetches | 0 | 0 |

The calls were the same-origin requests the sites' own pages make, run inside
the signed-in tab with `Runtime.evaluate` over DevTools. Only counts and
timings left the box.

Memory is the constraint. Chrome peaked at 1.16 GB during a Google sign-in
with one site open, and at 1.52 GB with both sites open after the ChatGPT
calls, close to the 1.5 GB cap. The Dragon has 5 GB in all. A sync should open
one site at a time and close the tab afterward.

Traps found on the way:

- **The user units stop when the last SSH session closes.** Lingering is off
  for `radxa` on the box, so the first start died 12 seconds after the
  `ssh` command that launched it returned. The spike kept a tunnel open to
  hold a session. A shipped version runs as a system service under
  `virtues`.
- **macOS Screen Sharing will not connect to a VNC server with security type
  None.** It connects and drops without a prompt. `Xtigervnc` cannot switch to
  password auth while running, so a second server (`X0tigervnc`) was attached
  to the same display with `-SecurityTypes VncAuth`.
- **Keys repeat without end through `X0tigervnc`**, and Shift was unreliable.
  `xset r off` on the display stopped the repeats. Email-code sign-in avoids
  typing a password at all.

The login cookies in the profile, read by name and expiry only:

| Site | Login cookie | Expires after |
|---|---|---|
| claude.ai | `sessionKey` | 28 days |
| ChatGPT | `__Secure-next-auth.session-token` (split into `.0` and `.1`) | 90 days |

Both are persistent cookies, so a Chrome restart keeps them. Cloudflare's
`cf_clearance` lasts a year on both sites, and `__cf_bm` lasts 30 minutes and
is reissued on each visit. Not measured yet: whether either site extends the
login expiry when it is used, and whether either site ends a session early
for other reasons.

## What this means for the design

- **Grok** fits the X pattern: a cron applet on the box, per credential, with
  the cookie jar handed over by the Mac app's source login (see
  [browser.md](browser.md)).
- **claude.ai and ChatGPT** can only be read from inside a real browser
  session. Two places can host it: a headed Chrome on the box (measured
  above), which keeps syncing while the Mac sleeps, or the Mac app's
  WKWebView, which runs only while the Mac app is open and may be throttled
  when hidden (also in browser.md). The export file through `chat_import`
  remains the way to load the full history once.
- The owner can sign in on the box through a live view, or the Mac app can
  hand over each site's login cookies the way it does for X, and the box
  browser loads them with `Network.setCookies`. Whether a login moved from
  the Mac to the box survives is not tested.
- All three list endpoints sort by last update, newest first. A sync can walk
  pages until it reaches a chat whose update time it already has, then fetch
  only those chats. Messages are keyed by the site's message id, so
  `chat_import`'s insert-or-ignore dedup applies as is.

## API shapes

### claude.ai

- Auth is the `sessionKey` cookie, which is `httpOnly`.
- `GET /api/organizations` lists organizations. The one whose `capabilities`
  includes `chat` holds the chats; an account can also have an API-only org.
- `GET /api/organizations/{org}/chat_conversations?limit=&offset=` lists
  chats sorted by `updated_at`, newest first. Each item has `uuid`, `name`,
  `summary`, `model`, `platform`, `project_uuid`, `is_starred`,
  `is_archived`, `is_temporary`, `created_at`, `updated_at` and
  `current_leaf_message_uuid`. No paging headers come back.
- `GET /api/organizations/{org}/chat_conversations/{uuid}?tree=True&rendering_mode=messages`
  returns `chat_messages[]`. Each message has `uuid`, `parent_message_uuid`,
  `index`, `sender` (`human` or `assistant`), `created_at`, `content[]`
  (typed blocks, `text` among them), `attachments` and `files`.
- Edits and retries make branches. To keep the version the owner sees, walk
  `parent_message_uuid` back from `current_leaf_message_uuid`.
- Not exercised: paging past one page, and content block types other than
  `text` (thinking and tool use), because the test account had too few chats.

### ChatGPT

- `GET /api/auth/session` (cookie auth) returns a JSON session with a bearer
  `accessToken` that lasts 240 hours. Every `/backend-api/*` call wants
  `Authorization: Bearer <accessToken>`.
- **A missing bearer returns `200` with an empty list**, not an error. A sync
  has to treat an empty first page with no prior sync as suspect, the same
  trap as X's stale query id.
- `GET /backend-api/conversations?offset=&limit=&order=updated` returns
  `{items, total, limit, offset}`, sorted by `update_time`. `limit=100` worked.
  `total` did not match the number of items paged through, so page until a
  short page instead of trusting it.
- Archived chats are left out of that list. `&is_archived=true` returns them.
  Project chats are also listed separately, under
  `GET /backend-api/gizmos/snorlax/sidebar`; the test account had none, so
  their shape is unverified.
- `GET /backend-api/conversation/{id}` returns a `mapping` of nodes (`id`,
  `parent`, `children`, `message`) and `current_node`. Walk `parent` back from
  `current_node` for the version the owner sees.
- `message.author.role` is `user`, `assistant`, `tool` or `system`.
  `content.content_type` was `text`, `multimodal_text`, `code`, `thoughts` or
  `reasoning_recap`. Keep `user` and `assistant` text. Drop `tool`, `code`
  and `thoughts`, per the keep-list below.
- Times are Unix seconds (`create_time`, `update_time`). claude.ai and Grok
  use ISO strings.

### Grok

- Auth is cookie-based. A request with no cookies gets a `401` JSON error and
  no bot challenge.
- `GET /rest/app-chat/conversations?pageSize=` returns
  `{conversations, nextPageToken, textSearchMatches}`. Each conversation has
  `conversationId`, `title`, `createTime`, `modifyTime`, `starred`,
  `temporary` and `kind`.
- `GET /rest/app-chat/conversations/{id}/response-node?includeThreads=true`
  returns `responseNodes[]` with `responseId`, `parentResponseId` and
  `sender`.
- `POST /rest/app-chat/conversations/{id}/load-responses` with
  `{"responseIds": [...]}` returns `responses[]`. Each response has 41 fields,
  most empty for plain text. The useful ones are `responseId`,
  `parentResponseId`, `sender` (`human` or `assistant`), `message` and
  `createTime`.
- Not exercised: the `pageToken` parameter name for the next page, and the
  sort order of the list.

**Built 2026-10-26 as `applets/grok_chats_sync`, without a live run.** The
owner chose to skip the live test. These parts are assumed, not observed, until
the first real sync:

- The login cookies are `sso` and `sso-rw` (`applets/sources.toml`). If they
  are wrong, the Mac pane's sign-in never completes, or the first sync gets a
  `401`.
- The next page is requested with `pageToken`. If the server ignores it and
  returns the first page again, the run fails with "the same page twice".
- The list is sorted by `modifyTime`, newest first. If it is not, the walk
  fails the run.
- The Mac pane hands grok.com's cookies over the way it does for Instagram
  (`jar_field = "cookies"`).

Each of these fails loudly, never as "no new chats": a `401` or `403` marks
the credential `reauth_required` and attention asks the owner to sign in
again, an empty first page fails the run, and a body without the expected
array fails the run. The applet writes through
`virtues_helpers::conversations`, which `chat_import` now shares, so an export
import and the live sync never write a message twice.

## What to keep

For these chat sites, keep the full text of the owner's turns and the
assistant's final replies, plus each chat's title, model and timestamps. Drop
reasoning, tool calls, tool output and code-interpreter blocks. That is the
same rule proposed for local coding-agent transcripts on the same day, where
the dropped kinds made up about 95% of the bytes.

`data_content_conversation` has no column for the title or the model today
(they were dropped with `metadata` in a 2026-08-28 cleanup), so the Grok sync
stores the turns and their times only. Keeping titles needs a migration.

## Open

- Run `grok_chats_sync` against a real account, and record the cookie names,
  how long they last, and whether the paging and sort assumptions above held.
- Restart the box browser on the same profile a day later and record whether
  both sites are still signed in.
- Page through a large account from the box browser, and measure memory
  while doing it.
- Confirm ChatGPT project chats and claude.ai paging on an account that has
  them.
