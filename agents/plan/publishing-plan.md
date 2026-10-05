# Publishing: the box serves, virtues names and carries

> **STATUS 2026-10-05: direction rewritten, spike not started.** The
> 09-01 version decided "v1 builds no ingress" and handed a user with no
> domain a file. This version reverses that: a page is served **by the box**,
> through a relay that forwards TLS it cannot read, at a name virtues hands
> out. The publication primitive, the face as the medium, and the doctrine
> carry over unchanged. One piece is built (`publish_to_github`, below); it
> becomes an optional destination, not the front door.

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

### We built the relay once already

`48abf48e` built a blind L4 SNI-passthrough relay, a box client, per-box ACME
and TLS hot-swap, so any browser could reach the box. `e56f0963` replaced it
with iroh, because the owner's own apps hold keys and iroh gives them direct
and hole-punched paths with no CA in the loop. That was right for **owner
reach** and stays right. Publishing is a different reader, a stranger with a
browser, and for that reader the retired design is the only one that works.

What the archive learned, and this plan inherits
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
| Names it | virtues (a box subdomain), or the owner's own domain |
| Carries it | a relay that forwards TLS it cannot decrypt |

> **If virtues.com vanished tomorrow, does the box still hold everything and
> still work for its owner?**

Yes, under this design: links stop resolving, nothing is lost. That is the
same answer Synology and Home Assistant give.

## The design

### Name

- **One hostname per box**, pages at unguessable paths:
  `<box>.<publish-domain>/p/<token>`. Certificate Transparency logs publish
  every hostname a cert covers within minutes and scanners watch them, so a
  per-page hostname would announce that the page exists. With paths, the logs
  show that a box exists and nothing else, and the relay sees "this box got a
  visit", not which page.
- **A dedicated domain**, not `virtues.com`, on the **Public Suffix List**, so
  each box is its own site for cookies and for browser reputation. One box
  hosting phishing must not get every box's links a red warning.
- **Own domain** is the same mechanism: a CNAME to the relay, a cert on the
  box for that name.

### Carry

The relay host already runs `iroh-relay` on :443 for `relay.virtues.ch`. Add
a **publish gateway** beside it:

1. An SNI router on :443 sends `relay.virtues.ch` to iroh-relay and any
   publish hostname to the gateway. It reads the SNI and nothing else.
2. The gateway is an iroh endpoint the box allowlists **for one ALPN only**
   (`virtues/publish/1`). On a browser connection it opens an iroh stream to
   that box and copies raw TLS bytes both ways.
3. On the box, that ALPN goes to the publish door (below), **never to the app
   router**. `relay::maybe_spawn` hands the full API to the iroh transport,
   which is correct for allowlisted devices and must not be extended to this
   peer.

Reusing iroh means no new tunnel protocol: the box already holds a
reconnecting connection to this host, with hole-punching and backoff.

The gateway enforces, without reading anything: connection and bandwidth caps
per box, and an honest "this server is offline" page when the box is not
connected (served for the hostname, no box content involved).

### Certificate

The box generates and keeps its key. Issuance is DNS-01: the box asks atlas,
authenticated by its identity, to publish the challenge TXT for its own name
only, and runs ACME itself. virtues touches DNS, never the key. Rate limits,
CAA and CT monitoring as above.

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
| Discovery via CT logs within minutes | one hostname per box, unguessable paths, uniform 404 |
| A bug in the public code path | separate door process, no DB credentials, sandboxed |
| Floods on a home uplink and a Q6A CPU | per-box caps at the gateway; one-tap pause on the box |
| Guest writes as prompt injection | off by default; edit links; rows marked as guest input |
| One box's phishing tarring every box | Public Suffix List; subscribers only; unroute a name on abuse |
| Relay metadata (which box, when) | stated plainly; own domain does not remove it, only the box-per-name |
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
- **Port forwarding** as the path. Exposes the home IP and needs router
  skills; it stays possible for experts with their own domain.
- **The owner's ambient `gh` login on the box.** Every repo, held by a login
  the service user cannot see, and absent on every other box.

## The work, in order

1. **Spike on the spare box** (`ssh dragon2`, never the main box): SNI router
   plus gateway on a scratch port of the relay host, the `virtues/publish/1`
   ALPN, a cert on the box via DNS-01, and a static page opened from a phone
   on cellular. Measure: first-byte latency through the relay, behavior when
   the box drops, what the relay logs.
2. **The publication primitive and the freezer**: `app_publications`
   (claim a migration number first), face → one self-contained file, assets
   inlined or content-addressed, the box-only lint `publish_to_github`
   already has.
3. **The door process** and its packaging (unit, user, sandbox, the bundle
   directory).
4. **The share sheet and link management** in the app, with "what leaves".
5. **Fix the origin bug** so existing page shares work on the LAN meanwhile.
6. **Own domain.**
7. **Live pages** (declared queries over the local socket).
8. **Guest writes**, once guest-input marking exists.
9. **GitHub App and S3 destinations**; retire the pasted-token source.

Paged print (`@page`, break control) rides on the freezer whenever it is
picked up; PDF is the browser's print dialog, never a headless browser on the
box.

## Open questions

- **The publish domain.** A new registrable domain on the Public Suffix List.
  Name not chosen.
- **The CA.** Let's Encrypt with a rate-limit override, a commercial ACME CA,
  or both. Decide before more than a few dozen boxes publish.
- **Free tier.** Owner reach through the relay is free since 2026-08-31
  ([open-relay.md](../record/open-relay.md)). Is publishing part of the
  subscription, as Nabu Casa's remote access is, or free like reach?
- **`publish_to_github` in chat now.** Keep it as an expert path until the
  share sheet exists, or pull it so the first thing users meet is not a token
  form.

## Verification

- A page published from the spare box opens on a phone over cellular, and the
  relay host's logs show a hostname and byte counts, nothing else.
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
and *the box is the server, the relay only forwards*.
