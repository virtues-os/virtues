# Privacy Policy — DRAFT

> **DRAFT — requires legal counsel review before publishing.** This captures how
> the system *actually works* (which counsel can't infer) plus standard scaffolding.
> Bracketed `[…]` items need a decision or a lawyer. Last updated: 2026-07-01.

## The short version

Your personal data — notes, location, health, messages, everything Virtues
collects about your life — lives on **your own box** (the appliance in your home),
not on our servers. We can't see it. When you reach your box from away, your
traffic passes through our **blind relay**, which forwards **end-to-end-encrypted
bytes it cannot decrypt**. What we hold in the cloud is small and boring: enough
to know who your account is and to take payment.

## Who we are

Virtues is a self-hosted personal-data appliance. [Legal entity name, address,
jurisdiction, contact email.] For privacy questions: [privacy@virtues.…].

## What lives where

**On your box (not us):** your actual data and the private keys that encrypt it.
The box is the source of truth. We do not have a copy and cannot retrieve it.
Deleting your account does not delete your box's data — that's yours to keep or
erase locally.

**With us (the cloud side — "atlas"):**
- **Account identity:** your email address and an opaque account identifier.
- **Subscription & wallet:** your plan status and prepaid balance/ledger.
- **Payments:** processed by **Stripe**; we receive confirmation and metadata but
  **do not store full card numbers**. See Stripe's privacy policy.
- **Aggregate usage:** coarse totals (e.g. AI spend, relay bytes) for billing and
  abuse prevention — numbers, not records of what you did.

**Passing through the relay (we cannot read):** to move traffic between a browser
and your box, our relay sees only (a) the destination name (the SNI — a per-box
hostname) and (b) byte volumes. It **terminates no encryption** (your box holds
its own TLS key), keeps **no logs of connections**, and holds **no database** —
it runs in memory only, so a reboot leaves nothing behind. It cannot associate
traffic with your identity, and it cannot see content. This is a structural
property, not a promise: the relay has no key to decrypt with.

## AI requests

When you use AI features, your request may be proxied through our AI gateway to
third-party model providers [list: Anthropic, …] to generate a response, and we
meter token/cost for billing. [If BYO-key is offered: requests using your own
provider key bypass our gateway.] [State retention: we do not retain prompt/
completion content beyond what's needed to fulfil the request and bill it —
CONFIRM with product + counsel.] Providers process content under their own terms.

## What we do **not** do

- We do not read, scan, or monetize the contents of your data or your traffic.
- We do not sell personal data.
- We do not build advertising profiles.
- The relay does not log who connects to what, or retain traffic.

## Legal bases / your rights

[GDPR/CCPA sections — lawful bases, access/erasure/portability, DPO/representative,
data-transfer mechanism for Stripe/model providers. Note: most "your data" rights
are satisfied at the box, which you control directly.]

## Retention

- Account + billing records: [retention period, e.g. duration of account + N years
  for tax/legal].
- Relay traffic: not retained (RAM-only).
- Aggregate usage counters: [period].

## Security

TLS terminates on your box with a key we never hold. The relay is blind by
construction. Cloud secrets (billing keys, the relay signing secret) are held
server-side and never exposed to your box or your browser. [Breach-notification
commitment.]

## Changes & contact

[Change-notice process.] Questions: [privacy@virtues.…].
