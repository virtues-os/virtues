# Sources as packages — P5, `oauth_direct`

**Status:** Open, blocked on research, not code. P0–P4 shipped 2026-08-04; the
package model, the sweep that motivated it and the decisions that stand are in
[`../record/sources-packages.md`](../record/sources-packages.md).

Adding an OAuth provider still means a code change and deploy of virtues-api,
because the hosted proxy holds every provider's client credentials. P5 lets a
package declare a provider the box talks to directly.

## Why the change is small

The proxy attaches at exactly two seams — `proxy_exchange` and `proxy_refresh`,
each `(source_id, token) → one normalized token set`. Everything else (the
AES-256-GCM vault, state HMAC, `expires_at`/`next_refresh_at`, the JIT-refresh
mutex, the `credential_refresh` cron, the applet secret contract) is already
provider- and proxy-agnostic. `secrets` is `serde_json::Value` end to end, so a
`{client_id, client_secret, access_token, refresh_token}` blob stores with
**zero migration**, and `settings_byo.rs` is the precedent for "the user's own
credential as a synthetic credentials row."

- **The proxy stores client secrets; it does not store tokens.** The box
  already stores raw provider refresh tokens at rest; going direct adds only a
  client_secret.
- `oauth2 = "4.4"` is in `virtues-core/Cargo.toml` and **referenced by nothing**
  — an unused dependency that already provides PKCE support. Use it rather than
  hand-rolling, or remove it if P5 is dropped.

## Design: two implementations behind one seam

This adds a second implementation behind the existing seam instead of a
parallel OAuth flow. It needs one catalog variant, two functions and one match.

**Provider quirks are where this could get over-engineered.** The proxy special-cases
providers in four places (authorize params — Google's `access_type=offline`,
Strava's comma-separated scopes; token-request shape — Notion's HTTP Basic;
response and refresh normalization). A fully generic `oauth_direct` would need a
configuration language for OAuth dialects. Do not build that. Split by capability:

- **Proxy** — curated providers, quirks in code, zero user setup.
- **Direct** — standards-compliant RFC 6749 + PKCE only. `authorize_url`,
  `token_url`, `scopes`, and `auth_style` (basic vs body) as the only
  provider-specific option. A provider needing more belongs in the proxy.

Client credentials go in the same encrypted `secrets` blob as the tokens.
Redirect to `http://127.0.0.1:<port>/oauth/callback` under a desktop-app client
type, which makes the client_secret non-confidential by design.

## Spike

One provider, self-serve registration, loopback redirect. Get it working end to end,
then decide whether it generalizes. Prerequisites:

- a `SourceAuth::OauthDirect` variant;
- `direct_{start,exchange,refresh}` in `crates/virtues-helpers/src/auth`;
- the `msg.contains("upstream 4")` reauth detection turned into a typed error —
  two call sites depend on a substring of an error message
  (`virtues-helpers/src/auth/refresh.rs`, `applets/credential_refresh/main.rs`).

Independently worth doing: a capability endpoint on virtues-api listing
supported providers. Today the box and the proxy are coupled by convention and
can drift apart silently until a request returns 404.

**Relay Sybil resistance is unaffected.** It is anchored on the billing
relationship (account → wallet → api_key), not on the OAuth exchange. `oauth_direct` moves where
a provider's client credentials live; virtues-api remains the AI gateway,
wallet and relay control plane.

Until P5 lands, a package can declare a `via_proxy` source, but the proxy must
have a route and a registered app — self-service OAuth through the hosted proxy
stays gated.

## Open question

Which providers accept a loopback `redirect_uri` under self-serve registration?
The box lives at `.local` / `127.0.0.1`, and Google will not accept a `.local`
redirect; loopback is accepted for desktop-app client types. This decides
whether P5 generalizes or stays a one-provider workaround. That is a product
question about how much registration effort users will accept, not an
engineering one.

## Update policy

Shipped applets follow the box, so an auto-update toggle only ever applies to
third-party packages:

- **Built-ins:** no toggle. They move when the box moves.
- **Third-party:** default **notify, don't apply**. Per-package opt-in to follow
  a ref. The resolved SHA is persisted (P3), so "update available" is
  answerable.
- **Auto-follow only for face-only and agent-only packages, or jailed `command`
  packages.** Silently pulling and running unjailed native code against all
  of the owner's data must not be the default anywhere.

**Not yet:** treating the applet format (`SKILL.md`, agents/plan/applet-shape.md)
as a stable public API. It is public and tracked; breaking it starts costing
other people once third parties build on it.
