# Cycle tracking

**Status:** Planned. Migration `0053_add_health_cycle` is claimed
(`.sql.pending`; rename it when the SQL lands). Nothing else is built.

The owner's menstrual cycle, read from Apple Health. The box stores what was
logged, rebuilds the cycles and Apple's estimates from it, and keeps all of it
away from the assistant until the owner turns access on. The database enforces
that, not the prompt.

## Apple keeps its answers

Apple Health computes period predictions, a fertile window, and a
retrospective ovulation estimate from wrist temperature. **The owner has no
route to any of them**, as of the iOS 26.5 SDK:

- No HealthKit identifier exists for them. They are absent from the headers, and
  the developer-forum requests (threads 699293, 725445, 749026) got no answer.
- Shortcuts' "Find Health Samples" reads HealthKit, so it reaches only the same
  types.
- "Export All Health Data" writes HealthKit samples, so it holds the inputs but
  not the outputs. This is inferred and not yet checked against a real export.
  A grep of one `export.xml` settles it.
- privacy.apple.com cannot decrypt Health, because it is end-to-end encrypted
  with 2FA. The EU DMA transfer API covers App Store data only. The EU Data
  Act excludes data derived "through proprietary and complex algorithms".
- The ovulation algorithm is published as "Apple proprietary" (Human
  Reproduction 40(3):469, 2025). Cycle Tracking carries the medical-device mark
  in the EU, which is a plausible reason Apple won't expose its output. Apple
  has never stated one.
- One partial route: Health → Cycle Tracking → Cycle History → Export PDF,
  covering the last 12 months. Whether it shows the ovulation estimates is
  unchecked.

**Every input is readable, though.** That includes flow with its cycle-start
flag, LH tests, basal temperature, cycle factors, Apple's own deviation flags,
and the nightly `appleSleepingWristTemperature` (absolute °C; Apple's baseline
is not exposed). So the box rebuilds the outputs itself, labels them as the
box's estimates rather than Apple's, and measures them against the owner's own
LH tests.

## Decisions

- **Three layers.** Logged samples go in `data_health_cycle_log`. The cycles
  are the view `data_health_cycle`, one row per cycle and never stored as fact.
  The box's estimates go in `data_health_cycle_estimate`, kept apart from what
  was logged and carrying the method and its version. This follows openEHR's
  diary → cycle → summary split.
- **Days, not instants.** These are all-day samples. The phone sends local
  calendar dates and the box stores `date`s. Converting a UTC midnight on the
  box puts an entry logged while travelling on the wrong day.
- **Read less.** Never request sexual activity, pregnancy tests or
  progesterone tests, so they never leave the phone. Pregnancy and lactation
  wait for a second switch (slice 5). Reading them makes predictions pause
  correctly, but they are the most dangerous rows to hold if a box is seized.
- **Wrist temperature is cycle data.** A biphasic nightly series reveals the
  cycle on its own, so it sits behind the same switch and the same grant.
- **Assistant access ships off.** This is the only `data_*` table not readable
  by default. Turning it on runs a real `GRANT`.
- **Symptoms go elsewhere.** Headache, fatigue and the rest are general in
  HealthKit. They belong in a later `data_health_symptom`, not here.
- **Not for contraception, anywhere.** We inherit Apple's framing: Apple's
  instructions say not to use it as birth control. The only contraceptive app
  with FDA clearance (Natural Cycles) has a 6.5% typical-use failure rate.

## Schema (`0053_add_health_cycle`)

