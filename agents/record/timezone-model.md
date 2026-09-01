# Timezone model — two timezones: box `home` + per-day user location (RFC)

> **Status:** IMPLEMENTED (2026-06-25). All five plan steps landed; `cargo check
> --workspace` and web `svelte-check` clean. See "Implementation status" below.
>
> **Status:** Draft RFC for review (2026-06-25). Started from a real bug — the web
> "Today" page showing the *previous* evening's records alongside today's — and,
> after several rounds, landed on a **two-timezone** model:
>
> 1. **`home_timezone`** — the timezone of the box's physical location. Stable,
>    read from the server's own clock. A fallback floor + scheduling anchor.
> 2. **per-day user-location timezone** (`wiki_days.start_timezone`) — *where the
>    owner actually was that day*. This is what the Today/wiki page renders in, so
>    it "feels local to where they were."
>
> These are different and both needed. The bug came from having neither populated.
> The `data_timezone` segment timeline (sub-day changepoints) stays deferred —
> per-day granularity is enough.

## The bug

The "Today" page fetches records by a UTC window from
[`day_boundaries_utc`](../virtues-core/src/api/day_summary.rs#L123-L153). With a
valid timezone it builds a correct 24h local-day window; otherwise it falls back
to a **36-hour** window (`00:00 today → 12:00 next day`) anchored at UTC midnight,
which reaches back into yesterday evening for any negative-UTC-offset user. That's
the leak (8:51 PM / 9:13 PM chat rows on today's page, all-day `12:00 AM Ashura`
sorting below them).

The fallback fires because the timezone is **NULL** — nothing populates it
([profile.rs:182](../virtues-core/src/api/profile.rs#L182) only reads). The fix
is to populate *both* timezones below and to make the fallback window a correct
24h as defense-in-depth.

## Principles we keep

- **All timestamps stored in UTC** (`TIMESTAMPTZ`). Timezone is applied only at
  the edges — day boundaries, cron, display, novelty hour-bucketing.
- Single-user, self-hosted appliance.

## Timezone 1 — `home_timezone` (the box's location)

The appliance sits in the owner's home, which has one fixed timezone. We store it
as one IANA string on the singleton profile, renamed `profile.timezone →
home_timezone`.

- **Source:** the server reads its *own* system timezone (`iana-time-zone` crate →
  IANA string from `/etc/localtime`) at onboarding. Literally "the location of the
  server." Stable; does not move when the owner travels.
- **Cloud caveat:** a datacenter box reads `UTC`, which is wrong. Appliance is the
  primary deploy, so server-self is the default; for cloud, set explicitly at
  onboarding (owner picks, or cross-check the pairing device's reported zone via
  the existing `device_info` blob).
- **Role:** the **fallback floor** for the per-day timezone below, and the anchor
  for box-side scheduling (the maintenance cron runs in home time).

## Timezone 2 — per-day user-location tz (`wiki_days.start_timezone`)

This is the one that makes the page "feel local to where they were." It is the
timezone the owner was *physically in* on that day, recorded per wiki day.

The signal of record is **the first located point of the day** — "where you woke
up". The same value is used live (today) and at the EOD lock, so they agree even
on a travel day. The resolution ladder is:

1. **Locked `start_timezone`** — if a summary already ran for the day (past days,
   and today after EOD), use the stored value. Authoritative, cheap.
2. **`tzf-rs(first located point of the day)`** — the dense GPS track
   (`data_location_point`, ~15s cadence) resolved offline via **`tzf-rs`**
   (`coords → IANA`, microseconds, no-cloud). Deliberately the *first* point
   (init), not modal: deterministic at day-start, never drifts as the day unfolds.
   A move taken *during* today surfaces as **tomorrow**, not a mid-day re-anchor.
3. **Viewing device's zone** (`Intl…timeZone`, sent as `?tz=` on the today fetch)
   — fallback for an in-progress *today only* with no located points yet (web-only
   / location off). Never applied to a past day (it would render history in the
   travel zone). "Today" is computed in `home_timezone`.
4. **`home_timezone`** — final floor.

This is re-derived per request rather than persisted-at-rollover; identical result
either way for a stationary day, and step 2 makes live-today consistent with the
EOD lock on a travel day. At the EOD lock the same ladder (minus the device step)
writes `start_timezone` via `resolve_day_timezone` in `generate_day_summary`.

- **Rendering:** the day page renders timestamps in the *same* zone the day was
  windowed in. The web Time column / chat times use `page.start_timezone` (the
  locked per-day zone), falling back to the browser zone for an unlocked today —
  which equals the `?tz=` the server used to window it. So which records appear and
  the times shown stay consistent.
- **No `end_timezone`:** dropped (migration `0015`). One zone per day; a day that
  crosses zones mid-flight anchors to where it began. Sub-day precision = the
  deferred `data_timezone` work.

## The whole model

```
profile.home_timezone           ← box location; server's own system tz; stable
   │                               role: fallback floor + box cron scheduling
   ▼ (final fallback)
wiki_days.start_timezone        ← WHERE THE OWNER WOKE UP that day
   resolution ladder (live + EOD lock agree):
     1. locked start_timezone (past days / today post-EOD)
     2. tzf-rs(first located point of the day)   ← "woke up in"; never drifts
     3. viewing device ?tz=  (today-only, no located points yet)
     4. home_timezone
       → renders the Today/wiki page · per-day boundaries · novelty bucketing
```

## Implementation plan

### 1. Rename `profile.timezone → home_timezone`

Migration `ALTER TABLE app_user_profile RENAME COLUMN timezone TO home_timezone;`
([0003_app_shell.sql:36](../virtues-core/migrations/0003_app_shell.sql#L36)), plus
[profile.rs](../virtues-core/src/api/profile.rs) L31/L82/L139-141/L182 and
[+layout.ts:61](../apps/web/src/routes/(app)/+layout.ts#L61). Keep the
`get_timezone()` helper name. (`wiki_days.start_timezone` is a different name,
untouched; `end_timezone` is dropped separately — see migration `0015`.)

### 2. Populate `home_timezone` from the server's own system tz

Add `iana-time-zone`. At onboarding (or lazily when `home_timezone IS NULL`), call
`iana_time_zone::get_timezone()` → persist via `profile::update_profile`. Cloud
fallback: explicit set / pairing-device cross-check in `consume_handler`
([pair.rs:613](../virtues-core/src/api/pair.rs#L613); thread `timezone` through
[iOS `PairingDeviceInfo`](../apps/ios/Virtues/Managers/Data/NetworkManager.swift#L387-L393)
and [web `DeviceInfo`](../apps/web/src/lib/types/device-pairing.ts#L8-L14)).

### 3. Add `tzf-rs` + derive per-day `start_timezone` from location

Add `tzf-rs` and `coords_to_tz` / `first_point_timezone` helpers. In
`generate_day_summary`, write `start_timezone` from `resolve_day_timezone`
(= `first_point_timezone` → `home_timezone`) instead of a home clone.

### 4. Live "today" resolution

The web today-page request carries `Intl…timeZone` as `?tz=`. The day-sources
handler resolves the day's zone via the ladder above — **first point first**, with
`?tz=` only as the today-with-no-GPS fallback — so live-today matches the EOD lock
rather than re-anchoring to where the viewer currently is.

### 5. Fix the fallback window

In `day_boundaries_utc`, change the fallback end from `00:00 → 12:00 next day` to a
true `00:00 → 00:00 next day` 24h window. Defense-in-depth; should rarely execute
once 2–4 land.

## Implementation status (2026-06-25)

Landed (Rust `cargo check --workspace` + web `svelte-check` clean):

- **Migrations** `0014_home_timezone.sql` (`timezone → home_timezone`) and
  `0015_drop_wiki_days_end_timezone.sql` (drop the vestigial column). Rust
  (`models.rs`, `profile.rs`, `dayline/context.rs`, `day_summary_eod`, `wiki.rs`,
  `tools/sql_query.rs`) + web (`client.ts`, `+layout.ts/.svelte`,
  `ProfileView.svelte`, onboarding `setup/+page.svelte`, `wiki/api.ts`,
  `wiki/converters.ts`, `wiki/types/day.ts`, `DayPage.svelte`) + all `wiki_days`
  seeds updated.
- **`virtues-core/src/timezone.rs`** (new module) — `system_timezone()`
  (`iana-time-zone`), `coords_to_tz()` (`tzf-rs`, memoised `DefaultFinder`),
  `first_point_timezone()` (first located point of the day → `Option`), and
  `resolve_day_timezone()` (= first point → home fallback; used at the EOD lock).
- **`profile::get_timezone()`** is a **pure read**; seeding moved to
  `ensure_home_timezone()`, called once at server startup ([server/mod.rs](../virtues-core/src/server/mod.rs))
  before the scheduler resolves cron zones, and in the pairing cross-check.
- **`day_summary`** computes boundaries + writes `start_timezone` from the per-day
  location tz; **fallback window fixed** 36h → 24h.
- **`get_day_sources(date, client_tz)` + `resolve_render_timezone()`** — ladder:
  locked `start_timezone` → `tzf-rs(first point)` → (today-only) `?tz=` device zone
  → home. Handler reads `?tz=`; web `getDaySources` sends `Intl…timeZone`.
- **Day-page rendering** — Time column + chat times render in `page.start_timezone`
  (`rowTz`) so shown times match the windowed day; `formatTimezoneDisplay`
  collapsed to single-zone (dead travel-day branch removed).
- **Pairing cross-check** — iOS `PairingDeviceInfo.timezone` =
  `TimeZone.current.identifier`; web pairing sends `device_info.timezone`;
  `pair.rs consume_handler` sets `home_timezone` from it only when the box's own
  value is unset/`UTC` (cloud case).

Not yet verified at runtime: needs `cargo sqlx migrate run` against a dev DB and a
manual check that Today no longer leaks the prior evening. iOS Swift is unbuilt
here (no Xcode).

## Known limitation

**Today, while travelling, *with* a GPS track:** the server windows today in the
first-point zone (origin, "woke up in"), but until the EOD lock writes
`start_timezone`, the *frontend* has no per-day zone (`page.start_timezone` is
NULL) and renders the Time column in the browser zone (destination). So the
*which-records* boundary is correct but the displayed times are off by the travel
offset for the rest of that one day; it self-heals at the EOD lock, and all
day-page components (table, dayline, timeline) share the same fallback so they
stay internally consistent. Closing it fully means surfacing the server's resolved
zone on the today fetch (or locking `start_timezone` at first access) — deferred;
the common cases (stationary, and travelling-viewing-past-days) are all correct.

## Deferred (don't build now)

- **`data_timezone` segment ontology.** Sub-day timezone changepoints. Only needed
  for true mid-day-travel precision; per-day `start_timezone` is enough for v1.
- **Surface resolved zone on the today fetch** — see Known limitation above.
- **All-day calendar events.** `12:00 AM Ashura` is an all-day event; after the
  window fix it lands on the right day but still renders "12:00 AM," meaningless
  for all-day. Date-only display bug, orthogonal to timezone — tracked separately.

## Decisions (signed off)

1. **Two timezones.** `home_timezone` = box location (stable, server's own clock,
   fallback floor + scheduling). Per-day `wiki_days.start_timezone` = where the
   owner woke up that day (renders the page "local to where they were").
2. **A day's tz is fixed at the day's start** — "the timezone you woke up in." It
   never drifts mid-day; a move surfaces as **tomorrow** starting in the new zone.
3. **Per-day tz source = the first located point of the day** (init, not modal),
   used identically live and at the EOD lock so they agree on travel days. The
   viewing device's `?tz=` is only a fallback for an in-progress today with no
   located points (web-only / location off); it never re-anchors a day that has a
   GPS track, and never touches a past day.
4. **`home_timezone` is a pure read** (`get_timezone`); seeded once at startup via
   `ensure_home_timezone`, not as a getter side effect.
5. **One zone per day** — `wiki_days.end_timezone` dropped; no sub-day split in v1.
6. **Go-forward-only.** No backfill of past days.
