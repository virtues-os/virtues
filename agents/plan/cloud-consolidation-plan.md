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

1. **Map fonts.** The 2026-10-02 build's fonts archive holds no links and
   unpacks, but a box that had recorded the first build's broken archive kept
   it under the 90-day refresh rule and stayed without fonts. The box now
   treats an archive with nothing unpacked as stale (aa6f3c8f, unreleased);
   the one affected box had the archive cleared by hand on 2026-10-05.
   Confirm that box's log shows the assets unpacked, then delete this item.
2. **Retire the old demo instance** (from 2026-10-12). The review demo moved
   to its own small VPS at this provider on 2026-10-05: same version, the
   database and env file carried over so the box kept its iroh identity and
   the reviewer's pairing, both hostnames switched in DNS with about two
   minutes of downtime, and monitoring added (it had none, which is how its
   nightly reset failed unseen for two weeks). The old instance is stopped
   at the service level and holds a final database dump. After the week:
   snapshot, then delete (ask first).
3. **Decide where the website's database lives.** It is the last database on
   the old hosting and holds two live projects, the website's and an
   unrelated one. Its port is open to any address because the website's host
   has no fixed IPs, so it does not belong on the billing server; the real fix
   is the website not talking to a database directly.

The old hosting's atlas and api databases, their instance, and the image
registry were deleted on 2026-10-05 after a final snapshot of each; DNS and
IAM stay where they are by choice.
