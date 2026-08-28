//! The button behind the case — the appliance's only physical control.
//!
//! **It forgets devices. It is not a factory reset**, and the difference is the
//! whole security argument (`agents/record/onboarding-paradigm.md` §3):
//!
//! * Anyone who can open the case can press it. That is a **nuisance**: the
//!   owner sets their devices up again and their record is where they left it.
//! * Only someone holding the **phrase** can then claim the box. That is the
//!   part that matters, and a screwdriver does not provide it.
//!
//! Those two sentences only stay true because the phrase **freezes at first
//! claim and never returns to the screen**. A box that showed its phrase again
//! after a reset would hand the case-opener everything.
//!
//! ## Why not more
//!
//! An earlier draft had the button also forget the network and unlink the
//! account. Both are worse on every axis. Neither adds security — the phrase is
//! the entire gate — and forgetting the network actively harms the case the
//! button exists for: an owner who has lost their laptop presses it and now has
//! a box that is *also* offline, unable to reach the relay or atlas. Recovery
//! got harder in exchange for nothing. So this shares exactly one action with
//! the app's `/api/pair/reopen-onboarding` — [`crate::api::pair::revoke_all_devices`]
//! — and a physical control that behaved differently from the software one
//! would be its own bug.
//!
//! ## Why a long press
//!
//! It is the power button. A short press is a person meaning "turn it off", and
//! on a board with no other control it is also what gets hit by a cable, a
//! shelf, or a cat. Unpairing every device in a household on a brush is not a
//! recoverable mistake in any useful sense — the owner has to redo setup on all
//! of them — so the gesture has to be unmistakably deliberate.
//!
//! ## Why we read the input device instead of asking logind
//!
//! `logind` can be told to ignore the power key, and it can be told to power
//! off, suspend or reboot — but it cannot be told to run this. So the installer
//! sets `HandlePowerKey=ignore` (otherwise the first press shuts the box down,
//! which is the failure this must not have) and we watch the evdev node
//! ourselves.
//!
//! Reading `input_event` structs directly rather than adding an evdev crate:
//! the struct is stable kernel ABI, we need exactly one key code from it, and a
//! dependency that pulls in a device-enumeration abstraction to answer "was
//! KEY_POWER held" is more surface than the question deserves.

use std::sync::atomic::{AtomicI64, Ordering};

/// Seconds the button has been held, or `-1` for "not being held".
///
/// An atomic rather than a channel because there is exactly one writer (the
/// evdev thread) and one reader (`api::display`, on its 2s poll), and the
/// reader always wants the latest value rather than a history.
static HELD_FOR: AtomicI64 = AtomicI64::new(-1);

/// How long the button must be held before anything happens.
///
/// Three seconds is long enough that nothing accidental reaches it and short
/// enough that someone deliberately holding it does not conclude the button is
/// broken and let go.
pub const HOLD_SECS: u64 = 3;

/// Seconds held so far, if the button is down right now.
pub fn hold_secs() -> Option<u64> {
    match HELD_FOR.load(Ordering::Relaxed) {
        n if n < 0 => None,
        n => Some(n as u64),
    }
}

fn note_hold(secs: u64) {
    HELD_FOR.store(secs as i64, Ordering::Relaxed);
}

fn clear_hold() {
    HELD_FOR.store(-1, Ordering::Relaxed);
}

#[cfg(target_os = "linux")]
pub use imp::spawn;

/// No physical button anywhere else, and no evdev to read. The DIY box's
/// equivalent control is `virtues reset --keep-data` at a shell it has.
#[cfg(not(target_os = "linux"))]
pub fn spawn(_pool: sqlx::PgPool) {}

#[cfg(target_os = "linux")]
mod imp {
    use sqlx::PgPool;
    use std::io::Read;
    use std::time::{Duration, Instant};

    /// `KEY_POWER` from the kernel's `input-event-codes.h`.
    const KEY_POWER: u16 = 116;
    /// `EV_KEY`.
    const EV_KEY: u16 = 1;

    /// The threshold, as a Duration. The panel narrates the hold via
    /// `super::hold_secs`, so the owner is told how long is left rather than
    /// having to guess whether the button works.
    const HOLD: Duration = Duration::from_secs(super::HOLD_SECS);

    /// How often we re-scan for a power button, when there wasn't one.
    const RESCAN: Duration = Duration::from_secs(60);

