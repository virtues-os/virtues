# Backup — what is still open

The design and everything built is recorded in
[`../record/backup.md`](../record/backup.md). Each gap below was checked
against the code on 2026-09-29. Delete a line when it ships; delete the file
when the list is empty.

## Correctness

- **Increment holes restore silently.** `restore::apply_from_volume` says a
  missing increment "fails loudly and names the window", but `survey_volume`
  only lists the `lake-*` files present — nothing records which increments
  should exist, so a deleted one restores short with no error. Needs a chain
  (each full's manifest naming the increments it expects, or each increment
  naming its predecessor), then the check. Fix the doc comment in the same
  change.
- **"Never prune below 2" is not honored.** `prune_full_archives` keeps one
  full via `.skip(1)`, and it runs *before* the new full is written, so a tight
  drive holds exactly one full during every write. Keep two, and refuse when
  two will not fit.
- **The local `/var/lib/virtues/backups` directory has no retention.** Only
  `pre-upgrade-*.dump` files are pruned (with release slots); every manual
  `virtues backup` tarball stays forever.

## Running without a human

- **Scheduled random verification.** `virtues backup --verify <archive>`
  exists; nothing runs it. Verify one archive at random on a schedule so bit
  rot surfaces before an incident, not during one.
- **Volume probing.** `capacity_bytes`, `free_bytes` and `probed_at` on
  `storage_volume` are never refreshed by core. The device classifier and
  benchmark still live in `tools/virtues-installer/src/storage.rs`.

## Visibility

- **The Home warning.** Backup age shows in the System page only. Home should
  carry one line when it is bad ("Last backup: 9 days ago", "No backup
  destination"), and onboarding should frame no destination as an unfinished
  step. There is no push path, so a box whose owner never opens the UI hears
  nothing.
- **Recovery-key copies.** The key is printed once. The decided answer to loss
  — copies into paired devices' keychains, and Settings counting how many
  independent copies exist — is not built.

## Drives

- **Format offer for an unmountable drive**, behind typed confirmation. Blocked
  on verifying that the Dragon image ships exFAT/NTFS drivers; nothing in the
  image tooling mentions either.
