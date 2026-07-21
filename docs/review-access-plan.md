# App Store review access

How an Apple reviewer — who owns no box and cannot reach yours — exercises the
iOS app well enough to clear guideline 2.1.

## The problem

Pairing is limited to the LAN, and not by any check we could relax:

- `/api/pair/consume` is plain HTTP to the box's own origin. The box has no TLS
  surface and no public inbound port (`virtues-core/src/server/mod.rs`).
- The iroh relay cannot carry the pairing step: `virtues-iroh/src/server.rs`
  rejects connections from peers that are not already allowlisted, and pairing
  is *how* a peer gets allowlisted. The relay ticket is the output of consume,
  not an input.

There is no co-location logic to remove. `consume_handler` performs no IP or
subnet validation, and the client's `normalize_server`
(`apps/web/plugins/reach/src/lib.rs`) accepts any `http(s)` origin. "Same
Wi-Fi" is UI copy, not enforcement. So a box that *is* publicly reachable
already pairs from anywhere — the missing piece was only a code that lives long
enough for a review cycle.

Existing code lifetimes are far too short: `oneoff` is 5–30 min, `standing`
rotates every ~20 min. A reviewer may open the app days after submission, and
resubmissions add rounds.

## The mechanism

`VIRTUES_REVIEW_PAIR_CODE` in `/etc/virtues/env`. At startup
`api::pair::ensure_review_code` installs one `app_pair_token` row with
`kind = 'review'`, `status = 'authorized'`, and a nominal 10-year expiry.

Nothing else changes. `claim_pair_token` already accepts any authorized,
unexpired row and only consumes `kind = 'oneoff'`, so a review code is
multi-use and permanent for free. Migration `0058_review_pair_code.sql` only
widens a CHECK constraint; it creates no row.

The code stays **6 digits** because the mobile pairing input is
`inputmode="numeric"` with `maxlength="7"` (`src-tauri/ui/mobile-pair.html`). A
longer, higher-entropy token would be untypeable there. Startup refuses
anything that is not exactly 6 digits rather than installing a code a reviewer
cannot enter.

### Why the env gate is the whole safety story

A 1M keyspace on a public origin is only acceptable because such a box holds
synthetic seed data — a successful guess exposes a fake life. Absent the env
var no review row is ever created, so customer boxes cannot grow a permanent
remote-pairing credential. **Never set this variable on a box holding a real
person's data.** An active review code logs a warning on every boot for exactly
this reason.

Residual risks on the demo box, all bounded and accepted: a successful guess
earns a permanent iroh allowlist entry, could burn inference credits via
`virtues-api`, and would see anything the reviewer's device synced.

## Current demo box

| | |
|---|---|
| Instance | `i-04e515dab909e10ef` (t4g.medium, 4 GB, arm64, us-east-1c) |
| Elastic IP | `52.205.15.22` |
| URL | https://demo-d5873b.virtues.ch |
| Security group | `sg-0e8503ced8f9a1a4e` — 80/443 public, no SSH |
| Access | SSM via `virtues-ec2-profile` |
| Cost | ~$27/mo running, ~$6/mo stopped |

An obscure hostname is not a security control — it only keeps opportunistic
scanners away. The controls are the synthetic data and the disposability.

## Provisioning

1. Launch t4g.medium (arm64 — both `x86_64` and `aarch64` are built), 30 GB
   gp3, instance profile `virtues-ec2-profile`, SG with 80/443 open and 22
   closed. Allocate an Elastic IP so DNS survives stop/start.
2. Route 53 A record → the EIP.
3. Caddy in front: `demo-<rand>.virtues.ch { reverse_proxy 127.0.0.1:8000 }`.
   Port 80 must stay open for the ACME http-01 challenge.
4. 4 GB swap. 4 GB RAM is enough at rest (Postgres + `virtues` + two Q8_0 CPU
   sidecars ≈ 2.5–3 GB) but the seed index build wants headroom. Slow is fine
   here; OOM is not.
5. Install `virtues` from a release that contains migration 0058.
6. `/etc/virtues/env`: `VIRTUES_PUBLIC_URL` + `VIRTUES_REVIEW_PAIR_CODE`.
7. `virtues seed` — the 12-week narrative plus the instrumented demo day, so
   the reviewer sees a life rather than an empty shell.
8. **Subscribe the box's account.** Atlas only issues a `relay_url` to a
   subscribed box, so without it the reviewer pairs and then loses the box the
   moment they leave the network they paired from.
9. Confirm `REVIEW PAIR CODE ACTIVE` in the boot log — that is the proof the
   row installed. A missing env var fails silently and looks like success.
10. Test-pair a real phone **over cellular**, not Wi-Fi. Wi-Fi would pass via
    the LAN path and prove nothing about the reviewer's experience.

Models: chat routes to `virtues-api`, so no local LLM is needed. Embeddings and
the reranker do run locally, CPU-only, and slowness is acceptable.

## Between review rounds

- Wipe the box and re-seed. The reviewer's own device data — health, location,
  contacts, ambient audio — syncs onto this box once paired, and it should not
  persist or bleed into the next round.
- **Stop**, don't terminate: the EBS volume, seed data, and review-code row all
  survive, and the EIP keeps DNS valid.
- Revoke by clearing the env var and restarting, which retires the row. Rotating
  the var to a new code does the same and installs the replacement.
- Watch `paired_from_ip` in the pairing audit log for anything unexpected.

## App Review notes

Give them the URL and the code, and say the app requires a paired box with the
demo instance standing in for one. Reviewers do not SSH anywhere or run
`virtues pair` — they type an address and six digits.

## Alternative not taken

An in-app demo mode (canned local data, no box, no pairing) would remove the
hosted-box dependency, survive review rounds without a babysat server, avoid
holding a reviewer's personal data, and double as a pre-purchase try-before-you-buy.
It is real product work, which is why the demo box came first. Worth revisiting
if review rounds become routine.
