# Publishing: virtues introduces, the box serves

> **STATUS 2026-10-06: wave 1 core sharing built; the loader is live at `s.virtues.ch`.** A page is
> served **by the box**. A visitor's browser reaches it as an iroh endpoint
> (iroh compiled to WebAssembly) through the relay we already run, encrypted
> end to end; virtues introduces the two and, once a direct transport for
> browsers lands, drops out of the path. The publication primitive, the face
> as the medium, and the doctrine carry over from the 09-01 plan.
> `publish_to_github` is built and becomes an optional destination, not the
> front door.

## The ask

Anyone, not just someone who runs a website: make a page in virtues (a trip
for two, an invitation, a plan) and send a link that opens on any phone.

The test every design below has to pass:

> A person with a virtues box and nothing else (no GitHub, no domain, no idea
> what a token is) gets a working link in one sheet, and no company holds or
> can read what they shared.

Every "bring your own" option fails the first half. Every hosted option
(including an encrypted one) fails the second, or makes virtues.com the place
the content lives, which turns a home server into a client of ours.

## What exists today (verified 2026-10-05)

| | |
|---|---|
| Page shares | `app_page_shares (id, page_id, token, created_at)`, `UNIQUE(page_id)`, UUIDv4 token (`0001_initial.sql`). `GET /api/s/:token` and `/api/s/:token/files/:file_id` are unauthenticated by design. `(public)/s/[token]/+page.svelte` renders through `PublicPageViewer`. |
| The origin bug | `PageContent.svelte:562` and `:585` build the share URL from `window.location.origin`: `http://localhost:7117` on a Mac. Nobody else can load it. |
| Listeners | `server/mod.rs`: plain HTTP on :8000 is the only listener; the box has no TLS surface. Off-LAN reach is iroh, allowlisted by EndpointId; a browser has no EndpointId, so **iroh cannot carry a browser.** |
| Faces | `face/index.html` in a sandboxed iframe, read-only `virtues.query`, 48KB cap (`applet_setup.rs` `FACE_HTML_MAX`). |
| `publish_to_github` | **Built, wave `322a7a8f`, unreleased, never run against GitHub.** Writes an applet's face as one `.html` file into a repo via the contents API, with a pasted fine-grained token (`github_publish` source). Grant hashes repo, branch, path and bytes; refuses faces that need the box; chat-only. `applets/AGENTS.md` tells faces meant for sharing to stand alone (`7ff23663`). |
| Print, pagination, export | None. Zero `@media print` in `apps/web`; "Copy as Markdown" is the only export. |
| Guests | None. `app_auth_user` plus pairing means *your devices*. |

## Prior art

### Home servers converged on one shape

| Product | Remote reach | Sharing |
|---|---|---|
| Home Assistant | **Nabu Casa Remote UI**: a relay forwards TLS, the certificate lives on the HA instance, Nabu Casa cannot read it. A subscription, and it funds the open-source project. DIY: DuckDNS plus a port forward. | Guest accounts, not public pages. |
| Synology | **QuickConnect**: direct when possible, Synology relay otherwise. Or a `synology.me` DDNS name, port forward, Let's Encrypt. | Share links with password and expiry, served by the NAS; dead while it is off. |
| Plex | Direct, falling back to a bandwidth-capped Plex relay. Per-server certs for `*.<hash>.plex.direct`, so even LAN traffic is HTTPS. | Friends stream from the owner's server, not from Plex. |
| Nextcloud | DIY port forward or Cloudflare Tunnel, which terminates TLS at Cloudflare (it reads the traffic). | Public links with password and expiry; federated shares between instances. |
| Umbrel, Start9 | Tor onion services by default; Tailscale as an app. | Links that only open in Tor Browser. |
| Tailscale Funnel | Relay forwards TLS to the node, which holds the cert. | A public URL per node. |
| ngrok | Relay terminates TLS. | Free tier shows an interstitial, because free tunnels became a phishing host. |

