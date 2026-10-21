# Privacy &amp; security model

> **STATUS — the transport half of this page is SUPERSEDED (2026-08-28). Do not
> quote it.** Everything down to *The inference boundary* describes the pre-iroh
> design: an SNI-routed TCP relay, per-box ACME, a box-held TLS key, a device
> bearer token, and browser-anywhere access. None of that is how reach works
> now. The current, accurate account for users is
> **[the reach manual page](../../docs/operate/reach.md)**; the design notes are
> in [relay-control-plane.md](../archive/relay-control-plane.md).
>
> Four claims below are wrong, not just out of date, and they are the ones most
> likely to be copied into user-facing text:
>
> - **"Blind relay."** The relay cannot read content — that part is true and
>   rests on end-to-end encryption. It *does* see which two device keys are
>   talking, the addresses they connect from, and how much traffic passes when.
>   Any server that forwards packets can see volume and timing, whatever the
>   design.
> - **"RAM-only relay", "no records".** Nothing enforces or verifies this. It
>   describes how one process happened to be run, and it has never been audited
>   or attested.
> - **"Any browser, anywhere, no install."** A browser has no paired key, so it
>   is refused. Reach is the iPhone app, the desktop app, or a terminal on the
>   box.
> - **Box-held TLS key / device bearer.** Reach is an iroh connection
>   authenticated by the device's own key against the box's allowlist. There is
>   no per-box ACME certificate and no bearer token.
>
> **Archived 2026-09-29.** The inference boundary, the one part that was still
> current, is maintained in [`privacy-boundary.md`](../build/privacy-boundary.md);
> read that file instead of the copy below.

*Everything from here to the inference boundary describes the superseded
design and is kept for reference.* It described how you reached your home box from a browser
without opening a port at your house.

> **One sentence (superseded):** the home box *dialed out* to the relay and held
> the connection open; a browser anywhere hit the relay, which forwarded
> *sealed* bytes to the box over that already-open pipe.

That design is written up in [networking-relay-tee.md](../archive/networking-relay-tee.md).

## Which party has which secret

This table is the privacy model. The ✗ cells show which parties never get a
given secret.

