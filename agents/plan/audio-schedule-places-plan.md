# Mic schedule, places and pause: what is left

The weekly schedule, muted places, muted markers, the Control Center pause
control and the recording widget are built. How they work, and the decisions
behind them, are in
[record/audio-schedule-places.md](../record/audio-schedule-places.md). What is
left is device verification and three small pieces. Delete this plan when the
list is empty; what remains then is the record and a manual page for the
phone's recording settings.

## Not built

- **Refresh the muted places when the app opens.** Today the phone copies the
  muted places into the plugin when the audio settings screen loads
  (`apps/web/src/lib/mobile/audioPlaces.ts`), not at launch.
- **Count muted minutes in the census and on the device page,** beside
  silent and recorded minutes, so "why is today's audio short" has an answer.
- **Sessionize skips markers** (`sessionize/audio.rs`): a muted marker is
  absence, not silence with a level. Check whether this landed before
  building it.

## Device gates

Each slice is a native iOS build. Verify on the review device before relying
on it; every change to this tap that looked obviously fine has surprised us at
least once.

1. **Gate and schedule.** Quiet hours behave exactly as before on the migrated
   store; a Saturday-only window mutes on Saturday and not on Friday at the
   same hour; the watchdog stays quiet and location stays coarse through a
   window.
2. **Places.** Walk into and out of a muted place with the console attached
   and see exactly one enter and one exit; sit still inside until the fix goes
   coarse and confirm no exit; kill and relaunch inside and confirm muted on
   the first precise fix; flip the toggle on the desktop and confirm the phone
   follows it after reopening.
3. **Markers.** A day with a muted afternoon is described as "muted by place",
   and the coverage figure separates muted from silent from recorded.
5. **Pause control and widget.** First sign in to Xcode again so automatic
   signing can make the new profiles, then make a device build. Then: add the
   control in Control Center; pause from the Lock Screen with the app in the
   background and see `[Audio] override none → silence until …` and a `pause`
   marker after five minutes; let an hour run out and see recording resume
   with no tap; during a muted window, tap to record anyway and see it end
   when the window closes; pause, force-quit, relaunch, and confirm it is
   still paused; Stop in the app and see the control read Off. Widget: add
   each size; pause from it and see the countdown tick; force-quit the app
   and see "No word since" 45 minutes later; turn on Airplane Mode for 30
   minutes and see "Can't reach your server".

## Deferred: the schedule on the box (slice 4)

Nothing on the box models a weekly schedule, and the phone is the only thing
that evaluates it. If desktop editing of the schedule is wanted, store a small
document against the device (`app_device.device_info` is an existing jsonb
field; decide then whether it is the right place, or add an append-only
column), pushed by the phone on change and pulled on open. Not before the
slices above pass on a device and have been used.