    pub fn spawn(pool: PgPool) {
        // Appliance only. A DIY box is someone's own server: its power button
        // is theirs and means what they configured it to mean, and quietly
        // repurposing it would be an unpleasant surprise on a machine we are a
        // guest on.
        if !crate::maintenance::setup_ap::is_appliance() {
            tracing::debug!("reset_button: not an appliance, leaving the power key alone");
            return;
        }
        let handle = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || watch(handle, pool));
    }

    /// Find the button and read it forever, re-scanning if it goes away.
    ///
    /// Blocking, on its own thread, because the read is a blocking read on a
    /// character device: there is nothing here for the async runtime to do
    /// except hold a thread hostage, which is what `spawn_blocking` is for.
    fn watch(handle: tokio::runtime::Handle, pool: PgPool) {
        loop {
            match power_button_device() {
                Some(path) => {
                    tracing::info!(device = %path, "reset_button: watching the power key");
                    // Returns on any read error — the device vanishing on a
                    // suspend/resume or a USB replug — and the loop re-scans.
                    read_device(&path, &handle, &pool);
                    tracing::warn!("reset_button: lost the power key, rescanning");
                }
                None => {
                    tracing::debug!("reset_button: no power button on this board");
                }
            }
            std::thread::sleep(RESCAN);
        }
    }

    /// Which `/dev/input/event*` is the power button?
    ///
    /// Found by capability rather than by name: boards disagree wildly about
    /// what they call it (`gpio-keys`, `pm8941_pwrkey`, `Power Button`), and a
    /// name match that misses leaves the one physical control on the product
    /// silently dead. `/proc/bus/input/devices` lists a `KEY=` bitmask per
    /// device; bit 116 set means this device can report KEY_POWER, which is the
    /// question actually being asked.
    fn power_button_device() -> Option<String> {
        let devices = std::fs::read_to_string("/proc/bus/input/devices").ok()?;
        for block in devices.split("\n\n") {
            if !block.lines().any(|l| l.starts_with("B: KEY=") && key_bitmask_has_power(l)) {
                continue;
            }
            // `H: Handlers=kbd event3 ...`
            let handler = block
                .lines()
                .find(|l| l.starts_with("H: Handlers="))
                .and_then(|l| l.split_whitespace().find(|w| w.starts_with("event")));
            if let Some(ev) = handler {
                let path = format!("/dev/input/{ev}");
                if std::path::Path::new(&path).exists() {
                    return Some(path);
                }
            }
        }
        None
    }

    /// Does this `B: KEY=` line have bit 116 set?
    ///
    /// The bitmask is printed as space-separated 64-bit hex words, **most
    /// significant word first**, so the word holding bit 116 is at index
    /// `words.len() - 1 - (116 / 64)` counting from the left. Getting that
    /// direction wrong reads a different key entirely, which is why it is
    /// pulled out and tested rather than inlined.
    fn key_bitmask_has_power(line: &str) -> bool {
        let Some(mask) = line.strip_prefix("B: KEY=") else {
            return false;
        };
        let words: Vec<&str> = mask.split_whitespace().collect();
        let word_index_from_right = (KEY_POWER / 64) as usize;
        if words.len() <= word_index_from_right {
            return false;
        }
        let w = words[words.len() - 1 - word_index_from_right];
        let Ok(v) = u64::from_str_radix(w, 16) else {
            return false;
        };
        v & (1u64 << (KEY_POWER % 64)) != 0
    }

    /// One `struct input_event`. Layout is kernel ABI:
    /// `timeval { long sec; long usec; }` then `u16 type, u16 code, i32 value`.
    ///
    /// Sized from `libc::timeval` rather than assumed, so a 32-bit board (where
    /// `long` is 4 bytes and the struct is 16, not 24) reads correctly instead
    /// of silently misparsing every event.
    const EVENT_SIZE: usize = std::mem::size_of::<libc::timeval>() + 8;

    fn read_device(path: &str, handle: &tokio::runtime::Handle, pool: &PgPool) {
        let Ok(mut f) = std::fs::File::open(path) else {
            return;
        };
        let mut buf = vec![0u8; EVENT_SIZE];
        // When the press began. `None` between presses.
        let mut down_at: Option<Instant> = None;
        // Set once per press, so holding past the threshold fires exactly one
        // reset rather than one per repeat event the kernel sends.
        let mut fired = false;

        loop {
            if f.read_exact(&mut buf).is_err() {
                return;
            }
            let ty = u16::from_ne_bytes([buf[EVENT_SIZE - 8], buf[EVENT_SIZE - 7]]);
            let code = u16::from_ne_bytes([buf[EVENT_SIZE - 6], buf[EVENT_SIZE - 5]]);
            let value = i32::from_ne_bytes([
                buf[EVENT_SIZE - 4],
                buf[EVENT_SIZE - 3],
                buf[EVENT_SIZE - 2],
                buf[EVENT_SIZE - 1],
            ]);
            if ty != EV_KEY || code != KEY_POWER {
                continue;
            }

            match value {
                // Press.
                1 => {
                    down_at = Some(Instant::now());
                    fired = false;
                    super::note_hold(0);
                }
                // Autorepeat, which is how we learn the button is still down
                // without polling. Boards differ on whether they send these, so
                // the release branch checks the elapsed time too.
                2 => {
                    if let (Some(start), false) = (down_at, fired) {
                        let held = start.elapsed();
                        super::note_hold(held.as_secs());
                        if held >= HOLD {
                            fired = true;
                            do_reset(handle, pool);
                        }
                    }
                }
                // Release. Fires the reset for a board that sends no autorepeat,
                // and clears the panel's countdown for a hold that was let go.
                0 => {
                    if let (Some(start), false) = (down_at, fired) {
                        if start.elapsed() >= HOLD {
                            do_reset(handle, pool);
                        }
                    }
                    down_at = None;
                    fired = false;
                    super::clear_hold();
                }
                _ => {}
            }
        }
    }

    /// Do it.
    ///
    /// Hands the query to the runtime that is already running rather than
    /// blocking this thread on one. Two reasons: `block_on` from inside a
    /// runtime's blocking pool is a footgun that depends on which pool you are
    /// actually on, and the button must stay responsive — an owner who holds it
    /// again while a slow revoke is in flight should not find the input device
    /// unread.
    fn do_reset(handle: &tokio::runtime::Handle, pool: &PgPool) {
        tracing::warn!("reset_button: held past the threshold — revoking every device");
        let pool = pool.clone();
        handle.spawn(async move {
            match crate::api::pair::revoke_all_devices(&pool).await {
                Ok((devices, creds)) => {
                    tracing::warn!(devices, creds, "reset_button: onboarding re-opened");
                }
                Err(e) => tracing::error!("reset_button: revoke failed: {e:#}"),
            }
        });
        super::clear_hold();
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn finds_power_in_a_real_bitmask() {
            // From a Dragon Q6A's /proc/bus/input/devices — bit 116 lives in
            // the second word from the right (116 / 64 == 1), and 1 << 52 is
            // 0x10000000000000.
            assert!(key_bitmask_has_power("B: KEY=10000000000000 0"));
        }

        #[test]
        fn ignores_a_keyboard_without_the_power_key() {
            // A real USB keyboard's mask. Bit 116 falls in the second word from
            // the right at position 52, and this one does not have it.
            assert!(!key_bitmask_has_power(
                "B: KEY=e080ffdf01cfffff fffffffffffffffe"
            ));
        }

        #[test]
        fn the_word_order_is_most_significant_first() {
            // The bitmask prints high words on the LEFT, so bit 116 is in the
            // second word from the RIGHT. Reading from the left instead finds
            // bit 52 of a different word — a plausible-looking match on the
            // wrong key, which is how the one physical control on the product
            // would end up wired to something else.
            assert!(key_bitmask_has_power("B: KEY=10000000000000 0"));
            assert!(!key_bitmask_has_power("B: KEY=0 10000000000000"));
        }

        #[test]
        fn a_short_mask_does_not_panic_or_match() {
            // Some devices print a single word. Indexing past it used to be the
            // obvious way to write this.
            assert!(!key_bitmask_has_power("B: KEY=1"));
            assert!(!key_bitmask_has_power("B: KEY="));
        }

        #[test]
        fn a_malformed_mask_is_not_a_match() {
            assert!(!key_bitmask_has_power("B: KEY=zzzz 0"));
            assert!(!key_bitmask_has_power("B: ABS=10000000000000 0"));
        }

        #[test]
        fn the_event_struct_is_the_kernel_size() {
            // 24 on 64-bit, 16 on 32-bit. A wrong size here reads garbage for
            // every field and the button silently never fires.
            assert_eq!(EVENT_SIZE, std::mem::size_of::<libc::timeval>() + 8);
            assert!(EVENT_SIZE == 24 || EVENT_SIZE == 16, "unexpected {EVENT_SIZE}");
        }
    }
}