What the comps settle:

1. **Relay-forwards, device-holds-the-cert is the mainstream answer**, run at
   consumer scale by Home Assistant, Synology and Plex. It is not exotic.
2. **Nabu Casa is the business model already proven:** open core, a paid
   relay that cannot read, the subscription funds the project.
3. **"The server must be on" is accepted.** Synology and Plex links die with
   the server and nobody treats that as a defect.
4. **Everyone who shares uses links with expiry, served by the device.** None
   uploads the content to the vendor.
5. **The two extremes fail the test above.** Tor fails "opens on any phone";
   a terminating tunnel (Cloudflare, ngrok) fails "no company can read it".
6. **The phishing tax is real** (ngrok's interstitial). A shared domain needs
   a reputation plan from day one.

### iroh already reaches browsers

Checked 2026-10-05 against the iroh docs and the n0 issue tracker:

- **iroh runs in the browser**, compiled to WebAssembly
  (docs.iroh.computer, "WebAssembly and Browsers"). A browser cannot send
  UDP, so a browser endpoint is **relay-only**: it talks to the relay over
  WebSocket or WebTransport, dials by EndpointId, and the connection is
  end-to-end encrypted, so the relay forwards bytes it cannot read.
- **Direct paths for browsers are coming through custom transports**
  (iroh 0.97+; we run iroh 1.x). n0 said it wants a WebRTC transport and
  would write one itself if nobody else did; two external crates exist
  (`iroh-webrtc-transport`, native↔browser over ICE, signaling over iroh
  itself), experimental as of August 2026. WebTransport with certificate
  hashes is the other route n0 names.
- **Protocols beside the transport:** `iroh-blobs` (BLAKE3 content-addressed,
  verifiable transfer), `iroh-gossip` (topic broadcast), `iroh-docs`
  (CRDT key-value documents built on both). `iroh-docs` is the candidate for
  box-to-box collaboration later.

This is the introducer model already shipped as a library: dial a key, the
relay introduces and carries until a direct path exists, nobody in the path
can read. It also means virtues keeps **one** networking idea: owner devices,
other boxes and strangers' browsers are all iroh endpoints.

### We built the relay once already

`48abf48e` built a blind L4 SNI-passthrough relay, a box client, per-box ACME
and TLS hot-swap, so any browser could reach the box. `e56f0963` replaced it
with iroh, because the owner's own apps hold keys and iroh gives them direct
and hole-punched paths with no CA in the loop. That was right for **owner
reach** and stays right. Publishing is a different reader, a stranger with a
browser, and for that reader the retired design is the only one that works.

What the archive learned, which binds the Funnel option below
([networking-relay-tee.md](../archive/networking-relay-tee.md),
[relay-control-plane.md](../archive/relay-control-plane.md)):

- **The CA rate limit is a launch gate.** Let's Encrypt caps new
  certificates per registered domain per week, and every box's name shares
  that bucket. Plex buys from a commercial CA for this reason. Apply for a
  rate-limit override early; plan a second CA.
- **Whoever runs DNS can mint a cert for a box's name.** virtues could
  impersonate a box. The answer is detectability, not denial: CAA records
  bound to our ACME account (RFC 8657), plus Certificate Transparency
  monitoring wired as a real alert. Owners who want no such trust use their
  own domain.
- **The privacy hardening was never built** (RAM-only relay, blinded tokens,
  audits). Do not describe the relay as anything more than "forwards bytes it
  cannot decrypt, and sees which box and when."

## Doctrine

> **The higher level may name, carry, and introduce. It may never hold or
> read.**

| Role | Who |
|---|---|
| Holds the page | the box |
| Reads it | the visitor's browser |
| Names it | the box's own key (its EndpointId, in the link); a domain only for the own-domain option |
| Introduces and carries it | the iroh relay, end to end encrypted, until a direct path exists |

> **If virtues.com vanished tomorrow, does the box still hold everything and
> still work for its owner?**

Yes, under this design: links stop resolving, nothing is lost. That is the
same answer Synology and Home Assistant give.

## The design

### Carry: an iroh endpoint in the visitor's browser

- **The link** comes in two forms (decided below), and both always work:
  `virtues.ch/<handle>#<token>` and the self-contained
  `s.virtues.ch/#<door-key>.<token>`. The part after `#` never leaves the
  browser. The door key **is** the door's public key (the door runs on the
  box with its own key, see wave 1), so dialing it authenticates the door: a
  server that does not hold that key cannot complete the handshake, virtues
  included. No certificate, no CA, no per-box DNS name.
