---
name: strava-activities-sync
description: "Brings in your runs, rides, swims, and other workouts from Strava."
metadata:
  invoke: none
  trigger: cron 0 */30 * * * *
  run: strava_activities_sync
  source: strava
  writes: health_workout
---