| Secret | Mac (browser) | Relay | Box | atlas | What it's for / why |
|---|:--:|:--:|:--:|:--:|---|
| **RELAY_SECRET** | ✗ | ✓ | ✗ | ✓ | Master HMAC key. atlas *mints* tokens with it; the relay *verifies* with it. The box never sees it — so a stolen box can't forge other boxes' tokens. |
| **per-SNI token** | ✗ | derives, doesn't store | ✓ | mints | `HMAC(RELAY_SECRET, "<sni>:<day>")`. atlas mints it for *your* box only; the box presents it to register. The relay recomputes to check — it stores nothing. |
| **box TLS private key** | ✗ | ✗ **never** | ✓ | ✗ | Only the box can decrypt the browser↔box TLS. The relay forwards ciphertext it has no key for, so by design it cannot read your data. |
| **api_key** | ✗ | ✗ | ✓ | ✓ | Box ↔ atlas auth (billing + "give me my relay config"). Proves which account the box is, so atlas mints the right token. |
| **device bearer** | ✓ | ✗ | ✓ | ✗ | Your app login, sent on every request. It travels *inside* the TLS the box terminates, so the relay never sees it. App auth is what actually controls access. |
| **your actual data** | sees its own | ✗ ciphertext only | ✓ lives here | ✗ | Notes, location, health — at rest and in transit, these stay on the box. The relay sees sealed bytes and nothing more. **This row covers storage and transport only. The next row covers the assistant.** |
| **what the assistant reads** | — | ✗ | ✓ sends it | ✗ | **This is the one place your data leaves the box, and it is how the assistant works.** Asking a question, or letting the box write your day, sends the relevant part of your data to a model provider. See [the inference boundary](#the-inference-boundary-where-your-data-does-leave) below. |

## What this buys you

- **No open port at home.** The box reaches *outbound* to the relay — which every
  network allows — so there's nothing inbound to attack, scan, or DDoS at your
  house. Works behind CGNAT, coworking/café wifi, and v6-only home ISPs alike.
- **The relay cannot read content.** The relay has no TLS key, so it is unable
  to decrypt your data; you do not have to rely on a policy. It sees the
  destination name and encrypted bytes.
- **End-to-end encrypted to *your* box.** TLS terminates on the box, with the
  box's own key. The café, the coworking network, and the relay all see only
  ciphertext.
- **Revocation without a database.** Tokens are scoped to your name *and* the day.
  A leaked token only works for your box, and only until it's re-minted — which
  atlas won't do if your access is revoked. The relay enforces this by
  recomputing an HMAC; it keeps no list of valid tokens.
- **RAM-only relay.** The relay has no database of which box is which. Reboot it
  and nothing is lost; boxes re-dial. Keeping no stored state also means there is
  nothing to leak, back up, or corrupt.
- **Your data lives at home.** The box is the source of truth and holds the keys.
  The cloud does two small jobs: mint a token (atlas) and move sealed bytes
  (relay). Delete both and your data is untouched on your box.

## The inference boundary — where your data *does* leave

Everything above is about storage and transport. A privacy page that stopped
there would be misleading, because it leaves out inference.

**Virtues is an assistant, and the assistant runs on models. Unless you point the
box at a local model, those models run on someone else's servers.**

What goes out, concretely:

| When | What leaves the box |
|---|---|
| You ask a question in chat | Your question, plus the parts of your data retrieved to answer it — message text, transcripts, calendar entries, transactions — plus your narrative identity and standing rules |
| The nightly day write-up | Verbatim message excerpts (both directions), individual transactions with merchant and amount, calendar entries with attendee names, app and browser titles, transcript text |
| A voice recording is transcribed | **The audio itself**, not a transcript — the recording is uploaded to the transcription model |
| An image is generated | Your prompt, and any image you supplied to edit |
| An applet with an `agent` prompt runs | Whatever that applet was written to look at |

It travels box → `api.virtues.com` → an AI gateway → the model provider
(Anthropic, Alibaba, Z.AI, Google, depending on the slot). So two parties beyond
the provider handle it in the clear. [virtues-api.md](../build/virtues-api.md) has the
detailed account.

**What we do about it: every one of those requests demands zero data
retention.** The gateway has retention agreements with its providers and will
route a request only through endpoints covered by one. `api.virtues.com` sets
that requirement on every call it forwards — chat, streaming chat, background
jobs, web searches, transcription, image generation — for every model that has
such an endpoint. You do not enable it and cannot accidentally turn it off: it is attached to
each request based on the model, and a request to a model whose provider offers
no zero-retention route is refused.

Concretely, that means the providers behind your slots do not keep your
prompts, and do not train on them.

Two exceptions:

- **A few models have no zero-retention route at all.** They are marked
  *Retained* in Settings → Models and you can still choose one on purpose;
  some models are only available that way. Doing so affects **only the slot you
  put it in**: pin one for chat and your bookmarks, summaries, searches and
  transcripts stay zero-retention. There is intentionally no global switch that
  could be left on.
- **Unknown models still require zero retention.** If the id is newer than our
  catalog or simply wrong, the request fails with an error instead of falling
  back to an endpoint we can't verify.

**What does NOT leave the box:**

- **Embeddings and reranking run on the box.** Search never sends your data out.
- **GPS coordinates are reduced on-box** to distance and pace before any prompt.
  The model is told you walked 3 km, not where you live.
- **The usage ledger records metadata only** — model, token counts, cost. Never
  content.

**What you can do beyond that:** point the box at a local model. The slot
system takes any OpenAI-compatible endpoint, so a model running on your own
hardware keeps inference on your own hardware — nothing leaves at all. The cost
is quality and speed, and you decide whether to pay it.

**What we do not claim:** that inference is private by design. The relay cannot
read your data because it has no key. Inference is different: zero retention is
enforced on our side and contractual on the provider's side. That is a real,
checkable guarantee, but it still depends on the provider keeping its
agreement, and we cannot detect a provider that breaks it. That is why the
local-model path exists, and why this page describes the mechanism in detail.

## Tradeoff: the relay is a single point of failure

The relay is a **single point of failure for *remote* access**: if it's down, you
can't reach the box from outside — though your LAN keeps working and the box keeps
collecting the whole time. That is the cost of reaching the box from anywhere with no
install. It's mitigated by automatic reconnect, low-TTL DNS (repoint fast), and —
later — multiple regions. A hosted relay is a dependency, and this page says so.

## What we are, legally

The relay operates as a **mere conduit** — it moves encrypted bytes it cannot
inspect, the same posture as a network carrier. We can't moderate traffic we can't
read, so abuse is handled at the edges through subscription-gated access and
revocation. That is a statement about *content*: the relay still
observes which keys are connected, from which addresses, and how much passes.
