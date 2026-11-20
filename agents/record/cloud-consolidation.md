# Cloud consolidation onto one server

**Written 2026-10-20.** atlas and virtues-api moved onto one dedicated server
with its own Postgres on 2026-09-28. How it runs now is in
[deployment.md](../build/deployment.md#cloud-sidecar-atlas--virtues-api-on-one-server).
The relay stays on its own server. The two items still open are in
[cloud-consolidation-plan.md](../plan/cloud-consolidation-plan.md).

## What happened

- **2026-09-28, cutover.** 53 seconds of downtime. Encrypted backups archive
  off the server, and a restore was tested before any real data landed.
- **2026-09-30, checks.** Both containers healthy with no errors logged, RAID
  and SMART clean, daily backups and WAL archiving current, no health alert
  fired, and the first map build published and served.
- **2026-10-01.** atlas deployed with `make deploy-atlas` for the first time
  (the refund fix, f1ae1f25, went live), so both services have a `:previous`
  rollback image.
- **2026-10-05.** The review demo moved to its own small VPS at the same
  provider. The database and env file carried over, so the box kept its iroh
  identity and the reviewer's pairing; downtime was about two minutes, and it
  gained the health checks it had lacked. The old hosting's instances,
  databases, image registry and leftover DNS records were deleted. DNS and IAM
  stay at the old provider by choice. Map fonts reached the one box that had
  recorded the first build's broken archive.
- **2026-11-19, maps read from object storage.** The newest build (9,923
  files, 178 GB) was uploaded to a private bucket and every object compared
  with the local file by size and md5. virtues-api was deployed with the
  bucket source on; a box's key read the index, a whole file that matched its
  sha256, and a resumed range through the live API. The directory on the
  server is still there and still the rollback. This is the first slice of
  [moving off the dedicated server](../plan/cloud-vps-move-plan.md).
- **2026-11-19, the map cut moved to a temporary server.** The operator's
  intranet now creates a server for the monthly cut, hands it a storage key
  and a report token made for that run, and deletes it on the final report.
  A rehearsal under a prefix no box reads ran in 49 minutes. The first
  scheduled cut then failed 21 minutes in: the job reported from its setup
  step after it had started cutting, so it had been started twice. The
  server was gone before its log could be read; what fits is Ubuntu's
  background upgrade on the new server restarting the job's service. Nothing
  was uploaded and boxes kept their build. The job now switches those
  upgrades off, cannot be restarted by anything on the server, uploads its
  log before the final report, and reports progress every five minutes. The
  rerun published build 20261118 (9,920 files, 178.2 GB) in 106 minutes, 86
  of them the planet download; every object matched its index, a box's key
  read it through the live API with matching checksums, and the previous
  build stayed in the bucket.
- **2026-11-19, alerts.** The intranet's Slack alerts channel had never been
  set, so no alert of any kind had been delivered. It now posts to a private
  channel and logs when it has nowhere to post. Failure class: an alert path
  that returns quietly when unconfigured looks, from the code, exactly like
  one that works; send one and read it back.
