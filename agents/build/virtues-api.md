# Virtues API — privacy claims and approved wording

> This is not a spec. It explains why the paid API is built the way it is, and
> gives the plain-language wording we use to describe it. The technical design
> lives in [`entitlement.md`](entitlement.md).
>
> **Everything on this page is true of what ships today.** It was rewritten
> 2026-07-30, because it previously described the double-blind voucher model —
> which was collapsed to a linked prepaid ledger in
> [`0005_accounts_ledger.sql`](../../services/virtues-api/migrations/0005_accounts_ledger.sql)
> and is no longer what runs. The old claims are kept, clearly marked, in
> [The unlinkability we gave up](#the-unlinkability-we-gave-up) at the bottom.
> **Do not copy wording from that section.**

---

## Summary

**We can see how much your usage cost. We do not store what the usage was.**

The rest of this page follows from that.

This page used to lead with a stronger claim. This one is smaller, and it is
true.

---

## What is guaranteed by architecture and what is only policy

Most companies promise privacy: *"we won't look at your data."* That promise is
only as good as the company, its lawyers, its next owner, and whatever a court
compels. So for any privacy claim, the useful question is: **which part is
enforced by the architecture, and which part is only policy?** An
architectural guarantee still applies when nobody at the company is following
the rules. A policy does not.

Here is how ours splits.

**Guaranteed by the architecture:**

- **Your data lives on your box.** Notes, location, health, messages, files. We
  have no copy and no way to fetch one. There is no retention policy because we
  never receive the data.
- **The relay cannot read your traffic.** When you reach your box from away, the
  bytes pass through our relay as end-to-end encrypted QUIC between your device
  and your box. Your device and your box hold the keys; the relay has none and
  cannot decrypt what it forwards. See [`open-relay.md`](../record/open-relay.md)
  and [`privacy-boundary.md`](privacy-boundary.md).
- **We keep no log of what you did.** The API stores a ledger of amounts —
  `-4200 micros, kind=charge` — and nothing about the request that caused it.
  There is no prompt log, no completion log, no history table.

**Policy combined with structure — real, but reversible:**

- **The API stores no names, emails, or payment details.** It knows an opaque
  `account_id` and a balance. Identity lives in Billing.
- **But Billing and the API share that `account_id`**, so *we* can join the two
  sides: Billing turns it into a customer, the API turns it into a spend
  history. A subpoena of both databases would produce that join.

That last bullet is the limit of what we can promise. This page used to say the
join was impossible. Removing the join again is the v2 objective; until then we
don't claim it.

---

## What actually passes through us

This needs to be exact, because "we never see your data" is easy to say and
easy to disprove.

**AI requests transit our gateway.** When you ask Virtues something, the request
goes from your box to `virtues-api`, which forwards it to an upstream model
provider and passes the answer back. Your prompt is *in that request*. We do not
log it, store it, or train on it — the proxy reads the response only to extract
the cost, and what lands in the database is a number. But the prompt does pass
through our server on its way to a third party, and we say so instead of
relying on "we don't store it."

**Nothing else passes through us.** Your life-data — the notes, the location
history, the health records, the files, the day pages — is collected, indexed,
and queried entirely on your box. It never transits our infrastructure at all.

**If you don't want AI requests to pass through us, bring your own key.** With
your own provider key, every AI call goes box → provider directly: `BearerClient`
(`virtues_api/client.rs`) diverts every `/v1/ai/*` request, streaming and
buffered, before any bearer is read, so chat, compaction, day summaries, image
generation and transcription all leave by your endpoint. The calls that still
go through us are not inference: Places, Unsplash and map downloads still go
through virtues-api. Say "your AI", with the transcription caveat below.

---

## The three parties

There are three systems. We run two of them, and those two *can* share data —
we state that directly instead of claiming, as we used to, that it was
impossible.

1. **Your home server** (VirtuesOS) — yours. Stores your data and your device
   api_key. A subpoena served on your home finds no records belonging to
   *Virtues*; it finds *your* records, which you already control.

2. **Billing (Atlas)** — ours. Talks to Stripe. Knows who you are, that you pay,
   and your opaque `account_id`. **Stores no usage detail and no content.**

3. **The API (virtues-api)** — ours. Serves requests and counts a prepaid wallet
   down. Knows an `account_id`, a balance, and a ledger of amounts. **Stores no
   names, emails, or cards — and stores no content.**

In one line: **Billing knows who you are but not what you did; the API knows
what each request cost but not what it was.** Neither stores what people
actually worry about — a log of your activity — because we never create that
log.

---

## Every credential (reference)

| Credential | Created by | Raw value lives on | Reference kept at | What it does |
|---|---|---|---|---|
| **Device api_key** | Billing, at signup (rotatable) | your home server | Billing *and* the API, both hashed | proves "I'm paid" *and* spends the wallet |
| **Device key** *(pairing bearer)* | your box, at pairing | each of your devices | your box only | your phone / laptop ↔ your own box; **never leaves your house** |

People often confuse these two bearers. The **api_key** authenticates *home
server ↔ our cloud*; the **device key** authenticates *your phone ↔ your own
box*, purely locally, and Virtues' cloud never sees it. Your phone finds your
box by dialing its iroh node id — Virtues runs no lookup service that maps you
to an address.

---

## What we keep, exactly

This is the complete list. If it isn't here, we don't have it.

| Where | What | What it is *not* |
|---|---|---|
| Billing | email, Stripe customer, subscription status, `account_id` | no usage, no content |
| The API | `account_id`, balance, daily counter, ledger rows (`amount`, `kind`, `timestamp`, pre-markup cost) | no prompts, no completions, no files, **not even which model you used** — an AI charge writes no reference to the call |
| The relay | which two endpoint keys are connected, the addresses they connect from, and how many bytes pass when | no key to decrypt with — but **not** "no logs": nothing enforces or attests that, and by design it sees volume and timing |
| Your box | everything | — |

The API stores counters and amounts, not a log of events. A ledger row says
money moved and when. It does not say what for.

---

## Claims the architecture guarantees

We used to say "by construction" about everything. It applies to three things,
and we use that kind of claim only for those three:

- **The relay cannot read your traffic.** It has no key. It is not a matter of
  choosing not to; it is unable to.
- **We cannot produce your data.** It's on your box. We have no copy, and no
  way to get one.
- **We cannot produce a history of your activity.** We never write one.

And the limit, which we state openly:

- **We *can* connect your identity to your spending**, by joining Billing to the
  API on `account_id`. A subpoena of both would get: *this customer spent these
  amounts at these times.* It would not get what you asked, what came back, or
  even which model. But it would get something.

A subpoena to Billing alone yields: *"This customer pays us $20/month."*
A subpoena to the API alone yields: *"Account `acct_9f…` has $4.40 left."*
A subpoena to both yields: *those two facts, joined — a spend timeline with a
name on it.* Amounts and timestamps. No content, and not even which model
served which request: an AI charge writes no reference to the call it paid for.

---

## What we may and may not claim

- **Your data lives on your own server.** This is the main point; come back to
  it.
- **The API keeps amounts, not a history of events.** It records that money
  moved, not what the money paid for.
- **The relay cannot decrypt your traffic.** This is the strongest claim that
  rests entirely on the architecture. Make it about *content* and stop there.
  The relay does see which two keys are talking, from where, and how much
  passes, so we cannot claim it is "blind", keeps "no logs" or is "RAM-only".
  The true user-facing wording is in `docs/operate/reach.md`.
- **With your own key, AI calls do not pass through us.** This is true for every
  AI call: the fork is in `BearerClient`, keyed on the `/v1/ai/` prefix, so a
  new AI caller cannot miss it. Saying only "out of your chat" understates it.
  - Caveat: BYO is all-or-nothing. There is no per-slot route, so a user cannot
    send chat to their key and leave audio with us.
  - And **audio does not work on a BYO endpoint** — the transcription path
    sends an `image_url` data URI, which OpenAI-compatible providers reject.
    It fails with the provider's own message rather than silently. Do not
    write copy implying BYO covers transcription.

**Retired — do not use.** These described the voucher model and are now false:
*"the link lives in your house"* (it also lives with us), *"two rooms, no shared
cabinet"* (they share `account_id`), *"a baton, not a record"* (no vouchers),
*"one rotation among thousands"* (no token rotation), *"nothing to hand over"*
(a spend timeline can be handed over).

**Also avoid:** "zero-knowledge" (a specific cryptographic term we don't use),
"anonymous" without qualification (the account is pseudonymous to the proxy, not
anonymous), "military-grade," "unhackable." An overclaim undermines every true
claim next to it. This page itself overclaimed: it promised structural
unlinkability for a month after the structure changed.

---

## FAQ answers (approved wording, use verbatim)

**Q: Do you know what I use Virtues AI for?**
> No. We don't log or store your prompts or the answers, and we never train on
> them. All that lands in our database is an amount and a timestamp — the charge
> row doesn't even record which model served the request. Your prompts do pass
> through our gateway on the way to the model provider; if you'd rather they
> didn't, bring your own provider key and every AI request goes straight from
> your box to your provider, background work included.

**Q: Can you see my notes, location, or health data?**
> No. That data never leaves your box, so this does not depend on a policy. We
> have no copy of it and no way to request one. When you reach your box from
> away, the traffic goes through our relay, which has no key to decrypt it.

**Q: Could you be forced to hand over my activity?**
> We could be compelled to produce what our systems store: that you're a
> customer, and a ledger of what your account spent and when. We could not
> produce your prompts, your files, or your data, because we don't have them.
> We'd rather tell you that precisely than claim there's nothing to hand over.

**Q: How is this different from "we don't sell your data" promises?**
> In part it isn't — some of what we do is policy, and we say which parts. The
> part that isn't policy: your data is on hardware we don't own, and the relay
> that carries your traffic has no key to read it. Those are true whether or
> not you trust us.

**Q: What *can* you see?**
> Your email, your payment method, that you're a paying customer, and a ledger
> of amounts your account spent, with timestamps. That's the complete list. No
> prompts, no completions, no model names, no files, no browsing, no location.

**Q: Do I have to use Virtues' servers at all?**
> No. Run VirtuesOS on your own hardware and pay only for the conveniences you
> want — or bring your own provider keys and use none of ours.

**Q: What happens if I cancel?**
> Your prepaid balance runs out at the end of the month and doesn't renew, like
> a prepaid card.

**Q: Is it open source? Can I verify this?**
> Yes. Both Billing and the API are in the public repository — you can read the
> exact code that handles your subscription and your usage, including the ledger
> schema that shows what we store and the proxy that shows what we don't. A lint
> rule keeps customer-identity columns (email, Stripe ids) out of the API's
> schema entirely.

---

## Short explanation for a non-technical person

> "Everything Virtues knows about you — your notes, your location, your health,
> your days — lives on a little server in your house, not with us. When you use
> the AI, the question goes through us to reach the model, but we don't keep it.
> We keep only what it cost, the way a phone company keeps your bill and not
> your conversations. If you don't want even that, you can plug in your own AI
> key, and then your AI requests don't go through us at all."

---

## Rules for future features

Check every future feature against the summary at the top: *we can see how much
your usage cost; we do not store what the usage was.*

**We do not store what you did.** A "usage dashboard" that lists your past
prompts, a "personalized recommendation from your history," or any feature that
requires the cloud to remember the *content* of your activity gets a no, unless
we can build it on the box where the data already lives. This rule is central
to the product.

The guarantee we *have* given up is that your identity and your spending cannot
be linked. We traded it for recoverability and easier operation when there was
one box in use. The plan to restore it is below.

---

## The unlinkability we gave up

**Nothing in this section describes what ships. Do not copy wording from it.**

The original design meant that a subpoena would get *"we cannot"* instead of
*"here is a spend timeline."* Billing and the API shared **no column**. Each
month your box generated a fresh random usage token, asked Billing for a
one-time **voucher** proving you were paid up, and redeemed that voucher at the
API to load the month's budget. The voucher passed between the two halves
*through your own server*, and both sides discarded it as soon as it was spent.
Billing saw `customer + voucher` and never the token; the API saw
`voucher + token` and never the customer. No single system had both ends of
the chain. Everyone's token rotated on the same day, so the timing could not
identify anyone either.

It was collapsed because it made every renewal a round-trip, made the bearer a
valuable secret that could not be recovered, and needed a 25-day anti-stacking
rule that could lock a paying customer out for an afternoon. The capabilities
it cost us — instant per-account revocation, chargeback clawback, recovery
without losing the wallet — are itemized in
[`entitlement.md` §6](entitlement.md#6-what-the-linked-model-gave-up-and-got-back).

The way back is **not** to rebuild the voucher exchange. That design needed no
special cryptography because keeping the two systems separate did the work.
Without that separation, blind signatures
([RFC 9474](https://www.rfc-editor.org/rfc/rfc9474.html)) are the right tool:
one party issues and redeems tokens without being able to link the two. That
is the v2 path.

When v2 ships, the summary at the top of this page can make the stronger
claim again. Until then, it states only what is true.
