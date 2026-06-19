# Virtues docs

Reference and design docs for the Virtues home-server appliance. Keep this index
in sync when you add, rename, or retire a doc — orphaned files rot fastest.

## Architecture & system model

- [architecture.md](architecture.md) — the action system: `function` / `service` /
  `view` runtimes, manifest↔SQL field ownership, dispatch + reconcile. The
  implementation contract. Authoring how-to lives in
  [`../actions/AUTHORING.md`](../actions/AUTHORING.md).
- [virtues-api.md](virtues-api.md) — the *why* behind the privacy architecture:
  the two-room (Billing / API) split, vouchers, "the link lives in your house."
  Philosophy and copy, not a spec.
- [entitlement.md](entitlement.md) — technical spec for the voucher / entitlement
  system: bearer ↔ budget, the billing/usage wall, blocklist.
- [auth-model.md](auth-model.md) — pair-only auth for the home server: device
  pairing, sessions, sudo gates.

## Networking & remote access

- [networking.md](networking.md) — **source of truth** for how you reach your
  box: the IPv6-direct doctrine, the pinhole, `virtues doctor`, and the honest
  boundary.
- [byo-networking.md](byo-networking.md) — bring-your-own-transport recipes
  (Tailscale / Headscale / plain-WG-VPS / Cloudflare / Tor / dynamic-DNS+IPv6).
- [deployment.md](deployment.md) — the two shipping shapes (native Linux binary on
  the home box, Docker on EC2 for atlas + api), systemd privilege split.

## Operations

- [recovery.md](recovery.md) — operator runbook: reaching the UI, lost-session
  recovery, backup/restore, upgrade rollback, diagnostic beacons.

## Product / feature specs

- [the-day.md](the-day.md) — design spec for the Day Page (life-mirror view:
  events, autobiography, novelty scoring, alignment).
- [things.md](things.md) — "Things" feature: foldered collections, pins, AI memos.
- [codemirror.md](codemirror.md) — CodeMirror 6 page editor: Yjs CRDT sync,
  live-preview decorations, media widgets, entity links.
