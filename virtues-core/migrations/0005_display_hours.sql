-- 0005_display_hours
--
-- Hours — the screen keeps hours, like a shop (Settings → Display).
-- Two box-local times; NULL/NULL means the screen never sleeps. Both or
-- neither, enforced: one time without the other is not a schedule.
--
-- What these drive is real power, not pixels: during the window the box
-- forces the panel's connector down and the backlight goes off (see
-- docs/display-plan.md, backlight audit 2026-08-26). Sleep is a PRECEDENCE
-- STATE below every interruption — the server-side engine wakes the glass
-- for a held button, an update, or a storage fault, and never sleeps an
-- unclaimed box (setup must show).
ALTER TABLE app_display
    ADD COLUMN sleep_start time,
    ADD COLUMN sleep_end time,
    ADD CONSTRAINT display_hours_paired CHECK ((sleep_start IS NULL) = (sleep_end IS NULL));
