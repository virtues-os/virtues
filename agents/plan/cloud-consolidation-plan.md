# Cloud consolidation — what is left

atlas and virtues-api moved onto one dedicated server with its own Postgres
on 2026-09-28 (53 seconds of downtime), with encrypted backups archiving off
the server and a restore tested before any real data landed. How it runs now
is in [deployment.md](../build/deployment.md#cloud-sidecar-atlas--virtues-api-on-one-server)
(rewritten dd27b61b). The relay stays on its own server. Delete this plan when
the list below is empty.

## Open

1. **Run the maps.** The cut job and the virtues-api routes are built
   ([offline-maps-plan.md](offline-maps-plan.md)); on the server:
   install the cut per `deploy/maps/README.md`, run the first cut (~140 GB
   planet download, ~175 GB per build), mount `/srv/maps` read-only into
   virtues-api with `VIRTUES_MAPS_DIR=/srv/maps`, keep Caddy from logging the
   maps routes, then watch a real box's first sync.
2. **Move the review demo** (still the `virtues-review-demo` instance on the
   old hosting) onto the new server.
3. **Decide where the website's database lives.**
4. **Shut down the old hosting** once the rollback week is over (from
   2026-10-05). It was meant to be stopped for that week, but on 2026-09-29
   `aws ec2 describe-instances` showed the `virtues` instance running and
   `aws rds describe-db-instances` showed `virtues-atlas-db` and
   `virtues-api-db` available. Confirm nothing still writes to them, stop
   them now, and delete after the week (ask first).
