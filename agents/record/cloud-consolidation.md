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
