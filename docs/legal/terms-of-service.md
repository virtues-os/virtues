# Terms of Service — DRAFT

> **DRAFT — requires legal counsel review before publishing.** Captures the real
> service shape + acceptable-use and payment terms; bracketed `[…]` items need a
> decision or a lawyer. Last updated: 2026-07-01.

## 1. What Virtues is

Virtues is a **self-hosted personal-data appliance**. You run a box that stores
your data locally and holds its own keys. We provide cloud services around it:
**identity + billing** ("atlas"), an optional **AI gateway**, and a **blind relay**
that lets you reach your box from anywhere without opening a port at home. Your
data lives on your box; these Terms govern the **cloud services**, not your data.

## 2. Accounts & eligibility

[Age/eligibility. One account per customer. You're responsible for your account
credentials and for the security/lawful operation of your own box.]

## 3. Payment, wallet & subscription

- **Subscription:** [$20/month] for access to the cloud services (relay reach, AI
  gateway, updates). Billing via Stripe.
- **Prepaid wallet:** AI and metered usage draw from a prepaid balance (you add
  funds; optional auto-top-up). Balances are **credit for services, not cash**,
  and are [non-refundable except as required by law / per §Refunds].
- **Lapse:** if a subscription lapses, cloud services (including relay reach) stop;
  **your box and its data keep working locally.** [Grace period.]
- [Refunds, chargebacks, price-change notice, taxes.]

## 4. Acceptable use

You agree not to use the cloud services to:
- transmit unlawful content, or infringe others' rights;
- attack, overload, or attempt to circumvent limits on the relay or other
  infrastructure (e.g. flooding, evading per-box connection limits);
- resell or proxy the relay for third-party traffic unrelated to your box;
- [other: malware distribution, CSAM (zero tolerance, reported per law), etc.].

Because the relay is blind, we enforce this at the **edges** (payment-gated access,
per-box rate limits, and account revocation) — **not by inspecting traffic**, which
we cannot do.

## 5. Our role re: traffic (mere conduit)

The relay transmits **end-to-end-encrypted traffic it cannot read, select, or
modify**, automatically and at your direction. We act as a **conduit** [DMCA
§512(a) / equivalent safe-harbor language — counsel to finalize, incl. designated
agent + repeat-infringer policy]. We do not monitor content and are not
responsible for it; we may suspend or terminate accounts for violations of §4 or
legal process.

## 6. Availability & DR

We aim to keep the relay available but do not guarantee uptime. The relay is a
single dependency for *remote* access; if it is down, your box remains reachable
on your local network and keeps collecting data. [SLA/credits: none for v1, or
specify.]

## 7. Software & updates

[License to the box software / OSS components, update mechanism, no obligation to
support indefinitely, EOL policy.]

## 8. Disclaimers & liability

[Services provided "as is"; disclaimer of warranties; limitation of liability
capped at [amount / fees paid]; carve-outs as required by law.]

## 9. Termination

Either party may terminate [notice]. On termination, cloud services stop; your box
and local data are unaffected and remain yours.

## 10. Changes, governing law, disputes

[Change-notice process; governing law + venue; dispute resolution/arbitration;
contact.]
