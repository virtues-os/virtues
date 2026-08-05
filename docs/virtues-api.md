# Virtues API — The Idea

> This is not a spec. It's the *why* — the philosophy, the motifs, and the
> plain-language way we tell people about it. The technical design lives in
> [`entitlement.md`](entitlement.md).
>
> **Everything on this page is true of what ships today.** It was rewritten
> 2026-07-30, because it previously described the double-blind voucher model —
> which was collapsed to a linked prepaid ledger in
> [`0005_accounts_ledger.sql`](../services/virtues-api/migrations/0005_accounts_ledger.sql)
> and is no longer what runs. The old claims are preserved, clearly fenced, in
> [Where we're going](#where-were-going-the-claim-we-gave-up) at the bottom.
> **Do not lift copy from that section.**

---

## The one sentence

**We can see what your usage cost. We never keep what it was.**

Everything else follows from that.

It is a smaller sentence than the one this page used to lead with, and it has
the advantage of being true.

---

## The core idea

Most companies promise privacy: *"we won't look at your data."* That promise is
only as good as the company, its lawyers, its next owner, and whatever a court
compels. So the interesting question about any privacy claim is: **which part is
architecture, and which part is policy?** Architecture holds when we're not in
the room. Policy doesn't.

Here is ours, split honestly.

**Architecture — holds by construction:**

- **Your data lives on your box.** Notes, location, health, messages, files. We
  have no copy and no way to fetch one. This is not a retention policy; there is
  nothing to retain.
- **The relay cannot read your traffic.** When you reach your box from away, the
  bytes pass through our relay — which holds **no TLS key** for that connection.
  Your box terminates the encryption with its own key. The relay physically
  cannot decrypt what it forwards. See
  [`privacy-model.md`](privacy-model.md).
- **We keep no record of what you did.** The API stores a ledger of amounts —
  `-4200 micros, kind=charge` — and nothing about the request that caused it.
  There is no prompt log, no completion log, no history table.

**Policy and structure together — real, but not impossible to undo:**

- **The API holds no names, emails, or payment details.** It knows an opaque
  `account_id` and a balance. Identity lives in Billing.
- **But Billing and the API share that `account_id`**, so *we* can join the two
  sides: Billing turns it into a customer, the API turns it into a spend
  history. A subpoena of both databases would produce that join.

That last bullet is the honest limit, and it used to be the thing this page
promised was impossible. Restoring it is the v2 objective; until then we don't
claim it.

---

## What actually passes through us

Worth being exact, because "we never see your data" is the kind of sentence
that's easy to say and easy to falsify.

**AI requests transit our gateway.** When you ask Virtues something, the request
goes from your box to `virtues-api`, which forwards it to an upstream model
provider and passes the answer back. Your prompt is *in that request*. We do not
log it, store it, or train on it — the proxy reads the response only to extract
the cost, and what lands in the database is a number — but it does pass through
our server on its way to a third party, and honesty requires saying so rather
than hiding behind "we don't store it."

**Everything else does not.** Your actual life-data — the notes, the location
history, the health records, the files, the day pages — is collected, indexed,
and queried entirely on your box. It never transits our infrastructure at all.

**If that trade doesn't suit you — not yet.** Bringing your own provider key is
designed to take the gateway out of the path entirely, with the box talking to
your provider directly. **It is not built.** The Billing screen stores a key,
but nothing reads it during a chat, so today every call still goes through the
gateway whether or not a key is set. Do not make this promise to anyone until
the routing lands.

---

## The three parties

Three rooms. Two of them are ours, and they *can* compare notes — which is the
part we say plainly rather than the part we used to claim was impossible.

1. **Your home server** (VirtuesOS) — yours. Holds your data and your device
   api_key. Nobody subpoenas your living room and finds *Virtues'* records
   there; they find *your* records, which you already control.

2. **Billing (Atlas)** — ours. Talks to Stripe. Knows who you are, that you pay,
   and your opaque `account_id`. **Holds no usage detail and no content.**

3. **The API (virtues-api)** — ours. Serves requests and counts a prepaid wallet
   down. Knows an `account_id`, a balance, and a ledger of amounts. **Holds no
   names, emails, or cards — and stores no content.**

The wall in one line: **Billing knows who you are but not what you did; the API
knows what it cost but not what it was.** Neither holds the thing people
actually worry about — a record of your activity — because that record does not
exist on our side at all.

---

## Every credential (reference)

| Credential | Created by | Raw value lives on | Reference kept at | What it does |
|---|---|---|---|---|
| **Device api_key** | Billing, at signup (rotatable) | your home server | Billing *and* the API, both hashed | proves "I'm paid" *and* spends the wallet |
| **Device key** *(pairing bearer)* | your box, at pairing | each of your devices | your box only | your phone / laptop ↔ your own box; **never leaves your house** |

Two "bearers" people conflate, kept distinct: the **api_key** is *home server ↔
our cloud*; the **device key** is *your phone ↔ your own box*, pure local auth
that Virtues' cloud never sees. Your phone finds your box by dialing its iroh
node id — Virtues runs no lookup service that maps you to an address.

---

## What we keep, exactly

The complete list. If it isn't here, we don't have it.

| Where | What | What it is *not* |
|---|---|---|
| Billing | email, Stripe customer, subscription status, `account_id` | no usage, no content |
| The API | `account_id`, balance, daily counter, ledger rows (`amount`, `kind`, `timestamp`, pre-markup cost) | no prompts, no completions, no files, **not even which model you used** — an AI charge writes no reference to the call |
| The relay | in-memory only: destination name (SNI) and byte counts | no logs, no database, no key to decrypt with |
| Your box | everything | — |

**Counters and amounts, not logs of events.** A ledger row says money moved and
when. It does not say what for.

---

## By construction, not by promise — what still earns that phrase

We used this line for everything. It applies to three things, and we should use
it for exactly those three:

- **The relay cannot read your traffic.** It has no key. Not "won't" — *can't*.
- **We cannot produce your data.** It's on your box. We have no copy, and no
  path to one.
- **We cannot produce a history of your activity.** It was never written down.

And the honest counterpart, which we state rather than bury:

- **We *can* connect your identity to your spending**, by joining Billing to the
  API on `account_id`. A subpoena of both would get: *this customer spent these
  amounts at these times.* Not what you asked. Not what came back. Not even
  which model. But not nothing, either.

A subpoena to Billing alone yields: *"This customer pays us $20/month."*
A subpoena to the API alone yields: *"Account `acct_9f…` has $4.40 left."*
A subpoena to both yields: *those two facts, joined — a spend timeline with a
name on it.* Amounts and timestamps. Not content, and not even which model
served which request: an AI charge writes no reference to the call it paid for.

---

## Thematic motifs (use these consistently)

The recurring images. Reach for them in copy, docs, support replies, talks:

- **"Your data lives in your house."** The life-data is on the user's own
  server. Always bring it back to this.
- **"Counters, not logs."** We track amounts moving, never a history of events.
- **"We know the price, not the purchase."** The cleanest one-line summary of
  what the ledger is.
- **"Nothing to decrypt with."** The relay's blindness — the strongest fully
  structural claim we have.
- ~~**"Bring your own key and we're out of the path."**~~ **Do not use — not
  true yet.** The key is stored but never read during inference (see
  `api/settings_byo.rs`). Restore this line when routing ships; per
  `docs/byo-ai-plan.md` it will still only cover `stream()`, so even then it
  needs qualifying rather than stating flat.

**Retired — do not use.** These described the voucher model and are now false:
*"the link lives in your house"* (it also lives with us), *"two rooms, no shared
cabinet"* (they share `account_id`), *"a baton, not a record"* (no vouchers),
*"one rotation among thousands"* (no token rotation), *"nothing to hand over"*
(a spend timeline can be handed over).

**Also avoid:** "zero-knowledge" (a specific cryptographic term we don't use),
"anonymous" without qualification (the account is pseudonymous to the proxy, not
anonymous), "military-grade," "unhackable." Overclaiming poisons the well — and
this page is itself the cautionary tale, having promised structural
unlinkability for a month after the structure changed.

---

## Marketing sentences (FAQ-ready, lift verbatim)

**Q: Do you know what I use Virtues AI for?**
> No. We don't log or store your prompts or the answers, and we never train on
> them. All that lands in our database is an amount and a timestamp — the charge
> row doesn't even record which model served the request. Your prompts do pass
> through our gateway on the way to the model provider, and today there is no
> way to avoid that. Bringing your own provider key is designed to remove us
> from that path, but it is not built yet — the setting stores a key without
> using it.

**Q: Can you see my notes, location, or health data?**
> No, and not as a matter of policy — that data never leaves your box. We have
> no copy of it and no way to request one. When you reach your box from away,
> the traffic goes through our relay, which holds no key to decrypt it.

**Q: Could you be forced to hand over my activity?**
> We could be compelled to produce what our systems hold: that you're a
> customer, and a ledger of what your account spent and when. We could not
> produce your prompts, your files, or your data, because we don't have them.
> We'd rather tell you that precisely than claim there's nothing to hand over.

**Q: How is this different from "we don't sell your data" promises?**
> Partly it isn't — some of what we do is policy, and we say which parts. The
> part that isn't policy: your data is on hardware we don't own, and the relay
> that carries your traffic has no key to read it. Those hold whether or not you
> trust us.

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

## How to tell a non-technical person (the 20-second version)

> "Everything Virtues knows about you — your notes, your location, your health,
> your days — lives on a little server in your house. Not with us. When you use
> the AI, the question goes through us to reach the model, but we don't keep it;
> what we keep is what it cost, the way a phone company keeps your bill and not
> your conversations. And if even that bothers you, you can plug in your own AI
> key and we're out of the loop completely."

---

## The standard we hold ourselves to

Every future feature gets measured against the one sentence at the top: *we can
see what your usage cost, we never keep what it was.*

The line we will not cross is **storing what you did.** A "usage dashboard" that
lists your past prompts, a "personalized recommendation from your history," any
feature that requires the cloud to remember the *content* of your activity — the
answer is no, or we find a way to do it on the box where the data already lives.
That refusal is the product.

The line we *have* crossed, and should be honest about, is identity↔spend
unlinkability. We traded it for recoverability and operability at a point when
the userbase was one box. Getting it back is below.

---

## Where we're going — the claim we gave up

**Nothing in this section describes what ships. Do not lift copy from it.**

The original design made the subpoena answer *"we cannot"* rather than *"here is
a spend timeline."* Billing and the API shared **no column**. Each month your box
minted a fresh random usage token, asked Billing for a one-time **voucher**
proving you were paid up, and redeemed that voucher at the API to load the
month's budget — with the voucher passing between the two halves *through your
own server*, and both sides forgetting it the instant it was spent. Billing saw
`customer + voucher` and never the token; the API saw `voucher + token` and never
the customer. No single system held both ends of the chain. Everyone's token
rotated on the same day, so even the timing carried no fingerprint.

It was collapsed because it made every renewal a round-trip, made the bearer a
precious unrecoverable secret, and needed a 25-day anti-stacking rule that could
lock a paying customer out for an afternoon. The capabilities it cost us —
instant per-account revocation, chargeback clawback, recovery without losing the
wallet — are itemized in
[`entitlement.md` §6](entitlement.md#6-what-the-linked-model-gave-up-and-got-back).

The way back is **not** to rebuild the voucher dance. That design argued we
needed no clever cryptography because separation did the work; without the
separation, the conclusion flips, and blind signatures
([RFC 9474](https://www.rfc-editor.org/rfc/rfc9474.html)) become exactly the
right tool — one party issuing and redeeming while blinding itself from the
link. That is the v2 path.

When it ships, the sentence at the top of this page can get its ambition back.
Until then, it says what's true.
