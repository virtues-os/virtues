# Cycle tracking

**Status:** Slices 1–3 built on `wave`, unreleased and never run on a device.
Migration 0053 holds the three raw tables (`fd07c088`), 0054 adds the missing
tombstone columns for HealthKit deletions (`0a3cfe8a`), and the phone has a
Cycle stream gated on the box's `api_version` 5. Next is slice 4, the check on
a real phone.

The owner's menstrual cycle, read from Apple Health. **Raw first:** the box
ingests the primitives exactly as Health stores them. Cycles, predictions,
windows and estimates come later as transforms over those rows, which can be
rebuilt whenever a method changes. All of it stays away from the assistant
until the owner turns access on, and the database enforces that, not the
prompt.

## Apple keeps its answers

Apple Health computes period predictions, a fertile window, and a
retrospective ovulation estimate from wrist temperature. **The owner has no
route to any of them**, as of the iOS 26.5 SDK:

- No HealthKit identifier exists for them. They are absent from the headers,
  and the developer-forum requests (threads 699293, 725445, 749026) got no
  answer.
- Shortcuts' "Find Health Samples" reads HealthKit, so it reaches only the
  same types.
- "Export All Health Data" writes HealthKit samples: the inputs, not the
  outputs. This is inferred and not yet checked against a real export; a grep
  of one `export.xml` settles it.
- privacy.apple.com cannot decrypt Health, which is end-to-end encrypted with
  2FA. The EU DMA transfer API covers App Store data only. The EU Data Act
  excludes data derived "through proprietary and complex algorithms".
- The ovulation algorithm is published as "Apple proprietary" (Human
  Reproduction 40(3):469, 2025). Cycle Tracking carries the medical-device mark
  in the EU, which is a plausible reason Apple won't expose its output. Apple
  has never stated one.
- One partial route: Health → Cycle Tracking → Cycle History → Export PDF,
  covering the last 12 months. Whether it shows the ovulation estimates is
  unchecked.