- **The loader** is one static page: iroh compiled to WebAssembly plus a
  renderer. The same bytes for every box and every page, no content. It is
  the one thing virtues serves, so it stays small, open, version-pinned, and
  its hash is published.
- **The relay** is the `iroh-relay` already on OVH (`relay.virtues.ch`,
  open to any endpoint with rate limits since 2026-08-31). Browser traffic is
  relayed until a direct transport exists; the relay sees two EndpointIds and
  byte counts, never content.
- **On the box**, the door is a second iroh endpoint that accepts **any**
  EndpointId on `virtues/publish/1`. The core's endpoint is untouched:
  `crates/virtues-iroh/src/server.rs` still closes any peer not on the
  allowlist before a byte of HTTP, and `relay::maybe_spawn` still hands only
  allowlisted devices the full API.
- **Direct later, same link:** when a WebRTC (or WebTransport) custom
  transport is mature, the loader adds it and most visits hole-punch to the
  box. virtues then only introduces, which is the north star. Nothing about
  the link, the box or the door changes.
- **Handles** are the short form: atlas maps a box's handle to its door
  key, and nothing else. The token stays after `#`, so virtues never learns
  which page, and a short id that maps to the token itself is refused
  (it would make virtues hold the secret).

### Funnel: a public site on your own domain (later)

For pages that want link previews, search indexing and no JavaScript, the
Nabu Casa shape: an SNI router beside iroh-relay forwards TLS for a hostname
over the same publish ALPN, and the box holds a cert for it (DNS-01, key
never leaves the box). Own domain first, since then the owner runs the DNS
and virtues cannot mint a cert for the name. A shared virtues domain would
need the Public Suffix List, a CA with headroom, CAA plus CT monitoring, and
one hostname per box with pages at paths, as the archive above records.

### The door

A **separate process**, its own unix user, systemd-sandboxed
(`ProtectSystem=strict`, `NoNewPrivileges`, no network except the iroh
stream), **with no database credentials and no API**. It serves:

- published bundles, read-only, from a content-addressed directory the core
  writes;
- for a live page only, the queries that page declared at publish time, asked
  of the core over a local socket that accepts `(publication_id, query_id)`
  and nothing else.

A complete compromise of the door yields what the owner already chose to
publish. If a design ever needs the door to hold more, the design is wrong.

### What a publication is

> A **publication** is a self-contained artifact with a token, produced by
> any surface: a face, a page, a wiki entity, a project, a query result.

`app_publications (id, producer_kind, producer_id, token, title,
content_hash, rendered_at, expires_at, revoked_at)` plus a hit log. Replaces
`app_page_shares`, which has no expiry, no revocation record and no way to
answer "was this ever opened."

- **Frozen** (the default): rendered once to bytes, no queries, no
  `virtues.js`, no box URL. The same bytes print, download, and publish to
  any destination.
- **Live** (opt-in): the frozen shell plus a declared list of queries. The
  share sheet shows each one and the rows it returns today.

### The share sheet

The person publishes; the model never does. A **Share** button on the face or
page opens one sheet:

- the page as it will appear;
- **what leaves**: every image, every entity a link dereferences, every
  query and its rows, with anything private-looking (phone, address,
  confirmation code) flagged. A page that mentions `[@Nick]` must not quietly
  carry Nick's number. **This is the most important screen in the plan.**
