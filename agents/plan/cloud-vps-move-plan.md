# Moving the cloud services off the dedicated server

**Written 2026-11-19. Slices 1 and 2 are done; no server is ordered.**
atlas, virtues-api and
their Postgres run on one dedicated server since 2026-09-28
([record](../record/cloud-consolidation.md),
[how it runs](../build/deployment.md#cloud-sidecar-atlas--virtues-api-on-one-server)).
This plan moves them to a small VPS, puts the map files in object storage, and
cancels the dedicated server. Delete this plan when the server is cancelled;
what survives is a dated entry in the record and the updated runbook.

## Why move

The numbers below were read off the server on 2026-11-19.

| What | Size |
|---|---|
| virtues-api and virtues-atlas | 33 MB of memory together, CPU under 2% at the 95th percentile |
| Postgres | two databases, 30 MB; a full backup is 6 MB |
| Network out | 0.2 Mbps at the 95th percentile |
| Map files | 331 GB: two builds of about 165 GB each |
| Monthly map cut | about 140 GB of scratch, 51 minutes on 8 cores, memory capped at 8 GB |

The server has 8 cores, 64 GB of memory and 878 GB of disk. The services use
almost none of it. The map files are the only reason it needs a large disk,
and files that are written once a month and read by range are what object
storage is for. Two things are also missing today and this move supplies both:
a tested way to rebuild the server from nothing, and a copy of the two env
files anywhere but on it.

## Where each thing goes

| What | Today | After |
|---|---|---|
| atlas, virtues-api, Postgres, Caddy | dedicated server | one small VPS, same provider |
| Map files | `/srv/maps` on the server | a private object storage bucket in the VPS's region |
| Monthly map cut | a timer on the server | a temporary instance that exists only for the run |
| Database backups | object storage in another region | unchanged |
| Relay, intranet, review demo | one VPS each | unchanged |

The VPS size is set by the image build. `tools/deploy-service.sh` compiles the
Rust services on the server, which needs about 4 cores and 8 GB. Running them
needs far less.

## What must not change

- **Map downloads stay unlogged and stay between the box and our API.**
  `services/virtues-api/src/routes/maps.rs` says why: which squares a box takes
  is location data. So the API fetches each file from the bucket and streams
  it to the box. A read that reaches storage carries the API server's address
  and the square's name, and no box's address or key. Bucket access logging
  stays off. The limit of this: the storage provider also hosts the API
  server, so it could match the time of a box's connection to the time of a
  read. That is no more than it can do today from the size of an encrypted
  download, since every square has a different size. This is the property
  the data sovereign persona would check, and nothing a person sees changes:
  the manual says nothing about map downloads today and needs no edit.
- **A box needs no update.** The routes, the index and the `Range` behavior
  stay as they are, so every released box keeps syncing.
- **Backups stay off the server and in another region**, encrypted, with WAL
  archiving.

## The work, in order

Each slice can ship alone, and each one before the cutover leaves the
dedicated server able to run everything.

### 0. Before anything moves

- Put an encrypted copy of `/etc/virtues/{api,atlas,alert}.env` somewhere off
  the server (decision 5). Without it, losing the server means re-issuing
  every provider key.
- Lower the DNS TTL of `s.virtues.ch` to 60 seconds. The other three names
  (`api`, `atlas`, `auth`) are already at 60.
- Check that no provider dashboard allowlists the server's address: the bank
  and sign-in providers, the payment provider and the AI gateway. The repo has
  no hardcoded address; a dashboard might.

### 1. virtues-api reads maps from the bucket

Done on 2026-11-19 ([record](../record/cloud-consolidation.md)). The API on
the dedicated server reads the bucket; `/srv/maps` is still there.

- Rollback: remove the five `VIRTUES_MAPS_S3_*` variables from `api.env` and
  redeploy. The directory source takes over.
- Until slice 2 is done, run `upload.sh <build>` after each monthly cut.
  Otherwise the API keeps serving the build the bucket has, which is a month
  older and still complete. `virtues-health` reports it.
- Only the newest build is in the bucket. A box asks for the build the index
  names, so the older one is not needed.
- The API is in the path of every map download, so a first sync of a few GB
  goes through the server's link. At today's traffic that is far below the
  link, and the capacity bars on the intranet show it if that changes.
- One check is left: read a box's log after its next daily sync.

### 2. The map cut runs on a temporary instance

Done on 2026-11-19 ([record](../record/cloud-consolidation.md)). The
intranet starts the cut on day 3 of each month on a temporary server and
deletes the server when it reports; `deploy/maps/run.sh` is the job. The
first scheduled cut published a build that the live API now serves.

Left:

- Stop the cut timer on the dedicated server. It still cuts its own copy
  each month, which nothing serves.
- `virtues-health` on the dedicated server compares the bucket with that
  local cut. When the timer stops, it needs the other check: the newest
  build in the bucket is younger than the schedule allows.
- The planet download decides how long a run takes: 15 minutes in the
  rehearsal and 86 in the first real cut, from the same source. The
  intranet ends a run after five hours, which leaves room for that.

### 3. The new VPS and a rehearsal

- Base setup as in the runbook: SSH by key only, firewall open on 22, 80 and
  443, unattended upgrades, Docker with capped logs, Postgres at the same
  version, Caddy, pgBackRest, `virtues-health` with its alert mail, the
  metrics agent, and the provider's daily backup of the VPS.
- Restore the database from the backup bucket with archiving switched off, so
  the rehearsal cannot write into the live backup set.
- Build both images there with `DEPLOY_HOST` pointed at the new server, start
  them against the restored copy, and copy the Caddyfile, the certificates and
  `/srv/loader`.
- Check through `--resolve`, without touching DNS: both `/health` endpoints, a
  metered AI call, the payment webhook's signature check, a map file, and row
  counts against production.
- Write each step into `deployment.md` as it is done. That section becomes
  the rebuild runbook, and this rehearsal is its first test.

### 4. Cutover

About a minute of downtime; the last move took 53 seconds.

1. Stop both containers on the dedicated server, then stop Postgres so the
   last WAL segment reaches the backup bucket. Disable Postgres and the backup
   timers there. Two servers must never archive into one backup set.
2. On the VPS, restore to the latest point, start Postgres, and compare row
   counts and the ledger sum with the numbers taken in step 1.
3. Start both containers. Point the four DNS names at the VPS, IPv6 included.
4. Check from outside, watch a real charge land, and take a fresh full backup.

While the names are switching, a box's AI calls and sign-ins fail for about a
minute and then work again. Map files a box already has keep working.

Rollback: point DNS back and start the dedicated server's Postgres and
containers. Anything written after the cutover has to be dumped back first,
because the ledger is on the VPS by then. The dedicated server stays whole
until it is cancelled (decision 3).

### 5. After the cutover

- Remove the directory source from the maps routes, `VIRTUES_MAPS_DIR`, and
  the `/srv/maps` mount in `tools/deploy-service.sh`. One source is left.
- Remove the RAID and SMART checks from the VPS's `virtues-health`; it has no
  disks of its own to watch.
- Update `deployment.md`, `deploy/maps/README.md`, the serving paragraph of
  [record/map-tiles.md](../record/map-tiles.md), and the intranet's Servers
  page (health URL and link size for the new server).
- Cancel the dedicated server before its next renewal. It renews on the first
  of each month.
- Add a dated entry to [record/cloud-consolidation.md](../record/cloud-consolidation.md)
  and delete this plan.

## What gets worse

- Image builds are slower on 4 cores and share them with the live services. A
  deploy will likely trip the intranet's CPU alert (over 60% for 15 minutes)
  until the alert learns about deploys.
- The map cut gains moving parts: an instance to create and delete, and an
  upload.
- The VPS has one virtual disk and no mirror. The provider's daily backup and
  the tested restore cover that; a mirror covered it before.

## Decisions for the owner

1. **How a box gets a map file.** Decided 2026-11-19: through the API. The
   API fetches the file from storage and passes it on. A short-lived link
   straight to storage was turned down because it shows the storage provider
   which address took which square.
2. **Who starts the map cut, and how often.** Decided 2026-11-19: the
   intranet runs it, on a temporary server, each month. It already has the
   provider key and posts to Slack. A cut every three months would serve
   existing boxes the same, since a box only refreshes its files when a build
   is 90 or more days newer; monthly was chosen anyway, and it gives a new
   box fresher maps.
3. **How long the dedicated server stays after the cutover.** Cancel at the
   first renewal after it, or pay one more month for an easier rollback.
4. **The 18 GB folder of model test files** in the server's home directory. It
   is not part of these services. Move it, archive it, or delete it.
5. **Where the copy of the env files lives.** In the backup bucket, encrypted
   with a passphrase only the owner has, or in the owner's password manager.

## Not part of this

- The relay's second server and DNS failover.
- The website's database, which is still open in
  [cloud-consolidation-plan.md](cloud-consolidation-plan.md).
