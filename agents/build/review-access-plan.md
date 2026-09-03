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

`VIRTUES_REVIEW_PAIR_CODE` in **`/var/lib/virtues/virtues.env`** — the file
the unit actually reads (`EnvironmentFile=-/var/lib/virtues/virtues.env`), and
the one the installer writes. This doc said `/etc/virtues/env` until 2026-09-03;
on a real box that directory exists and is **empty**, so the variable goes into
a file nothing loads, no row is installed, and the only symptom is a reviewer
who cannot pair. At startup `api::pair::ensure_review_code` installs one
`app_pair_token` row with `kind = 'review'`, `status = 'authorized'`, and a
nominal 10-year expiry.

Nothing else changes. `claim_pair_token` already accepts any authorized,
unexpired row and only consumes `kind = 'oneoff'`, so a review code is
multi-use and permanent for free. The `'review'` value in the `kind` CHECK
constraint arrived as `0058_review_pair_code.sql`, which folded into
`0001_initial.sql` in the 2026-08-18 squash — there is no separate migration to
look for any more, and no release since carries the one without the other.

The code stays **6 digits** because the mobile pairing input is
`inputmode="numeric"` with `maxlength="7"` (`src-tauri/ui/connect.html`). A
longer, higher-entropy token would be untypeable there. Startup refuses
anything that is not exactly 6 digits rather than installing a code a reviewer
cannot enter.

### Why the env gate is the whole safety story

A 1M keyspace on a public origin is acceptable only because two things hold at
once. First, `/api/pair/consume` is rate-limited to **10 attempts per IP per
30-minute window**, which puts a single-source sweep of the space far out of
reach. Second, such a box holds synthetic seed data — a successful guess exposes
a fake life. Absent the env var no review row is ever created, so customer boxes
cannot grow a permanent remote-pairing credential. **Never set this variable on a
box holding a real person's data.** An active review code logs a warning on every
boot for exactly this reason.

Residual risks on the demo box, all bounded and accepted: the rate limit is
per-IP, so a distributed sweep is slowed rather than stopped; a successful guess
earns a permanent iroh allowlist entry, could burn inference credits via
`virtues-api`, and would see anything the reviewer's device synced.

