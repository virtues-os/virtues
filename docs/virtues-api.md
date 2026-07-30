# Virtues API — The Idea

> This is not a spec. It's the *why* — the philosophy, the motifs, and the
> plain-language way we tell people about it. The technical design lives in
> [`entitlement.md`](entitlement.md).

> ## ⚠️ This page describes the v2 goal, not what ships today.
>
> The unlinkability claim below was **architecturally true** under the
> double-blind voucher model. That model was collapsed to a linked prepaid
> ledger in
> [`0005_accounts_ledger.sql`](../services/virtues-api/migrations/0005_accounts_ledger.sql):
> Atlas and virtues-api now **share an opaque `account_id`**, so identity and
> usage *are* joinable by someone holding both of our databases.
>
> **Nothing on this page may be used as customer-facing copy until either the
> blind-signature work ([RFC 9474](https://www.rfc-editor.org/rfc/rfc9474.html))
> ships or the wording is rewritten to the weaker claim that is actually true.**
> See [`entitlement.md` §1](entitlement.md#1-the-privacy-posture--read-this-first)
> for the honest current posture.
>
> **What is true today and is safe to say:** your data lives on your box; we
> never see your content — not prompts, not completions, not files, only token
> counts and costs; and the proxy holds no names, emails, or payment details.
> That is *data minimization*, which is a real and defensible promise. It is not
> the *structural impossibility* this page describes.

---

## The one sentence

**The only machine on Earth that can connect your identity to your usage is the one in your house.**

Everything else follows from that.

> **Status: aspirational.** Today a second machine can also make that
> connection — ours, by joining Atlas to virtues-api on `account_id`. Restoring
> this sentence to literal truth is the v2 objective.

---

## The core idea

Most companies promise privacy: *"we won't look at your data."* That promise is only as good as the company, its lawyers, its next owner, and whatever a court compels. We didn't want a promise. We wanted it to be **impossible** — so that even we, even under subpoena, even if we wanted to, cannot tie what you do to who you are.

So we split the company's knowledge in two and made sure the two halves can never be rejoined:

- **Billing** knows *who you are* — your email, your card, that you pay us $20/month. It knows **nothing** about what you do.
- **The API** knows *what gets used* — that some anonymous token spent a few cents on a map lookup. It knows **nothing** about who you are.

The link between "who" and "what" exists in exactly one place: **your own server, in your own home.** That's your data, on your hardware. We never hold it.

> **Where this diverges from the code.** The split above is real — Billing still
> holds no usage, and the API still holds no names, emails, or cards. What the
> shipped system lacks is the *"can never be rejoined"* part: both sides key on
> the same opaque `account_id`, so the two halves can be joined by us. The
> subpoena scenario in the paragraph above is therefore not yet defended
> against by construction.

---

## The three parties

Think of it as three rooms that never share a filing cabinet:

1. **Your home server** (VirtuesOS) — yours. Holds both your billing identity and your usage token, linked together. This is fine, because it's *your* box and *your* data. Nobody subpoenas your living room and finds Virtues' records there — they find *your* records, which you already control.

2. **Billing (Atlas)** — ours. Talks to Stripe. Knows you're a paying customer. Credits your wallet each month when the subscription renews. **Never sees your content or your usage detail.**

3. **The API (virtues-api)** — ours. Serves your requests — AI, maps, search, integrations — and counts down a prepaid budget. Knows a token has money left. **Never sees you.**

---

## The four parties and every credential (reference)

> A little more concrete than the rest of this page — the actual moving parts,
> for when you need them. Full detail in `docs/`.

**The parties — who knows what:**

| Party | Its job — and *only* this | Knows | Never sees |
|---|---|---|---|
| **Your home server** (VirtuesOS) | runs your stuff; the one place the link lives | everything (it's yours) | — |
| **Billing** (Atlas) | who pays, plus admin / support | customer, email, that you pay $20/mo | your usage token |
| **The API** (virtues-api) | serves requests, counts the budget down | an anonymous token has budget left | who you are |

Billing and the API are the "two rooms, no shared cabinet"; the home server is
the only place they meet. (Your phone finds your home server by dialing the
global IPv6 baked into its pairing bundle — Virtues runs no lookup service for
it.)

**Where every credential lives:**

| Credential | Created by | Raw value lives on | Reference kept at | What it does | Never sees it |
|---|---|---|---|---|---|
| **Device api_key** | Billing, at signup (rotatable) | your home server | Billing *and* the API (both hashed) | proves "I'm paid" *and* spends the wallet — AI, maps, search | — |
| **Device key** *(pairing bearer)* | your box, at pairing | each of your devices | your box | your phone / laptop ↔ your own box (local) | all of Virtues' cloud |

> **Changed from the v2 design.** There used to be three cloud credentials — a
> billing token, a monthly voucher, and a separate usage bearer — precisely so
> that no one credential was known to both halves. Today there is **one**
> api_key, known to both, resolving to an `account_id` that both also hold. That
> collapse is the whole of what was given up; see
> [`entitlement.md` §1](entitlement.md#1-the-privacy-posture--read-this-first).

Two "bearers" people conflate, kept distinct:
- The **usage token** is *home server ↔ the API* — your anonymous "I'm a paying
  subscriber" pass that gates everything Virtues-operated.
- The **device key** is *your phone ↔ your own box* — pure local auth that
  **never leaves your house**; Virtues' cloud never sees it.

The wall in one line: **Billing holds `token ↔ customer`, the API holds
`bearer ↔ budget`, and the only place those two halves meet is your home
server.**

---

## The voucher — how the two halves *were* meant to stay apart

> **Not shipped.** This section describes the v2 design. What runs today is a
> single api_key and a wallet keyed on a shared `account_id`; renewal is a
> Stripe webhook that credits the wallet server-side, with the box not involved.

Every month, on the **same day for everyone** (so the timing reveals nothing), your home server quietly does three things:

1. Mints a brand-new, random usage token for the month.
2. Asks Billing: *"am I paid up?"* Billing checks, says yes, and hands back a **voucher** — a one-time "good for 30 days" ticket. Billing never sees the new token.
3. Redeems that voucher at the API, loading the month's budget onto the new token. The API never sees who you are.

The voucher is a relay baton. It passes from Billing to the API *through your home server* — and the instant it's spent, both sides forget it ever existed. The two halves touched a shared object for a moment and kept no record of it.

Because everyone's token rotates on the same day, even the *timing* of all this carries no fingerprint. You're one indistinguishable rotation among thousands.

*(The cohort-aligned month boundary did ship, and is still in the code — but it
now buys operational tidiness rather than privacy, since the shared
`account_id` already links the two sides.)*

---

## Why this needs no clever cryptography

> **Not shipped — and the conclusion has flipped.** The argument below is sound
> *given* the voucher model. Without it, separation is no longer doing the work,
> and blind signatures stop being the tool for "the opposite situation" and
> become exactly the tool we need. RFC 9474 is the v2 path.

People expect a privacy claim like this to rest on some exotic crypto. Ours doesn't, and that's the point. The whole guarantee comes from one structural fact:

**Issuance and redemption live in two separate places that share no storage.**

- **Billing issues the voucher** → it sees `customer + voucher`, and *never the usage token*.
- **The API redeems the voucher** → it sees `voucher + usage token`, and *never the customer*.

No single system ever holds both ends of the chain. There's nothing to "promise not to correlate," because no one is in a position to correlate it in the first place. Separation does the work that secrecy would otherwise have to.

(For the technically curious: anonymous-token schemes like blind signatures exist precisely for the *opposite* situation — when one party must both issue and redeem and therefore has to blind itself from the link. We don't have that problem, so we don't reach for that tool. Architecture beats cryptography when you can arrange for the link to simply not exist on any one desk.)

---

## By construction, not by promise

This is the line that matters, and we mean it literally:

- We don't have a "usage log we promise not to read." **There is no usage log.** We keep counters — a number that goes down — never a list of what you did.
- We don't have a "customer-to-activity table we promise not to join." **There is no shared key to join on.** Billing's records and the API's records have no field in common.
- We don't *decline* to link your identity to your behavior. **We are unable to.** The only copy of that link lives on hardware we don't own and can't reach.

A subpoena to Billing yields: *"This customer pays us $20/month."* Nothing else.
A subpoena to the API yields: *"Some token has $4.40 left this month."* Nothing else.
A subpoena to both yields: *those two facts, still unjoinable.*

---

## What we honestly give up (and why it's fine)

We're not going to pretend this is free. The architecture costs us two conveniences, and we chose privacy over both:

- **No instant "off switch" per person.** If you cancel, your current month finishes and then simply doesn't renew — the prepaid time runs out. We can't reach into your token and kill it mid-month, because we don't know which token is yours. (This is how every prepaid thing works — a transit card, a gift card.)
- **We can't claw back on a disputed charge.** If a payment reverses, we can't find the token to drain it. The loss is capped at one month, and the hardware/subscription costs far more than a month of abuse is worth. Fair trade.

Neither of these is something a normal, honest customer ever notices. They're the price of a guarantee that holds even when we're not in the room.

---

## Thematic motifs (use these consistently)

These are the recurring images. Reach for them in copy, docs, support replies, talks:

- **"The link lives in your house."** The single source of truth for who-does-what is the user's own server. Always bring it back to this.
- **"By construction, not by promise."** Whenever we state a privacy property, frame it as architecturally impossible, not policy-restrained.
- **"Counters, not logs."** We track a number going down, never a history of events.
- **"Two rooms, no shared cabinet."** Billing and the API are separate by design.
- **"A baton, not a record."** The voucher passes through and is forgotten. *(v2 — see the banner at the top of this page.)*
- **"One rotation among thousands."** Cohort timing erases the fingerprint.
- **"Nothing to hand over."** The subpoena test — when compelled, we have nothing that ties you to your usage.

Avoid: "zero-knowledge" (that's a specific cryptographic term we don't use), "anonymous" without qualification (issuance is unlinkable; we still see per-token usage at the gate — be precise), "military-grade," "unhackable." Overclaiming poisons the well.

---

## Marketing sentences (FAQ-ready, lift verbatim)

**Q: Do you know what I use Virtues AI for?**
> No. Our API sees an anonymous prepaid token spending its budget — never your name, email, or account. The only place your identity and your usage are connected is your own server, at home.

**Q: Could you be forced to hand over my activity?**
> There's nothing to hand over. Our billing system knows you pay us; our API knows a token has budget left. Neither database shares a single field with the other, so there's no way to join them — not by us, not by a court, not by a future owner of the company.

**Q: How is this different from "we don't sell your data" promises?**
> A promise can be broken, subpoenaed, or sold with the company. We built this so the link between you and your usage doesn't exist on our side at all. It's a property of the architecture, not a line in a policy.

**Q: What *can* you see?**
> Your email, your payment method, and that you're a paying customer — the same things any store knows. On the usage side: that some anonymous token spent some budget. That's the complete list.

**Q: Do I have to use Virtues' servers at all?**
> No. You can run VirtuesOS on your own hardware and pay only for the API conveniences you want — or bring your own keys and use none of ours. The choice is yours; the privacy guarantee is the same either way.

**Q: What happens if I cancel?**
> Your current month finishes, then it simply stops renewing — like a prepaid card running out. We don't reach into your device to switch anything off, because we can't find your token to begin with.

**Q: Is it open source? Can I verify this?**
> Yes. Both billing and the API are open source. You can read the exact code that handles your subscription and your usage, and the lint rules that enforce the two halves never share a key.

---

## How to tell a non-technical person (the 20-second version)

> "Virtues works like a prepaid phone, but split in two halves that can't talk
> to each other. One half knows you paid. The other half serves what you ask
> for and watches a balance tick down. Neither half knows the other exists.
> The only thing that connects them is the little server sitting in your
> house — which is yours, not ours. So if anyone ever asks us what you've been
> doing, the honest answer is: we have no idea, and we built it that way on
> purpose."

---

## The standard we hold ourselves to

Every future feature gets measured against the one sentence at the top. If a
feature would require Billing and the API to share a key — a "usage dashboard
in your account," a "personalized recommendation from your history," a
referral program that tracks who invited whom — the answer is **no**, or we
find a way to do it that keeps the halves apart. We will refuse features to
keep this true. That refusal *is* the product.
