# Sign-ins: keeping a site session alive

**Status:** Proposed 2026-11-19. Nothing here is decided by the owner except
the three earlier decisions listed under "Already decided". Three parts of
slice 1 are built on wave and in no release as of 2026-11-19: the Instagram
sync reports a dead session as one, a dead sign-in is one attention item, and
a sync replays a session with the user agent of the browser that made it.
None has run against a live site, on a server or in a built app.

Some syncs have no API to use, so they act as the owner's browser. The owner
signs in once in the Mac app, the app hands the session's cookies to the
server, and the sync uses them. This plan covers what happens after that
hand-over: keeping the session fresh, saying clearly when it ends, and signing
in again with as little work for the owner as possible.

## What exists today

- **Three syncs replay a session with a plain HTTP client:** X, Instagram and
  Grok (`applets/sources.toml`, the `login` entries). They need the cookies and
  nothing else. Grok's login cookie names are still unconfirmed (the TODO in
  that file).
- **Two sites answer only a real browser:** claude.ai and ChatGPT. A visible
  Chrome on a server, signed in by the owner, read both
  ([record/ai-chat-sync-spike-2026-10-23.md](../record/ai-chat-sync-spike-2026-10-23.md)).
  Memory peaked at 1.5 GB with both open. No sync is built on this.
- **The Mac app's Browser pane** has a persistent cookie store, and the
  assistant can drive it ([record/browser.md](../record/browser.md),
  [record/webkit-agent-spike.md](../record/webkit-agent-spike.md)). `site_jar`
  in `apps/web/src-tauri/src/browser.rs` reads a site's cookies out of it.
  Other desktops sign in through an incognito window that keeps nothing.
- **The hand-over happens once, at connect time.** The server records when the
  session's login cookies expire (`credentials::jar`) and warns three days
  before. A sign-in made before the whole-jar hand-over existed has no expiry
  recorded, so it gets no warning.
- **OAuth sources refresh themselves.** The refresh sweep never touches a
  session sign-in.
- **Saved passwords** are in Drive → Passwords
  ([secrets-plan.md](secrets-plan.md)). No sync, tool or model can read them.

## What goes wrong

1. **Cookies a site sends back during a sync are dropped.** Sites reissue some
   cookies on each visit. The server keeps using the copy from connect time.
2. **Nothing renews a session.** When it ends, the only path is the owner
   noticing and signing in again by hand.

## The change

A sign-in belongs to a browser that keeps it alive. The Mac app's Browser pane
is the first such browser. A browser on the server is the second. The syncs do
not change with where the cookies came from.

### Slice 1: keep the cookies a site reissues

Built already: a session sync that meets a login redirect marks its sign-in
for reconnect (`virtues_applets::http_client_no_redirects()`, used by the
Instagram sync), and a sync whose sign-in needs the owner is left out of the
failed-applet items, so the sign-in's own item is the only one. The app's
sign-in also sends the user agent of its browser (`browser_login_user_agent`,
shell surface 17), and the X, Instagram and Grok syncs replay the session with
it (`virtues_applets::session_user_agent`). A sign-in made before that, or
pasted by the owner, keeps the syncs' default until the owner signs in again.

Left:

- **The sync keeps cookies the site reissues.** A sync applet is not given the
  server's encryption key, so it cannot write the stored sign-in. The applet
  would return the reissued cookies in its output and the runner would merge
  them, guarding against the owner signing in again during the run. Which
  cookies each site reissues is not measured; measure that on a live session
  before building this.
- Sign-ins with no expiry recorded get one the next time the Mac app re-sends
  the cookies (slice 2).
- Check X and Grok for a login redirect the way Instagram was checked. Both
  handle a 401 or 403 today.

### Slice 2: the Mac app re-sends cookies

While the app is open it sends each connected site's cookies to the server at
launch, once a day, and when the server marks a sign-in for reconnect. If the
Mac's copy is still signed in, the sync recovers and the owner sees nothing.

### Slice 3: signing in again from the notification

The attention item opens the site's login page in the Browser pane. For a site
where the owner has allowed it, the app fills the saved password and the
authenticator code, and hands the page to the owner for anything else. The
shell fills these fields itself and a model never sees the values. Today the
shell refuses to let a model type into password and one-time-code fields.
Filling a saved login in the Mac pane is the unbuilt half of slice 6 in
[secrets-plan.md](secrets-plan.md), and this slice depends on it.

### Slice 4: a browser on the server

A visible Chrome, started when a sync needs it and stopped after, with a
memory cap. It serves the sites that cannot be replayed, and it lets a server
sign in again while the Mac is asleep or when the owner has no Mac.

- Signing in uses the saved password and an authenticator code generated on
  the server, for sites the owner has allowed.
- When a site asks for an SMS or email code, the server first looks for it in
  the owner's mail and messages with an immediate fetch. Scheduled syncs deliver mail and
  messages several minutes after they arrive, which is too slow for most
  codes. If it cannot find the code, the app asks the owner to type it.
- When a site asks for a tap on the owner's phone, or a passkey, the app says
  so and waits.
- A failed sign-in is not retried on a schedule. Repeated automated failures
  can lock an account.

If the server runs on the owner's Mac, slices 2 and 3 cover most of this.

## Already decided

- No TLS impersonation, stealth patches, hidden automation, challenge solving
  or moving a Cloudflare clearance cookie between browsers (2026-10-23).
- No browser extension for collecting sessions (2026-10-23).
- The server's browser is never streamed to the owner as their own browser
  (2026-10-07, [record/browser-spike.md](../record/browser-spike.md)).

## Open decisions

1. Whether a sync may ever read a saved password, and if so whether the
   permission is per site. Today nothing can.
2. Whether the server may store an authenticator secret beside the password.
   A server holding both holds every factor for that account.
3. Whether slice 4 is built before the server runs on Mac and Windows, or
   waits for it.
4. Which sites get a server browser sync first. The 2026-10-23 record points
   at claude.ai and ChatGPT.

## Tests to run before slice 4

Both are listed as untested in the 2026-10-23 record.

- **Does a session moved from the Mac to a server Chrome survive?** This
  decides whether the server browser can borrow the Mac's sessions or must
  always sign in itself.
- **Does a site extend a session when it is used?** claude.ai's login cookie
  was issued for 28 days and ChatGPT's for 90. If use extends them, a running
  sync keeps itself signed in.
- A real sign-in on the server through a challenge and a second factor, on an
  account the owner agrees to use.

## Choosing how a new source syncs

In order of preference: an official API, an export file for the first load,
sharing into Virtues from the phone ([bookmarks-plan.md](bookmarks-plan.md),
item 1), session replay, the server browser. The last two break when the site
changes and put an account at some risk, so a source uses them only when the
first three do not exist. Check a provider's terms for AI use before adding
any source.

## What each persona gets

Personas are in [build/personas.md](../build/personas.md).

- **Narrative seeker and quantified self:** saves and chats keep arriving. At
  most one clear request when a site needs them.
- **Tech prosumer:** a server that signs itself back in.
- **Recovery:** one notice for a dead sign-in. No repeated alerts and no
  counts.
- **Data sovereign:** passwords and codes stay on their server, each site is
  their choice, and the manual says what the server stores and what it sends
  to each site.
