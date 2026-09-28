# Cloud consolidation: our hosted services onto one server

**Status: cut over 2026-09-28 (53 seconds of downtime).** atlas and
virtues-api run on the new server against its own Postgres, with backups
archiving off the server and a restore tested before any real data landed.
The old hosting is stopped but kept for the rollback week. Left: the maps
deploy (step 7), moving the review demo, deciding where the website's database
lives, then shutting the old hosting down (step 8) and updating the deploy
runbook. When that is done, delete this plan.

## Why

- **Maps.** The box's map files (agents/plan/offline-maps-plan.md) are
  gigabytes per box, and the current cloud charges per GB sent. The new server
  has unmetered bandwidth, so maps become one more virtues-api route:
  `BearerAuth`, streamed with `Range`, logging nothing.
- **One place, more room.** atlas, virtues-api, their databases, the website's
  database and the review demo run on several small managed instances today.
  One dedicated server holds all of them with room to spare.

The relay stays on its own server, so map downloads and deploys never touch
remote access.

## What moves

| Piece | After |
|---|---|
| atlas (billing) | container on the new server |
| virtues-api (AI proxy, usage, maps) | container on the new server |
| atlas, virtues-api and website databases | Postgres on the new server, local socket |
| Review demo | on the new server |
| TLS | Caddy on the new server |
| DNS | stays where it is; only the A/AAAA records change |
| Relay | stays on its own server |

Neither service calls a cloud-provider API (checked 2026-09-27: atlas depends
on Stripe, Resend, the relay and virtues-api; virtues-api on the AI gateway,
Google, Plaid and Unsplash). The env files move as they are.

## Steps

1. **Provision.** OS install with an SSH key only. Firewall: 22, 80, 443.
   Unattended security upgrades. Docker, Caddy, Postgres (the major version
   production runs today).
2. **Backups before data.** Nightly `pg_dump` of every database plus
   continuous WAL archiving to object storage off the server, encrypted with
   `age`. Test a restore before cutover.
3. **Images.** virtues-api from GHCR (CI already pushes it); atlas added to the
   same build.
4. **Rehearse.** Restore copies of the databases, run the containers against
   them on hidden hostnames, and point a dev box at them: link, billing page,
   a chat turn, a map download.
5. **Cut over**, with Adam, in a quiet hour:
   - stop writes on the old hosting;
   - take a final dump and restore it on the new server;
   - start the containers;
   - switch `atlas.virtues.com` and `api.virtues.com` (TTL lowered beforehand);
   - check Stripe webhooks arrive (Stripe retries, so a gap is safe), `/health`
     on both, a real box's chat turn and relay authorize.
6. **Rollback window.** Leave the old hosting stopped but intact for a week.
   Rolling back is starting it and switching DNS back.
7. **Maps on virtues-api.** The monthly cut job as a systemd timer writing
   `/srv/maps/<build>/`; `GET /v1/maps/index` and `GET /v1/maps/<build>/<file>`
   behind `BearerAuth`, excluded from request tracing.
8. **Shut down** the old hosting and update the deploy runbook.

## Risks

- **Self-managed Postgres** replaces managed backups: step 2 has to be real,
  with a tested restore, before any production data lands.
- **One server** carries billing, AI and maps. Rebuild time from backups is the
  recovery objective; write the runbook while provisioning, not after.
- **Live users.** A missed env var shows up as a slow failure on real boxes.
  Rehearse against copies first (step 4), and keep the rollback week.