**Rotate the code whenever the box's address becomes known outside App Review**
— publishing the two together is what turns a slow, bounded guess into a targeted
one. Rotation is a one-line env change plus a restart; see
[Between review rounds](#between-review-rounds).

## Current demo box

**The live box's address, instance ID, security-group ID, and current pair code
are deliberately not in this repo.** They live in the private ops note alongside
the App Review submission record. This file describes the *shape* of the box, not
its coordinates.

| | |
|---|---|
| Instance | t4g.medium (4 GB, arm64), 30 GB gp3 |
| Address | Elastic IP + a random `demo-<rand>.virtues.ch` Route 53 record |
| Security group | 80/443 public, no SSH |
| Access | SSM via an instance profile — no key pair |
| Cost | ~$27/mo running, ~$6/mo stopped |

An obscure hostname is not a security control — it only keeps opportunistic
scanners away, and it does even that only while it stays unpublished. The real
controls are the per-IP rate limit on `/api/pair/consume` (10 attempts per
30-minute window, keyed on the proxy-appended XFF entry — see
`ensure_review_code` and `consume_handler` in `virtues-core/src/api/pair.rs`),
the synthetic seed data, and the box's disposability.

## Provisioning

1. Launch t4g.medium (arm64 — both `x86_64` and `aarch64` are built), 30 GB
   gp3, the SSM-enabled EC2 instance profile, SG with 80/443 open and 22
   closed. Allocate an Elastic IP so DNS survives stop/start.
2. Route 53 A record → the EIP.
3. Caddy in front: `demo-<rand>.virtues.ch { reverse_proxy 127.0.0.1:8000 }`.
   Port 80 must stay open for the ACME http-01 challenge.
4. 4 GB swap. 4 GB RAM is enough at rest (Postgres + `virtues` + two Q8_0 CPU
   sidecars ≈ 2.5–3 GB) but the seed index build wants headroom. Slow is fine
   here; OOM is not.
5. Install `virtues` from any current release (`gh release list`), pinning it
   with `VIRTUES_VERSION=vX.Y.Z`. Over SSM there is no TTY, and the installer
   asks two questions — so both answers have to arrive as environment:

   ```sh
   curl -fsSL https://virtues.com/sh \
     | VIRTUES_VERSION=vX.Y.Z VIRTUES_INFERENCE=bundled sh -s -- --no-init
   ```

   `--no-init` skips only the interactive pairing handoff; the service is
   enabled and started before that step regardless. `VIRTUES_INFERENCE=bundled`
   is the one that is easy to miss: without it the installer reaches the
   Inference step, finds neither our hardware nor a terminal to ask on, and
   **exits 0** having installed nothing — a failure that reads as success in a
   log you are only skimming.
6. `/var/lib/virtues/virtues.env` (NOT `/etc/virtues/env` — see above):
   `VIRTUES_PUBLIC_URL` + `VIRTUES_REVIEW_PAIR_CODE`. Draw the code randomly
   per round (`shuf -i 100000-999999 -n 1`, on the box, so it never rides in as
   a command parameter) and record it in the private ops note — never in this
   repo, a commit message, or an issue. Then `systemctl restart virtues`.
7. `virtues seed` — the 12-week narrative plus the instrumented demo day, so
   the reviewer sees a life rather than an empty shell.
8. ~~Subscribe the box's account.~~ **No longer a step** — kept at its old
   number because it is the one everybody's notes still say to do. Atlas used to
   issue a `relay_url` only to a subscribed box, so without it the reviewer
   paired and then lost the box the moment they left the network they paired
   from. The open-relay work deleted that coupling on both sides:
   `relay::DEFAULT_RELAY_URL` is compiled into the box ("so a box that never
   signs in is still reachable from its first boot", gated only on the
   box-install marker), and `services/virtues-atlas/src/routes/relay.rs`
   resolves the config "with no subscription requirement — reachability is part
   of ownership, not the subscription". A review box needs no atlas account, no
   claim, and no card.
9. Confirm `REVIEW PAIR CODE ACTIVE` in the boot log — that is the proof the
   row installed. A missing env var fails silently and looks like success.
10. Test-pair a real phone **over cellular**, not Wi-Fi. Wi-Fi would pass via
    the LAN path and prove nothing about the reviewer's experience.

Models: chat routes to `virtues-api`, so no local LLM is needed. Embeddings and
the reranker do run locally, CPU-only, and slowness is acceptable.

## What the first real run found (2026-09-03)

The box described above was launched 2026-07-21 and **never actually brought
up** — Caddy and the binary were installed, then it was stopped the same day.
`/etc/virtues/` was empty, there was no systemd unit, and the `virtues`
database had no tables at all, not even `_sqlx_migrations`. So no review round
has ever exercised this path, and every iOS submission since July went out with
review notes pointing at a box that was switched off. Assume nothing here has
been tested until you have tested it.

Three things broke on the way to a working box, all of them fixed in the same
change as this note:

- **`virtues seed` was dead.** `demo_narrative.sql` still inserted
  `wiki_days.morning_baseline`, a column migration 0011 dropped. `raw_sql` runs
  a file as one unit, so the whole 12-week narrative and the bookmarks silently
  failed and only `demo_day.sql` landed. Every developer who seeded since that
  migration got a third of the data.
- **The bundled inference sidecars could not start.** `llama-server` links
  `libgomp`, which a minimal Ubuntu does not carry; the installer never
  installed it, and the install still reported success.
- **The seed was frozen in February** — see `seeds/demo_reanchor.sql`, which now
  moves the instrumented day onto today at seed time.

None of these are visible from a green CI run, and two of them present as
success. Budget for a full bring-up, not a checklist tick.

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