```sql
CREATE TABLE public.data_health_cycle_log (
    id               text PRIMARY KEY,
    kind             text NOT NULL,
    value            text,
    temperature_c    double precision,        -- basal_temperature only
    started_on       date NOT NULL,           -- the phone's calendar day
    ended_on         date,                    -- inclusive; NULL = interval still running
    started_at       timestamptz NOT NULL,
    ended_at         timestamptz,
    is_cycle_start   boolean,                 -- HKMetadataKeyMenstrualCycleStart, as logged
    -- standard tail, as data_health_sleep
    source_stream_id text NOT NULL UNIQUE,    -- HKSample.uuid
    source_table     text NOT NULL,
    source_provider  text NOT NULL,
    deleted_at_source timestamptz,
    is_archived      boolean DEFAULT false NOT NULL,
    metadata         jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at       timestamptz DEFAULT now() NOT NULL,
    updated_at       timestamptz DEFAULT now() NOT NULL,
    CHECK (kind IN ('flow','spotting','basal_temperature','ovulation_test',
                    'cervical_mucus','contraceptive','pregnancy','lactation','deviation')),
    CHECK (CASE kind
        WHEN 'flow'           THEN value IN ('unspecified','light','medium','heavy','none')
        WHEN 'ovulation_test' THEN value IN ('negative','lh_surge','estrogen_surge','indeterminate')
        WHEN 'cervical_mucus' THEN value IN ('dry','sticky','creamy','watery','egg_white')
        WHEN 'contraceptive'  THEN value IN ('unspecified','implant','injection','iud','ring','oral','patch')
        WHEN 'deviation'      THEN value IN ('irregular','infrequent','prolonged','persistent_spotting')
        ELSE value IS NULL END),
    CHECK ((kind = 'basal_temperature') = (temperature_c IS NOT NULL)),
    CHECK (ended_on IS NOT NULL OR kind IN ('contraceptive','pregnancy','lactation')),
    CHECK (ended_on IS NULL OR ended_on >= started_on)
);

CREATE TABLE public.data_health_wrist_temperature (
    id               text PRIMARY KEY,
    night_of         date NOT NULL,           -- the calendar day the sleep began
    temperature_c    double precision NOT NULL,
    -- same standard tail
);

CREATE TABLE public.data_health_cycle_estimate (
    id               text PRIMARY KEY,        -- deterministic: (kind, cycle_started_on, method_version)
    kind             text NOT NULL CHECK (kind IN ('ovulation','next_period','fertile_window')),
    cycle_started_on date NOT NULL,           -- joins data_health_cycle.started_on
    estimated_on     date,                    -- ovulation day, or range start
    estimated_until  date,                    -- range end; NULL for a single day
    signal_c         double precision,        -- ovulation: the temperature shift found
    method           text NOT NULL,           -- 'temperature_shift', 'calendar', 'luteal'
    method_version   integer NOT NULL,
    computed_at      timestamptz DEFAULT now() NOT NULL
);
```

HealthKit maps onto `kind` as follows. Raw ints go in `metadata.healthkit_raw`:

| kind | HealthKit | value |
|---|---|---|
| `flow` | `menstrualFlow` (enum `HKCategoryValueVaginalBleeding` since iOS 18, same ints) | 1 unspecified, 2 light, 3 medium, 4 heavy, 5 none |
| `spotting` | `intermenstrualBleeding` | NULL |
| `basal_temperature` | `basalBodyTemperature` | `temperature_c` |
| `ovulation_test` | `ovulationTestResult` | 1 negative, 2 lh_surge, 3 indeterminate, 4 estrogen_surge |
| `cervical_mucus` | `cervicalMucusQuality` | 1 dry … 5 egg_white |
| `contraceptive` | `contraceptive` (14.3, interval) | 1 unspecified … 7 patch |
| `pregnancy`, `lactation` | same (14.3, interval), slice 5 | NULL |
| `deviation` | the four iOS 16 read-only types | which one |

**The cycle view** must be `WITH (security_invoker = true)`. A view otherwise
runs with its owner's rights, and the grant loop grants views. The rule:

1. A bleed day is a `flow` day whose value is not `none`, with every day of a
   spanning sample expanded. Spotting never counts.