- expiry (default 30 days), optional password, optional link-preview card;
- **Create link** → copy, and the phone's share sheet.

Afterwards: **Update link** when the source changes (same URL), and a list of
every link with opens and **Revoke**. Revoke deletes the bundle on the box;
there is no other copy.

### Guests writing back (later)

Off by default. A separate **edit link**; writes are size-capped and land in
that publication's own table, **marked as guest input**. The box's AI reads
guest rows as data, never as instructions: a stranger's text reaching the
agent is a prompt-injection path. No guest writes until that marking exists.

### Other destinations

All take the same frozen bundle:

- **Download HTML**: always, including the free open-source build.
- **GitHub**: `publish_to_github` exists, but a pasted fine-grained token is
  the wrong front door; nobody makes one. The right one is a **virtues GitHub
  App** connected by device flow (a code, approve, pick repos on GitHub's own
  screen), scoped to the chosen repos, no secret on the box and no proxy.
  Never the owner's ambient `gh` login: it covers every repo.
- **S3-compatible storage** (R2, B2, any bucket) for people who want a copy
  off their own box.

These live under **Publish to…**, a destination connector kind, not among the
data sources.

## What "answers strangers" costs

Today nothing reaches the box's HTTP code until iroh has checked the peer's
key. With a door, anyone on the internet can send bytes to code on the box.

| Exposure | Answer |
|---|---|
| Discovery | the introducer path has no hostname per box, nothing reaches CT logs; pages are unguessable tokens, uniform "not found" |
| A bug in the public code path | separate door process, no DB credentials, sandboxed |
| Floods on a home uplink and a Q6A CPU | per-box caps at the gateway; one-tap pause on the box |
| Guest writes as prompt injection | off by default; edit links; rows marked as guest input |
| One box's phishing tarring the loader domain | the loader shows content in a sandboxed frame under a banner naming the box; refuse a revoked EndpointId at the loader; unroute on abuse |
| Relay metadata (which box, when) | stated plainly; a direct transport removes the relay from most visits |
| Visitor IPs landing on the owner's box | not passed through by default; say so either way |
| Box off or asleep | the gateway's offline page; an encrypted fallback copy is a later option, never the default |
| Legal notices | land on virtues as the router; the response is unrouting, never reading |

## Rejected

- **virtues hosts the content in plaintext** (Docs, Notion, Claude
  artifacts). It makes virtues a company that reads people's pages and a
  moderation business.
- **virtues hosts encrypted blobs, key in the URL fragment** (Excalidraw,
  Proton). Blind, but the content lives on our servers, the box is not the
  server, and link previews die. It was the default of the previous draft.
- **A terminating tunnel** (Cloudflare Tunnel, ngrok). The vendor reads.
- **Tor only.** Fails "opens on any phone".
- **Our own WebRTC stack on the box** (`str0m` beside iroh). A second
  networking system for one feature; iroh's custom-transport route gives the
  same direct path inside the stack we already run.
- **Funnel as the default.** Per-box certs, a CA rate limit, names in CT
  logs and virtues able to mint a cert for a box's name, all to buy link
  previews. Kept for own-domain public sites only.
- **Port forwarding** as the path. Exposes the home IP and needs router
  skills; it stays possible for experts with their own domain.
- **The owner's ambient `gh` login on the box.** Every repo, held by a login
  the service user cannot see, and absent on every other box.

## The work, in waves

Each wave is usable on its own and builds on the last without redoing it.

### Wave 1: share anything, kept current

| Piece | What it takes | Difficulty |
|---|---|---|
| **Core sharing** | `app_publications`, the freezer, the door, the loader with its offline timeout, the Share sheet with "what leaves", update and revoke | Medium: the largest chunk, no unknowns left after the spike |
| **Live pages** | queries approved on the Share sheet, stored with the publication; the door asks the core for those and only those; the page keeps calling `virtues.query` as faces already do | Easy to medium: the review UI is the work, not the plumbing |
| **Preview cards** | dropped for now: one generic card for every link, so virtues never learns which link is opened | Done (generic) |
| **Box-to-box viewing** | another owner's app dials the door over native iroh and renders the page, no loader | Trivial |

