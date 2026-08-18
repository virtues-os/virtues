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


> **Accuracy note (2026-08-18).** Two rows below describe the pre-iroh design:
> there is no longer a **device bearer** token (a device is authenticated by its
> paired iroh key, checked against an allowlist) and the box does not terminate
> its own **TLS** for the browser (reach is an iroh connection, LAN-direct or
> hole-punched, relayed only as a fallback). The blindness property is unchanged
> — the relay still forwards bytes it cannot read — but the mechanism is iroh's
> transport encryption rather than a box-held TLS key. Treat the two rows as
> historical until this page is rewritten against
> [the current reach design](relay-control-plane.md).

## Who holds which secret — and who deliberately doesn't

This table *is* the privacy model. The ✗ cells are the point.

| Secret | Mac (browser) | Relay | Box | atlas | What it's for / why |
|---|:--:|:--:|:--:|:--:|---|
| **RELAY_SECRET** | ✗ | ✓ | ✗ | ✓ | Master HMAC key. atlas *mints* tokens with it; the relay *verifies* with it. The box never sees it — so a stolen box can't forge other boxes' tokens. |
| **per-SNI token** | ✗ | derives, doesn't store | ✓ | mints | `HMAC(RELAY_SECRET, "<sni>:<day>")`. atlas mints it for *your* box only; the box presents it to register. The relay recomputes to check — it stores nothing. |
| **box TLS private key** | ✗ | ✗ **never** | ✓ | ✗ | **The blindness.** Only the box can decrypt the browser↔box TLS. The relay forwards ciphertext it has no key for — it *cannot* read your data, by construction. |
| **api_key** | ✗ | ✗ | ✓ | ✓ | Box ↔ atlas auth (billing + "give me my relay config"). Proves which account the box is, so atlas mints the right token. |
| **device bearer** | ✓ | ✗ | ✓ | ✗ | Your app login, sent on every request. It travels *inside* the TLS the box terminates — so the relay never sees it. App-auth is the real lock. |
| **your actual data** | sees its own | ✗ ciphertext only | ✓ lives here | ✗ | Notes, location, health — at rest and in transit, these stay on the box. The relay sees sealed bytes and nothing more. **But read the next row: this is a statement about STORAGE and TRANSPORT, not about the assistant.** |
| **what the assistant reads** | — | ✗ | ✓ sends it | ✗ | **This is the one place your data leaves the box, and it is not a leak — it is the feature.** Asking a question, or letting the box write your day, sends the relevant part of your record to a model provider. See [the inference boundary](#the-inference-boundary-where-your-data-does-leave) below. |

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

## The inference boundary — where your data *does* leave

Everything above is about storage and transport, and all of it is true. It is
also not the whole picture, and a privacy page that stops there is telling you
something misleading by omission.

**Virtues is an assistant. Assistants run on models. Unless you point the box at
a local one, those models are somebody else's computers.**

What goes out, concretely:

| When | What leaves the box |
|---|---|
| You ask a question in chat | Your question, plus the parts of your record retrieved to answer it — message text, transcripts, calendar entries, transactions — plus your narrative identity and standing rules |
| The nightly day write-up | Verbatim message excerpts (both directions), individual transactions with merchant and amount, calendar entries with attendee names, app and browser titles, transcript text |
| A voice recording is transcribed | **The audio itself**, not a transcript — the recording is uploaded to the transcription model |
| An applet with an `agent` prompt runs | Whatever that applet was written to look at |

It travels box → `api.virtues.com` → an AI gateway → the model provider
(OpenAI, Anthropic, Google, xAI, depending on the slot). So two parties beyond
the provider handle it in the clear. [virtues-api.md](virtues-api.md) states
this plainly and is the detailed account.

**What does NOT leave, and this is not incidental:**

- **Embeddings and reranking run on the box.** Search never ships your record out.
- **GPS coordinates are reduced on-box** to distance and pace before any prompt.
  The model is told you walked 3 km, not where you live.
- **The usage ledger records metadata only** — model, token counts, cost. Never
  content.

**What you can do about it:** point the box at a local model. The slot system
takes any OpenAI-compatible endpoint, so a model running on your own hardware
keeps inference on your own hardware. The trade is quality and speed, and it is
yours to make.

**What we will not claim:** that this is private by construction. The relay
genuinely cannot read your data — that is physics, not policy. The inference
boundary is the opposite: it rests on the provider's contract and our choice of
provider. Saying so is the point of this page.

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