2. Bleed days within 2 days of each other form one bleed.
3. A bleed that starts 10 or more days after the previous period's start opens
   a period. Anything sooner counts as intermenstrual bleeding. This follows
   Clue's rule.
4. A cycle runs from one period start to the day before the next. Columns:
   `cycle_number, started_on, ended_on, period_days, length_days, is_current,
   overlaps_pause`.

`is_cycle_start` is stored and not trusted. Other apps set it inconsistently,
so one rule gives one answer. Rule 3 compares against the previous *episode*
in plain SQL. A run of short bleeds (day 0, 8, 16) needs a recursive CTE to
compare against the previous *accepted* start, so write that version and test
it.

## The estimates

These are rebuilt whole, using an algorithm in a core module beside
`dayline/sleep.rs`, after any ingest batch that touched cycle or temperature
rows. A whole-table replace per `method_version` keeps them reproducible.

- **Ovulation (`temperature_shift`).** Compute a nightly baseline as the mean
  of the previous 6 nights. Find the first night k where 3 consecutive nights
  each exceed the max of the previous 6, and their mean is ≥0.2 °C above the
  baseline. Ovulation is k − 1. **Abstain** when there is no such shift or when
  the cycle has under about 70% night coverage. Apple produces an estimate in
  about 71% of cycles, so abstaining is a correct outcome and not a gap. Only
  completed cycles, or the current cycle after 3 high nights.
- **Next period.** If this cycle has an ovulation estimate, the next period is
  ovulation plus the owner's median luteal length (or 13 days with fewer than 3
  measured). Otherwise use the calendar: the median of the last 6 eligible
  cycles, shown as a min–max range. Eligible means complete, no pause, not
  flagged. Use nothing until there are 3 eligible cycles. Use nothing during
  contraception.
- **Fertile window.** Apple's published method, reproduced exactly: 6 days
  ending 13 days before the predicted next start. When a positive LH test is
  logged, shift the window so day 5 is that test. It is shown only as "for
  trying to conceive" and is off by default (a setting in slice 4).
- **Possibly unlogged.** A cycle longer than 1.6 × median is flagged, left out
  of the ranges, and dismissable.

**The bar to meet**, from Human Reproduction 2025 (n=889 cycles confirmed by
LH tests) and Apple's instructions:

| | Apple |
|---|---|
| Ovulation, completed cycle | MAE 1.22 d, 89% within ±2 d |
| Ovulation, ongoing cycle | MAE 1.59 d, 80% within ±2 d |
| Next period | MAE 1.65 d, 89% within ±3 d |

**We measure ours per owner** against their own positive LH tests, as
ovulation ≈ LH + 1 day, and show the result on the Cycle page: "Matched your
LH tests within 2 days in 7 of 8 cycles." With no LH tests logged, the page
says it has nothing to check against. Never claim Apple equivalence. Apple's
environmental correction and baseline are proprietary.

## The privacy gate

- **The grant loop undoes any `REVOKE`.** `ensure_applet_db_grants`
  (`server/faces.rs`) grants SELECT on every `data_%` relation to
  `virtues_face_reader` at every boot. It must skip the four cycle relations
  unless assistant access is on, and that change must land with the migration.
- **Most readers bypass grants.** The pool role is a superuser, so the day
  write-up (`get_day_sources` in `api/day_summary.rs`), lifeline, census and
  circumstances read everything regardless of grants. Add a field to
  `OntologyDescriptor`, `model_access: ModelAccess::{Default, OwnerOptIn}`. It
  drives the sql catalog entry, the grant loop's skip list, and every
  model-facing evidence gather. Sweep every reader of `ontologies()` and
  `day_source` for model-facing paths.
- **Turning it on** runs `GRANT`. Turning it off runs `REVOKE`. The on-state
  copy may claim zero data retention per request and nothing more. It must name
  what leaves the box, as `agents/build/privacy-boundary.md` requires. Add the
  row there in the same change.