**Built 2026-10-06:** core sharing (Share sheet, door supervisor, loader at
`s.virtues.ch`) and live pages. A page that reads data shares as a
**snapshot** (rows baked in, the server answers nothing) or **live** (the
page sends approved query keys through the loader and the door to the core,
which runs only those, read-only). The door reports opens, so counts are
real. **Preview cards: decided against for now (2026-10-06).** An unfurler
never sees the `#`, so a per-link card would put an identifier in the visible
path, and then `s.virtues.ch` would learn which link is opened. Every link
shows the same generic card instead.

They ship together because they share one rule: the door serves only what
the owner approved, whether a frozen file, a declared query or a card, and
the Share sheet shows all three before anything leaves.

**The door is its own iroh endpoint.** It holds its own key, homes on the
relay itself, and the link names *its* EndpointId, not the box's. Public
traffic therefore never reaches the core process: the core writes bundles
into a directory the door reads, and (for live pages) answers the door's
declared-query requests on a local socket. A compromise of the door cannot
speak as the box, because it never held the box's key.

Wire protocol on `virtues/publish/1`: one bi-stream per request, a JSON
request line (`{"op":"page","token":…}`, later `"query"` and `"card"`), a
one-line JSON status header, then the body.

### Wave 2: simple collaboration

Writes the page declares at publish time (a checklist tick, an RSVP, a vote,
a comment), approved on the Share sheet like queries in reverse, through an
**edit link**. Guest rows are marked as guest input so the box's AI reads
them as data, never as instructions; open viewers get updates pushed.
Medium.

### Wave 3: reach and speed

- **Own domain** (`trip.yourname.com`, no JavaScript, real previews and
  search): an SNI router beside the relay, a cert on the box via TLS-ALPN-01
  through the passthrough (the owner adds one DNS record), the door serving
  plain HTTPS. Medium to hard, mostly operational care.
- **Direct connections**: the WebRTC custom transport for iroh, once the
  external crates mature. Medium to hard; can wait.

### Wave 4: full co-editing and box-to-box sync

