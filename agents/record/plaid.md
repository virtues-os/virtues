# Plaid

**Written 2026-09-29.** How bank connections work through the cloud proxy, why
Plaid and FinanceKit never need reconciling, and the single-use-token bug that
killed every connect for two weeks. Hosted Link rebuilt 2026-07-24; exchange
fix 2026-09-16. Supersedes `agents/plan/plaid-plan.md` (deleted). The code is
the Plaid Hosted Link section of `services/virtues-api/src/routes/oauth.rs`.

---

## Plaid is the general collector

FinanceKit needs an iPhone and reaches only what Apple covers. Plaid is the
path that has to work for everyone else, so it is not a nice-to-have beside
FinanceKit.

## What was wrong before the rebuild

As of 2026-07-24 Plaid had never once connected: zero occurrences of "plaid"
in the lab box's unrotated journal from Jul 8 to Jul 24, no Plaid credential,
no Plaid applet run. The old flow launched the *self-hosted* Link URL with a
top-level `redirect_uri`, and the callback read only `public_token` — no
`oauth_state_id`, no resume leg, although a comment pointed at one. Any OAuth
bank, which is most large US banks, died on the return.

## Hosted Link, and the two facts that shape it

Plaid is not an OAuth provider. With Hosted Link, Plaid hosts the whole flow,
each bank's own OAuth round trip included, then redirects the browser to our
`completion_redirect_uri`.

1. **The completion redirect carries no result** — not the public token, not
   even success versus exit. The outcome comes from `/link/token/get` with the
   link token, which only `/plaid/start` ever saw.
2. **The completion URI is matched exactly** against the one registered in
   Plaid's dashboard, so it cannot carry a per-session query parameter.

Hence `plaid_link_session` (proxy migration 0007): start parks the link token,
return URL and box state, and hands the browser a first-party `SameSite=Lax`
cookie scoped to `/plaid`; the callback reads the cookie, polls Plaid, and
bounces back to the box like the OAuth providers do. **No top-level
`redirect_uri`**: that is the self-hosted contract, where our own page must
relaunch Link with `receivedRedirectUri`, and mixing the two is what hung the
old flow. The session, URL and cookie share one 30-minute lifetime, Plaid's
own default.

**Cancel is the ordinary path, and it looks like drift.** Plaid fires the
completion redirect whether the person linked an account or backed out, so
"no public token" is a cancel. It is also exactly what a renamed response
field would look like. The callback logs the session's key set — keys only,
since these payloads carry tokens — so one live attempt tells the two apart.
The person sees `connect_cancelled` either way.

The institution name is resolved and stored, so a credential is named after
the bank rather than "Plaid account".

## The exchange_sig incident

On 2026-09-03 (384754b2) the proxy's OAuth state became a server-side
`oauth_session` row, and the exchange token it hands a box became single-use:
the callback records the token's signature in `exchange_sig`, and `/exchange`
burns it by that signature. The authorize-code providers already had their
session row open at the callback and only updated it. Plaid's session lives in
a different table, `plaid_link_session`, which is deleted as it is read — so
**Plaid never wrote `oauth_session.exchange_sig`**, `/exchange` matched
nothing, and every Plaid connect since 2026-09-03 died at the box with
"exchange_token already used or unknown", after the Plaid credential had
already been minted and was now unreachable.

Fixed 2026-09-16 (86401734): the Plaid callback inserts a finished
`oauth_session` row carrying the signature before the browser sees the token,
and bounces as a failure if it cannot, rather than handing over a dead token.
A test asserts a Plaid exchange token is spendable exactly once.

**The failure class:** a single-use ledger keyed on one table, while a second
flow mints tokens without ever writing to it. When a guard is added to a
shared door, enumerate every path that mints what the door checks.

## Multi-collector reconciliation: not needed

The fear was that a bank covered by both collectors would appear twice, since
Plaid and FinanceKit write the same tables in disjoint namespaces (`plaid:*`,
`apple_finance:*`). That overlap cannot happen, by construction:

- **FinanceKit returns only Apple Card, Apple Cash and Apple Savings**
  (US-only). It is an Apple-products API, not an aggregator.
- **Plaid cannot reach Apple's products**; they are not connectable
  institutions.
- The lab box confirmed it on 2026-07-24: `data_financial_account` held three
  rows, all `source_provider = apple_finance`, all institution "Apple".

The design also failed on its own terms: FinanceKit's stored payload carries
four keys (`name`, `currencyCode`, `institutionName`, `id`) — no mask, no
last four, no account number, and `account_type` is `"other"` for all three
rows — so the proposed join key (institution + mask + type + currency) was not
derivable.

Revisit only if a genuinely overlapping pair of collectors appears: a second
aggregator, CSV/OFX import, or manual entry. The narrower real risk is linking
the same institution twice through Plaid, which yields two Items with
different `account_id`s. That is a connect-time warning on an `institution_id`
an active credential already holds, not an ownership layer.

**Standing non-goal:** never fuzzy-match transactions across providers (equal
amounts, dates a day apart for pending versus posted, differing merchant
strings). That is the semantic entity resolution the deterministic covenant
removed.

## Traps

- **`optional_products: ["investments", "liabilities"]` breaks every
  connect** on a Plaid account not enabled for them: `/link/token/create`
  rejects the whole request with `INVALID_PRODUCT`. A test asserts the body
  omits it.
- **`client_user_id` is one constant for the whole fleet**, so every box looks
  like one user in Plaid's dashboard. `/plaid/start` is an unauthenticated
  browser GET with no identity to give.
- **Proxy env changes need a container recreate**; a restart does not re-read
  `--env-file`.
- **Order of rollout:** a new box on an old proxy is fully broken, since Hosted
  Link lives in the proxy. Ship the proxy first.