- **Deletes must propagate.** Today no health type handles HealthKit
  deletions: the collector ignores `HKAnchoredObjectQuery`'s deleted objects,
  and nothing sets `deleted_at_source`. A cycle entry deleted on the phone
  would stay on the box, so slice 0 fixes this for every type.

## Slices

0. **Health deletions, all types.** The phone sends
   `{id, metric_type: "<type>_deleted"}`. `ios_ingest` sets
   `deleted_at_source`. Every health reader filters on it.
1. **Box.** Migration 0053, the `cycle` and `wrist_temperature` arms in
   `applets/ios_ingest/healthkit.rs` (plus `manifest.toml` `writes`), the
   registry entries with `OwnerOptIn`, the grant-loop skip, and the evidence
   gates.
   - Tests: view fixtures (a spanning sample, a 1-day gap inside a period,
     mid-cycle spotting, the 0/8/16 chain, a timezone change, a deleted
     sample). Also a role test proving `virtues_face_reader` cannot SELECT any
     of the four relations.
2. **Phone.** A "Track your cycle" switch that requests only the slice-1
   types, one anchor key per type, and the payload below.
   - **Trap:** an older box logs and drops an unknown `metric_type`, and the
     phone's anchor still advances. Gate the switch on the box's `api_version`
     (see [version-compat-plan.md](version-compat-plan.md)), or that data is
     lost the way sleep's was before `sleep_refetch_v1`.
3. **Estimates.** The module above, `method_version = 1`, the LH check, and
   unit tests on synthetic temperature series (a clean shift, no shift, a
   fever spike, missing nights).
4. **Screens.** A Cycle room under Health, one row per cycle aligned on day 1
   (mockup: <https://claude.ai/artifact/Wwy4p3wTUXV4sVyHj44gTu>). Also a
   margin line on day pages ("Day 4, period"), and Settings switches for
   tracking, assistant access and the fertile window. Copy: "cycle", "period",
   "spotting"; "Likely next period"; never "late", "abnormal" or "women's
   health". "No period expected" is a valid state. Hyphens, not em dashes.
5. **Later.** Pregnancy and lactation behind their own switch, symptoms,
   Apple Health `export.zip` import for history older than the 3-year
   backfill, and in-app logging (same table, `source_provider = 'virtues'`).

Phone payload, stream `healthkit`:

```json
{ "id": "<HKSample.uuid>", "metric_type": "cycle", "kind": "flow", "value": "medium",
  "temperature_c": null, "local_start_date": "2026-09-29", "local_end_date": "2026-09-29",
  "timestamp": "2026-09-29T07:00:00.000Z", "end_timestamp": "2026-09-30T07:00:00.000Z",
  "is_cycle_start": true,
  "raw_data": { "hk_type": "HKCategoryTypeIdentifierMenstrualFlow", "hk_value": 3 } }
```

## Unverified

- That Health app samples span exactly local midnight to midnight. This is
  observed, not documented.
- How an ongoing contraception factor's end date is stored.
- Whether `export.xml` or the Cycle History PDF contains Apple's estimates.
  Check both on a real phone before slice 5.
- Wrist-temperature details: one sample per night, and what `startDate` means
  for it.

## Sources

HealthKit headers, iPhoneOS 26.5 SDK · Apple Cycle Tracking IFU Rev D (2023),
`apple.com/legal/ifu/cycle-tracking` · support.apple.com 120356, 120357,
102208, HT213275 · Human Reproduction 40(3):469–478 (2025),
doi:10.1093/humrep/deaf005 · Bull et al., npj Digit Med 2:83 (2019) · Apple
Women's Health Study, npj Digit Med 6:100 (2023) · Munro et al., FIGO AUB
System 1, IJGO 143:393 (2018) · openEHR `menstrual_diary.v1` · Clue
period-start rules · FTC v. Flo Health (2021) · Washington My Health My Data
Act · App Store Review Guidelines 5.1.2(i), 5.1.3.
