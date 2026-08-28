# Privacy &amp; security model

> **STATUS — the transport half of this page is SUPERSEDED (2026-08-28). Do not
> quote it.** Everything down to *The inference boundary* describes the pre-iroh
> design: an SNI-routed TCP relay, per-box ACME, a box-held TLS key, a device
> bearer token, and browser-anywhere access. None of that is how reach works
> now. The current, accurate account for users is
> **[the reach manual page](../docs/operate/reach.md)**; the design notes are
> in [relay-control-plane.md](relay-control-plane.md).
>
> Four claims below are wrong on the merits, not merely dated, and they are the
> ones most likely to be lifted into copy:
>
> - **"Blind relay."** The relay cannot read content — that part is true and
>   rests on end-to-end encryption. It *does* see which two device keys are
>   talking, the addresses they connect from, and how much traffic passes when.
>   Anything that forwards packets sees volume and timing; there is no design in
>   which it does not.
> - **"RAM-only relay", "no records".** Nothing enforces or verifies this. It is
>   an operational description of one process, not a property, and it has never
>   been audited or attested.
> - **"Any browser, anywhere, no install."** A browser has no paired key, so it
>   is refused. Reach is the iPhone app, the desktop app, or a terminal on the
>   box.
> - **Box-held TLS key / device bearer.** Reach is an iroh connection
>   authenticated by the device's own key against the box's allowlist. There is
>   no per-box ACME certificate and no bearer token.
>
> **The inference boundary section is current** and remains the honest account
> of where your data actually leaves the box.

*Everything from here to the inference boundary is kept as a record of the
superseded design.* It described how you reached your home box from a browser
without opening a port at your house.

> **One sentence (superseded):** the home box *dialed out* to the relay and held
> the connection open; a browser anywhere hit the relay, which forwarded
> *sealed* bytes to the box over that already-open pipe.

See the [visual walkthrough](relay-walkthrough.html) for the lifecycle diagrams
of that design.

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
| An image is generated | Your prompt, and any image you supplied to edit |
| An applet with an `agent` prompt runs | Whatever that applet was written to look at |

It travels box → `api.virtues.com` → an AI gateway → the model provider
(Anthropic, Alibaba, Z.AI, Google, depending on the slot). So two parties beyond
the provider handle it in the clear. [virtues-api.md](virtues-api.md) states
this plainly and is the detailed account.

**What we do about it: every one of those requests demands zero data
retention.** The gateway holds retention agreements with its providers and will
route a request only through endpoints covered by one. `api.virtues.com` sets
that requirement on every call it forwards — chat, streaming chat, background
jobs, web searches, transcription, image generation — for every model that has
such an endpoint. It is not a preference you enable and it is not a default you
can drift off of; it is attached per request, derived from the model, and a
model whose provider offers no zero-retention route is refused rather than
quietly used.

Concretely, that means the providers behind your slots do not keep your
prompts, and do not train on them.

Two honest edges:

- **A few models have no zero-retention route at all.** They are marked
  *Retained* in Settings → Models and you can still choose one deliberately —
  some models are only available that way. Doing so affects **only the slot you
  put it in**: pin one for chat and your bookmarks, summaries, searches and
  transcripts stay zero-retention. There is no global switch to leave flipped,
  on purpose.
- **A model we have never heard of enforces anyway.** If the id is newer than
  our catalog or simply wrong, the request fails loudly instead of falling back
  to an endpoint we can't vouch for.

**What does NOT leave, and this is not incidental:**

- **Embeddings and reranking run on the box.** Search never ships your record out.
- **GPS coordinates are reduced on-box** to distance and pace before any prompt.
  The model is told you walked 3 km, not where you live.
- **The usage ledger records metadata only** — model, token counts, cost. Never
  content.

**What you can do beyond that:** point the box at a local model. The slot
system takes any OpenAI-compatible endpoint, so a model running on your own
hardware keeps inference on your own hardware — nothing leaves at all. The trade
is quality and speed, and it is yours to make.

**What we will not claim:** that this is private by construction. The relay
genuinely cannot read your data — that is physics, not policy. The inference
boundary is different in kind: zero retention is enforced on our side and
contractual on theirs, which is a real, checkable guarantee and still a promise
rather than an impossibility. A provider that breaks its agreement breaks
something we cannot detect from here. That is precisely why the local-model path
exists, and why this page describes the mechanism instead of asking you to trust
a badge.

## The honest tradeoff

The relay is a **single point of failure for *remote* access**: if it's down, you
can't reach the box from outside — though your LAN keeps working and the box keeps
collecting the whole time. That's the price of "reach from anywhere with no
install." It's mitigated by automatic reconnect, low-TTL DNS (repoint fast), and —
later — multiple regions. We'd rather state this plainly than pretend a hosted
relay has no dependency.

## What we are, legally

The relay operates as a **mere conduit** — it moves encrypted bytes it cannot
inspect, the same posture as a network carrier. We can't moderate what we can't
read; abuse is handled at the edges (subscription-gated access and revocation),
not by reading traffic. That is a statement about *content*: the relay still
observes which keys are connected, from which addresses, and how much passes.