**Everything the owner logs is readable**, as is every sensor input: flow with
its cycle-start flag, spotting, LH, pregnancy and progesterone tests, basal
temperature, mucus, sexual activity, the cycle factors, all 39 symptom types,
Apple's deviation flags (once the owner confirms one), and the nightly
`appleSleepingWristTemperature` (absolute °C; Apple's baseline is not exposed).

These are the inputs HealthKit has no type for:

- the usual cycle and period length typed in at setup;
- a pregnancy's due date or gestational age (there is no metadata key);
- the Cycle Tracking settings themselves.

## Decisions

- **Raw tables, transforms later.** Slice 1 is three tables of samples:
  `data_health_cycle_log`, `data_health_symptom` and
  `data_health_wrist_temperature`. A cycle is not a HealthKit object, so it is
  never stored as a fact.
- **Days, not instants.** These are all-day samples. The phone sends local
  calendar dates and the box stores `date`s next to the instants. Converting a
  UTC midnight on the box puts an entry logged while travelling on the wrong
  day.
- **Read less.** Never request sexual activity, pregnancy tests or
  progesterone tests, so they never leave the phone. Pregnancy and lactation
  wait for a second switch (slice 6). Reading them makes predictions pause
  correctly, but they are the most dangerous rows to hold if a box is seized.
- **Wrist temperature and symptoms are behind the same gate.** A biphasic
  nightly temperature series reveals the cycle on its own. Some symptoms
  (breast pain, vaginal dryness, hot flashes) are reproductive. Loosening a
  gate later is easy; taking access back is not.
- **Assistant access ships off.** These are the only `data_*` tables not
  readable by default.
- **Not for contraception, anywhere.** We inherit Apple's framing: Apple's
  instructions say not to use it as birth control. The only contraceptive app
  with FDA clearance (Natural Cycles) has a 6.5% typical-use failure rate.

## Schema (`0053_add_health_cycle`)

Three tables, each with the standard `data_health_*` tail (`source_stream_id`
UNIQUE = `HKSample.uuid`, `source_table`, `source_provider`,
`deleted_at_source`, `is_archived`, `metadata` holding `healthkit_raw`,
`created_at`, `updated_at`). Each row carries both:

- `started_on` / `ended_on`: the phone's calendar days, inclusive;
- `started_at` / `ended_at`: the sample's instants.

| Table | Value columns |
|---|---|
| `data_health_cycle_log` | `kind`, `value`, `temperature_celsius`, `is_cycle_start` |
| `data_health_symptom` | `symptom`, `value` |
| `data_health_wrist_temperature` | `temperature_celsius`, `night_of` |

How `kind` and `value` map from HealthKit. Raw ints are kept in `metadata`:

| kind | HealthKit | value |
|---|---|---|
| `flow` | `menstrualFlow` (enum `HKCategoryValueVaginalBleeding` since iOS 18, same ints) | 1 unspecified, 2 light, 3 medium, 4 heavy, 5 none |
| `spotting` | `intermenstrualBleeding` | NULL |
| `basal_temperature` | `basalBodyTemperature` | `temperature_celsius` |
| `ovulation_test` | `ovulationTestResult` | 1 negative, 2 lh_surge, 3 indeterminate, 4 estrogen_surge |
| `cervical_mucus` | `cervicalMucusQuality` | 1 dry … 5 egg_white |
| `contraceptive` | `contraceptive` (14.3, interval) | 1 unspecified … 7 patch |
| `pregnancy`, `lactation` | same (14.3, interval), slice 6 | NULL |
| `deviation` | the four iOS 16 read-only types | irregular, infrequent, prolonged, persistent_spotting |

Symptoms map by HealthKit value enum:

- **Severity** types (cramps, headache and most others): unspecified=0,
  not_present=1, mild=2, moderate=3, severe=4. Zero is *unspecified*, not
  absent.
- `mood_changes` and `sleep_changes` use presence: present=0, not_present=1.
- `appetite_changes`: unspecified=0, no_change=1, decreased=2, increased=3.

**The ingest validates before it inserts.** A row with an unknown kind, symptom
or value is logged and skipped. If it reached a CHECK constraint instead, the
whole batch would fail, and the phone retries a failed batch forever (see
[ios-durability-plan.md](ios-durability-plan.md)).

## The privacy gate

- **The grant loop undoes any `REVOKE`.** `ensure_applet_db_grants`
  (`server/faces.rs`) grants SELECT on every `data_%` relation to
  `virtues_face_reader` at every boot. It now skips every ontology marked
  `ModelAccess::OwnerOptIn` and revokes them explicitly. The role is created in
  code, not in a migration, so a migration cannot revoke from it.
- **Most readers bypass grants.** The pool role is a superuser, so code paths
  that feed a model have to check the registry themselves.
  - The new tables have no `day_source`, no lane, no embedding and no SQL
    catalog entry. That keeps them out of day write-ups, the lifeline, search
    and `<coverage>`.
  - `dayline/annotate.rs` writes a window's ontologies into
    `wiki_events.source_ontologies`, which the model can read. It skips
    opt-in ontologies.
- **Turning access on** (slice 5) runs `GRANT`, adds the catalog entries, and
  adds a row to "What leaves" in `agents/build/privacy-boundary.md`. The
  on-state copy may claim zero data retention per request and nothing more.
- **Deletes propagate** (slice 2). A sample deleted in Health is tombstoned
  (`deleted_at_source`) on whichever table holds it, and every health reader
  skips tombstones.
  - A phone that outruns its box loses deletions. An older box logs the
    unknown `deleted` metric type and drops it, and the phone's anchor still
    advances, so those samples stay live on that box. The version gate in
    slice 3 should cover this too.

## Slices

1. **Box, raw streams.** Migration 0053, the `cycle`, `symptom` and
   `wrist_temperature` arms in `applets/ios_ingest/healthkit.rs` (plus the
   `manifest.toml` `writes`), registry entries with `OwnerOptIn`, the grant-loop
   skip, and the annotate skip. Tests: the vocabulary mapping, and a role test
   proving `virtues_face_reader` cannot SELECT the three tables.
2. **Health deletions, all types.** Built.
   - The phone sends `{id: "deleted:<uuid>", metric_type: "deleted",
     deleted_id: "<uuid>"}`. The outbox ignores an id it has already queued,
     so the deletion cannot reuse the sample's id.
   - `ios_ingest` tombstones that `source_stream_id` on every HealthKit table,
     for `source_provider = 'ios'` only.
   - The readers in dayline, census, circumstances and day summary skip
     tombstones, as do the lane measures, the health day sources and the SQL
     catalog note.
3. **Phone.** Built.
   - A Cycle stream on the device screen (`MobileDeviceScreen.svelte`). Its
     Enable button calls `plugin:health|enable_cycle`, which shows a Health
     sheet for the cycle types only and sets its own opt-in flag.
   - One anchor key per type, and full history for these types instead of the
     3-year backfill.
   - The payloads below.
   - **The gate:** an older box logs and drops an unknown `metric_type` while
     the phone's anchor advances, which would lose that data for good. So the
     button stays disabled until the box reports `api_version` 5 or higher
     (`virtues-core/src/api_version.rs`), and an older box shows "Needs a
     server update". Deletions are not gated, so a box below 5 loses them.
   - Wrist temperature's `local_start_date` is the start minus 12 hours, by
     definition. A night that starts after midnight still counts as the
     night before.
4. **Check on a real phone.**
   - Compare row counts with Health.
   - Grep an `export.xml` for anything that isn't a HealthKit sample.
   - Confirm what an all-day sample's instants are.
   - Generate a Cycle History PDF and see whether it holds the ovulation
     estimates.
5. **Transforms and screens** (below), plus the assistant-access switch.
6. **Later.**
   - Pregnancy and lactation behind their own switch.
   - An Apple Health `export.zip` import.
   - In-app logging (same tables, `source_provider = 'virtues'`).

Phone payloads, stream `healthkit`:

```json
{ "id": "<HKSample.uuid>", "metric_type": "cycle", "kind": "flow", "value": "medium",
  "local_start_date": "2026-09-29", "local_end_date": "2026-09-29",
  "timestamp": "2026-09-29T07:00:00.000Z", "end_timestamp": "2026-09-30T07:00:00.000Z",
  "is_cycle_start": true,
  "raw_data": { "hk_type": "HKCategoryTypeIdentifierMenstrualFlow", "hk_value": 3 } }

{ "id": "<uuid>", "metric_type": "symptom", "symptom": "abdominal_cramps", "value": "moderate",
  "local_start_date": "2026-09-29", "local_end_date": "2026-09-29",
  "timestamp": "...", "end_timestamp": "...", "raw_data": { ... } }

{ "id": "<uuid>", "metric_type": "wrist_temperature", "temperature_celsius": 35.91,
  "local_start_date": "2026-09-29",
  "timestamp": "...", "end_timestamp": "...", "raw_data": { ... } }
```

`local_end_date` is inclusive. An all-day sample ends at the next midnight, so
the phone subtracts a moment before taking the date. A basal temperature
reading uses `kind: "basal_temperature"` and `temperature_celsius`.

## Transforms (slice 5)

**Cycles** are a `WITH (security_invoker = true)` view, `data_health_cycle`. A
plain view runs with its owner's rights, and the grant loop grants views. The
rule:

1. A bleed day is a `flow` day whose value is not `none`, with every day of a
   spanning sample expanded. Spotting never counts.
2. Bleed days within 2 days of each other form one bleed.
3. A bleed that starts 10 or more days after the previous period's start opens
   a period; anything sooner is intermenstrual bleeding. This follows Clue's
   rule, and comparing against the previous *accepted* start needs a recursive
   CTE.
4. A cycle runs from one period start to the day before the next.

`is_cycle_start` is stored and not trusted: other apps set it inconsistently.

**Estimates** go in `data_health_cycle_estimate`, apart from what was logged,
with the method and its version on every row. They are rebuilt whole by a core
module beside `dayline/sleep.rs`.

- **Ovulation.** Use a temperature shift: 3 consecutive nights each above the
  max of the previous 6, with a mean ≥0.2 °C over the 6-night baseline.
  Ovulation is the night before the first high one. When no shift is found the
  module gives no estimate, which Apple does too in about 29% of cycles.
- **Next period.** When this cycle has an ovulation estimate, take it plus the
  owner's median luteal length. Otherwise take the median of the last 6
  eligible cycles, shown as a range, and only once there are 3 of them.
- **Fertile window.** Apple's published method: 6 days ending 13 days before
  the predicted start, shifted so a logged LH surge is day 5. Off by default,
  and labeled for trying to conceive.

**The bar** comes from Human Reproduction 2025 (889 cycles confirmed by LH
tests):

| Estimate | MAE | Within the window |
|---|---|---|
| Ovulation, completed cycle | 1.22 d | 89% within ±2 d |
| Ovulation, ongoing cycle | 1.59 d | 80% within ±2 d |
| Next period | 1.65 d | 89% within ±3 d |

We measure ours per owner against their own LH tests and show the result.
Never claim Apple equivalence.

**Screens:**

- A Cycle room with one row per cycle, aligned on day 1 (mockup:
  <https://claude.ai/artifact/Wwy4p3wTUXV4sVyHj44gTu>).
- A day-page margin line.
- Settings switches.
- Copy says "cycle", "period" and "spotting". Never "late", "abnormal" or
  "women's health".

## Unverified

- That Health app samples span exactly local midnight to midnight.
- How an ongoing contraception factor's end date is stored.
- Wrist temperature: one sample per night, and what its `startDate` means.
- Whether `export.xml` or the Cycle History PDF contains Apple's estimates.

## Sources

HealthKit headers, iPhoneOS 26.5 SDK · Apple Cycle Tracking IFU Rev D (2023),
`apple.com/legal/ifu/cycle-tracking` · support.apple.com 120356, 120357,
102208, HT213275 · Human Reproduction 40(3):469–478 (2025),
doi:10.1093/humrep/deaf005 · Bull et al., npj Digit Med 2:83 (2019) · Apple
Women's Health Study, npj Digit Med 6:100 (2023) · Munro et al., FIGO AUB
System 1, IJGO 143:393 (2018) · openEHR `menstrual_diary.v1` · Clue
period-start rules · FTC v. Flo Health (2021) · Washington My Health My Data
Act · App Store Review Guidelines 5.1.2(i), 5.1.3.
