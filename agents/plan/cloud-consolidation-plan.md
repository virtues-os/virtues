# Cloud consolidation — what is left

atlas and virtues-api moved onto one dedicated server with its own Postgres
on 2026-09-28 (53 seconds of downtime), with encrypted backups archiving off
the server and a restore tested before any real data landed. How it runs now
is in [deployment.md](../build/deployment.md#cloud-sidecar-atlas--virtues-api-on-one-server)
(rewritten dd27b61b). The relay stays on its own server. Delete this plan when
the list below is empty.

Verified on 2026-09-30: both containers healthy with no errors logged, RAID
and SMART clean, daily backups and WAL archiving current, no health alert has
fired, and the first map build is published and served. On 2026-10-01 atlas
was deployed with `make deploy-atlas` for the first time (the refund fix,
f1ae1f25, is live), so both services now have a `:previous` rollback image.

## Open

1. **Decide where the website's database lives.** It is the last database on
   the old hosting and holds two live projects, the website's and an
   unrelated one. Its port is open to any address because the website's host
   has no fixed IPs, so it does not belong on the billing server; the real fix
   is the website not talking to a database directly.
2. **Delete the final snapshots** (from 2026-11-05). Each retired instance
   and database left one; nothing has needed them yet.

Done on 2026-10-05: the review demo moved to its own small VPS at this
provider (the database and env file carried over, so the box kept its iroh
identity and the reviewer's pairing; about two minutes of downtime; health
checks added, which it had lacked), and the old hosting's instances,
databases, image registry, and leftover DNS records were deleted. DNS and IAM
stay where they are by choice. Map fonts reached the one box that had
recorded the first build's broken archive.
