# The Mac — what is left

The built half (monotonic version counter, three-state permissions with an
observation time, silent staged updates, one `mac-latest` channel, the
command-surface version) and the outage that prompted it are in
[mac-silent-outage.md](../record/mac-silent-outage.md). Where the Mac's
interface comes from (the box's live SPA or its own baked copy) is owned by
`local-ui-plan.md`, not here.

## Open

- **One version the user sees.** The target is that the user never learns the
  Mac app has a version: the displayed release is the paired box's, cached for
  offline, and the app's own number (`tauri.conf.json`, 1.0.x) is a build
  counter nothing displays. Today Devices still shows `app_version`
  (`DevicesView.svelte`). Do not renumber `virtues-core` to reconcile them:
  boxes compare those versions during upgrade.
- **Apply while running.** Staged updates apply on the next launch; an app
  left running for weeks never gets there. Apply in an idle window and
  relaunch to the same state. The 6h check also has no wake or network-up
  recheck (audit U6); opening This Mac's update state is the only foreground
  check.
- **Re-grant after a signature change.** Local builds are still unsigned
  unless `APPLE_SIGNING_IDENTITY` is set (`tools/build-mac-app.sh` warns).
  Default to a Developer ID identity from the keychain and make unsigned
  opt-in. In the copy, Accessibility needs the same "Not listed? Click +"
  line Full Disk Access has, and both should say remove-and-re-add when the
  collector reports denied while an entry exists.

## Not doing

- A changelog in the app. The box is the product surface.
- A forced relaunch, even for security fixes.
- A per-device channel override: it is how a fleet comes to disagree with
  itself.
