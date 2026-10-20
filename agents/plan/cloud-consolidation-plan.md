# Cloud consolidation: what is left

atlas and virtues-api run on one dedicated server since 2026-09-28; the move
is recorded in [record/cloud-consolidation.md](../record/cloud-consolidation.md)
and how it runs is in
[deployment.md](../build/deployment.md#cloud-sidecar-atlas--virtues-api-on-one-server).
Delete this plan when the list below is empty.

## Open

1. **Decide where the website's database lives.** It is the last database on
   the old hosting and contains two live projects, the website's and an
   unrelated one. Its port is open to any address because the website's host
   has no fixed IPs, so it does not belong on the billing server. The real fix
   is for the website to stop talking to a database directly.
2. **Delete the final snapshots** (from 2026-11-05). Each retired instance
   and database left one; nothing has needed them yet.
