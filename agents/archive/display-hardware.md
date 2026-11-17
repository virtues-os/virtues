# The Dragon Q6A panel — measured behavior

> **Recorded 2026-08-26/27.** Bench findings from making the box's screen
> sleep and wake on real hardware (board "Rosy Swallow", HDMI panel). Extracted
> from `agents/plan/display-plan.md` on 2026-08-28 because these facts exist
> **nowhere in the source tree** and would have been lost when the plan was
> filed. Everything here was observed on the hardware.

## The panel reports a wrong EDID in two ways

**After any forced off → `detect` cycle, the connector cannot re-read EDID.**
`msm_dp_bridge_get_modes` fails (rc=0, reproducibly, clean cycles included) and
the connector falls back to VESA 1024×768/800×600, after which the scaler draws
everything stretched. Only a full power cycle of panel *and* box recovers the
real EDID.

**And the EDID's content is itself inconsistent** — the same panel reported
1920×1080-preferred after a cold boot and 1024×600-preferred after a single
hotplug, in one afternoon.

This is why the EDID pin exists. `install.rs` contains the fix; this document
is the only place that explains the cause.

**Pin per panel model, not per box.** "Capture the EDID at firstboot" is the
obvious design and it is wrong here: firstboot could capture either of the two
EDIDs above. The pinned blob has to be a known-good one, shipped with the
image.

## Never run ddcutil against this panel

A `ddcutil detect` scan threw GENI i2c DMA errors and left the DDC line wedged;
recovery required a physical power cycle. **A brightness feature must never
ship a probe.** This warning appears in no source file.

Brightness control cannot be built on this hardware: `/sys/class/backlight/`
is empty (HDMI panel, nothing on a kernel PWM) and
the panel does not speak DDC/CI. The hardware supports only off
and on.

## The bootloader ignores extlinux.conf

On the Q6A, **`/boot/extlinux/extlinux.conf` has no effect.** The Qualcomm boot
chain bakes bootargs into the device tree, so the documented
`/etc/kernel/cmdline` + `u-boot-update` flow writes an append line nothing
reads.

That is why the EDID pin ships as a **runtime module parameter** written by
`ExecStartPre` into `/sys/module/drm/parameters/edid_firmware`, rather than as
a kernel cmdline argument. Without this note the installer's approach looks
arbitrary, and someone will helpfully "fix" it back to a cmdline that has no
effect.

## Why sleep forces the connector off instead of using DPMS

The by-the-book path is DPMS through the compositor, which would mean
compiling and shipping cage with `wlr-output-power-management-v1` forever —
stock cage lacks it, so `wlopm` cannot sleep the output under the running
kiosk. Forcing the connector avoids carrying a compositor patch indefinitely.
Revisit only if a kernel update breaks connector forcing.

## The captured EDID blob

128 bytes, md5 `c20eb215b495a300a9738d06d9285a45`, taken from the bench panel
and archived on that box at `/home/radxa/panel-edid.bin`.

**This repository does not ship it.** The installer references
`edid/virtues-panel-1080.bin` guarded by `[ -f … ]`, so on any box without the
file the pin silently no-ops and the panel can report a wrong EDID again. Checking the
blob in next to the unit template that references it is real work with a
hardware verification step, not a documentation change — it is recorded here as
a gap, and this base64 is the only surviving copy:

```
AP///////wBI9BFSAQQAAAUXAQSlNR54AoBCrFEwtCUQUFMAAAABAQEBAQEBAQEBAQEBAQEBKDaAoHA4H0AwIDUAB0QhAAAaIi2AoHA4H0AwIDUAB0QhAAAaAAAA/gAKICAgICAgICAgICAgAAAA/gAxOTIweDEwODAKICAgADY=
```

Restore with `base64 -d` into a 128-byte file; verify the md5 above before
trusting it.

## How sleep and wake work

Sleep forces the connector off while the kiosk unit keeps running (cage
survives losing its outputs, and starts with none); wake forces `detect`, and
restarts the unit only if it failed anyway. Both run under the existing
privilege model (`system_display::sleep_engine`).

**The sleep marker carries the connector name** for a reason worth keeping: an
upgrade restarts `virtues.service` while the display unit keeps running, and
without adoption the successor process believes the screen is awake — leaving
the panel dark until someone SSHes in.

**Sleep is a state that interruptions override; it is not a plain scheduled
toggle.** A screen asleep during a
storage fault, an update, or a held case button violates the duty-list
contract. The button case is the most serious: a hold on a dark screen gets no
countdown, which is the exact failure the countdown exists to prevent. The
server owns the schedule and overrides it whenever an interruption is active.

## Tools left installed on the bench board

`wlopm` and `ddcutil` are apt-installed on the bench board. Harmless, but see
above — do not run `ddcutil` against the panel again.