Two people editing one document live, and two boxes keeping a shared project
in sync (`iroh-docs`, or the page editor's existing CRDT). Hard. A privacy
fact the sheet must state: data synced to another box cannot be pulled back
by revoking.

### Also on the way

- **Fix the origin bug** so existing page shares work on the LAN meanwhile.
- **GitHub App and S3 destinations**; retire the pasted-token source.
- **Paged print** (`@page`, break control) rides on the freezer; PDF is the
  browser's print dialog, never a headless browser on the box.

### Spike results, 2026-10-05

A throwaway spike outside the repo, laptop Chromium: the spare Q6A behind
office NAT with client isolation loaded 5/5, first paint ~0.57 s (connect
~0.3 s, fetch ~0.15 s); a 1 MB page moved at ~7.5 Mbit/s through the relay.
iOS Safari (Simulator, iOS 26.5) loads it too, first paint ~0.73 s. The
loader is 0.95 MB gzipped before `wasm-opt`. A link to a key nobody holds
never connects. **A stopped box makes `connect` hang**: the relay does not
report an absent peer, so the loader owns a timeout and the "offline"
message. Still open: a real phone on cellular.

**The sandboxed door on a real box, 2026-10-06** (spare Q6A, the same
`systemd-run` properties `crate::door` uses): it ran as a DynamicUser, homed
on the relay, and served a page through the loader in ~0.6 s. Probes run
inside the same sandbox: the box's env file denied, the Postgres socket
directory denied, the core's :8000 on loopback denied, the bundle directory
readable and read-only. The full Share loop (sheet, link, door started by the
first link and stopped by the last revoke) was verified on a scratch core on
a laptop; the two have not yet run together on one box.

## Decided 2026-10-05

- **The loader lives on `virtues.ch`**, the domain the relay already uses,
  not `virtues.com`: shorter links, and shared pages never load on the domain
  that holds account and billing sessions (GitHub keeps
  `githubusercontent.com` apart for the same reason). **Live at
  `https://s.virtues.ch/` since 2026-10-06**, static files behind the Caddy
  that already serves the API (iroh-relay holds ports 80 and 443 on the relay
  host itself). Headers and file hashes: `apps/loader/README.md`. A link to
  the spare box's test door opened through it in ~0.6 s.
- **Link format: a box handle, with the self-contained form always valid.**
  - `virtues.ch/<handle>#<token>`, about 35 characters. The handle is a name
    for the box, like a username; atlas maps it to the door key. That is
    *naming*, which the doctrine allows; the token never leaves the browser.
    The trust it costs: virtues could point a handle at an impostor door,
    the same trust anyone places in DNS.
  - `s.virtues.ch/#<door-key>.<token>`, about 81 characters with the key in
    base64url (43) and a 128-bit token (22). virtues holds nothing; this is
    the form for anyone who wants no trust in virtues at all. Both forms
    work forever, so no link ever breaks because the format moved on.
  - The core builds the long form today (`api::publications::link_for`);
    handles need atlas and come later.
- **One door key per box.** Anyone holding two of a box's links can tell
  they came from the same box; the share sheet's details say so.
- **The door key is the box's link identity.** Losing it ends every link the
  box ever shared, so it goes into backups and restores, and "new door key"
  is an explicit "end all my links" action. **Built 2026-10-06:** the key
  lives sealed in `box_secrets` and each page in `app_publications.page`
  (0044); the core rebuilds the bundle directory from the database before
  starting the door, so a restored box serves its links again. Settings has a
  Shared links page listing every link with turn-off. ("New door key" as a
  button is not built.)
- **Publishing is part of the subscription**, as Nabu Casa's remote access
  is. The DIY path (own relay, own domain) arrives with wave 3.
- **Abuse policy is deferred.** Before any public launch: a stated policy
  (virtues routes and never reads; on a valid report it stops routing a
  door key) and terms to match.

## Open questions

- **Relay load.** Until a direct transport lands, every visit's bytes cross
  the OVH relay. The spike measures what a page costs.
- **Link previews.** Is a generic card acceptable for the default path, or
  does the share sheet offer an owner-written preview (title, one image) that
  the loader domain serves, which would mean virtues holds that much?
- **Handles.** Chosen by the owner or assigned; how they are claimed,
  changed, and released, and what an old handle does after a change.
- **`publish_to_github` in chat now.** Keep it as an expert path until the
  share sheet exists, or pull it so the first thing users meet is not a token
  form.

## Verification

- A page published from the spare box opens on a phone over cellular through
  the loader, and the relay host's logs show EndpointIds and byte counts,
  nothing else.
- A loader pointed at an EndpointId whose key the box does not hold fails
  the handshake, proving the link authenticates the box.
- With the box unplugged, the link shows the offline page within seconds;
  with it back, the page returns without republishing.
- The door process, given a deliberately poisoned request set (path
  traversal, oversized headers, other publications' tokens), serves nothing
  outside the requested publication, proven by a test.
- The share sheet names every image and every dereferenced entity in a page
  that links a person, before anything is written.
- Revoking makes the URL 404 at once, and the hit log shows the opens before.
- A frozen bundle renders identically on screen, in the print dialog, and as
  a downloaded file opened with the network off.

## Death condition

Delete when a person with only a box can make a page, share a link, update
it and revoke it. What survives: a manual page on sharing, and a record of
the three decisions worth keeping: *the face's constraints are publishing's
constraints*; *virtues may name, carry and introduce, never hold or read*;
and *the box is the server; virtues introduces, and carries only until it no longer has to*.
