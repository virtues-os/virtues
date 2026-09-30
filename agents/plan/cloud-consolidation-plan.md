# Cloud consolidation — what is left

atlas and virtues-api moved onto one dedicated server with its own Postgres
on 2026-09-28 (53 seconds of downtime), with encrypted backups archiving off
the server and a restore tested before any real data landed. How it runs now
is in [deployment.md](../build/deployment.md#cloud-sidecar-atlas--virtues-api-on-one-server)
(rewritten dd27b61b). The relay stays on its own server. Delete this plan when
the list below is empty.

Verified on 2026-09-30: both containers healthy with no errors logged, RAID
and SMART clean, daily backups and WAL archiving current, no health alert has
fired, and the first map build is published and served.

## Open

1. **Map fonts.** The first build (2026-09-28) is published, virtues-api
   serves it from `/v1/maps/*` behind the bearer, Caddy keeps no access log
   and the routes sit outside request tracing, and a dev box synced its world
   file and squares on 2026-09-29. But the build's `assets.tar` held 252
   symlinks (font aliases in the upstream assets repo), and the box refuses
   any link in that archive, so no box has fonts or sprites yet. `cut.py` now
   dereferences them: install it on the server before the next run (the timer
   fires early each month), then confirm a box logs its assets unpacked.
2. **Deploy atlas through `make deploy-atlas`.** Production atlas runs an
   image from early September. Since then its code has changed by one fix
   (f1ae1f25: charge refunds no longer touch subscription status) plus copy
   and Dockerfile changes. Neither deploy target has been run end to end yet;
   this is the first. The script tags the running image as `:previous` before
   the swap, so the first deploy creates atlas's rollback image. No
   subscription was ever set to `refunded` by the old handler (checked
   2026-09-30).
3. **Move the review demo** (still on the old hosting). Not onto this
   server: a box install beside billing means a second Postgres, a port
   clash, and dev-auth owner access next to the billing service. It needs its
   own VM.
4. **Decide where the website's database lives.** It is still on the old
   hosting, still connected to occasionally, and reachable on its port from
   any address because the website's host has no fixed IPs. The real fix is
   the website not talking to a database directly.
5. **Shut down the old hosting** after the rollback week (from 2026-10-05).
   At cutover only the service containers were stopped; the instance and the
   atlas and api databases were deliberately kept running so a rollback
   could be a DNS change and a container start. On 2026-09-30 the containers
   were still stopped and neither database had had a connection since the
   cutover. After the week, per resource: take a final snapshot, stop, then
   delete once the snapshot is confirmed (ask first).
