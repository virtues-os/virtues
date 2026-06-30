# Privacy &amp; security model — the blind relay

Virtues lets you reach your home box from any browser, anywhere — without opening
a port at your house and without us being able to read your data. This page is
the honest, complete statement of how that works and what each party can (and
can't) see.

> **One sentence:** your home box *dials out* to a blind relay and holds the
> connection open; a browser anywhere hits the relay, which forwards *sealed*
> bytes to your box over that already-open pipe. Nobody opens a port at your
> house, and the relay can't read a thing.

See the [visual walkthrough](relay-walkthrough.html) for the lifecycle diagrams,
and [relay-control-plane.md](relay-control-plane.md) for the design.

## Who holds which secret — and who deliberately doesn't

This table *is* the privacy model. The ✗ cells are the point.

| Secret | Mac (browser) | Relay | Box | atlas | What it's for / why |
|---|:--:|:--:|:--:|:--:|---|
| **RELAY_SECRET** | ✗ | ✓ | ✗ | ✓ | Master HMAC key. atlas *mints* tokens with it; the relay *verifies* with it. The box never sees it — so a stolen box can't forge other boxes' tokens. |
| **per-SNI token** | ✗ | derives, doesn't store | ✓ | mints | `HMAC(RELAY_SECRET, "<sni>:<day>")`. atlas mints it for *your* box only; the box presents it to register. The relay recomputes to check — it stores nothing. |
| **box TLS private key** | ✗ | ✗ **never** | ✓ | ✗ | **The blindness.** Only the box can decrypt the browser↔box TLS. The relay forwards ciphertext it has no key for — it *cannot* read your data, by construction. |
| **api_key** | ✗ | ✗ | ✓ | ✓ | Box ↔ atlas auth (billing + "give me my relay config"). Proves which account the box is, so atlas mints the right token. |
| **device bearer** | ✓ | ✗ | ✓ | ✗ | Your app login, sent on every request. It travels *inside* the TLS the box terminates — so the relay never sees it. App-auth is the real lock. |
| **your actual data** | sees its own | ✗ ciphertext only | ✓ lives here | ✗ | Notes, location, health — never leave the box except as E2E-encrypted bytes to *your* browser. The relay sees the SNI (cleartext) + sealed bytes, nothing more. |

## What this buys you

- **No open port at home.** The box reaches *outbound* to the relay — which every
  network allows — so there's nothing inbound to attack, scan, or DDoS at your
  house. Works behind CGNAT, coworking/café wifi, and v6-only home ISPs alike.
- **Blind by construction, not by promise.** The relay has no TLS key. "We can't
  read your data" isn't a policy you have to trust — it's something the relay is
  *physically unable* to do. It sees the destination name and sealed bytes.
- **End-to-end encrypted to *your* box.** TLS terminates on the box, with the
  box's own key. The café, the coworking network, and the relay all see only
  ciphertext.
- **Revocation without a database.** Tokens are scoped to your name *and* the day.
  A leaked token only works for your box, and only until it's re-minted — which
  atlas won't do if your access is revoked. The relay enforces this by
  recomputing an HMAC; it keeps no list of valid tokens.
- **RAM-only relay.** No database of who's who. Reboot it and nothing is lost —
  boxes simply re-dial. The property that makes it private (no records) is the
  same one that makes it simple (no state to leak, back up, or corrupt).
- **Your data lives at home.** The box is the source of truth and holds the keys.
  The cloud does two small jobs: mint a token (atlas) and move sealed bytes
  (relay). Delete both and your data is untouched on your box.

## The honest tradeoff

The relay is a **single point of failure for *remote* access**: if it's down, you
can't reach the box from outside — though your LAN keeps working and the box keeps
collecting the whole time. That's the price of "reach from anywhere with no
install." It's mitigated by automatic reconnect, low-TTL DNS (repoint fast), and —
later — multiple regions. We'd rather state this plainly than pretend a hosted
relay has no dependency.

## What we are, legally

Because the relay is blind, it operates as a **mere conduit** — it moves
encrypted bytes it cannot inspect, the same posture as a network carrier. We
can't moderate what we can't see; abuse is handled at the edges (payment-gated
access and revocation), not by reading traffic.
