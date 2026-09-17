/**
 * What the wiki's components need that the wire does not already say.
 *
 * This barrel used to export a `WikiPage` discriminated union of five page
 * types, their guards, and fourteen supporting types. Reference-counting them
 * settled it: thirteen of the fourteen had no consumer outside this directory,
 * and the fourteenth — `Citation` — was shadowed, since every real consumer
 * imports from `$lib/types/Citation` instead.
 *
 * The union's `type` field existed only so the guards could read it, and the
 * guards existed only so one `{#if}` chain in WikiContent could dispatch. That
 * chain now switches on a `kind` set beside the fetch that decides it.
 */
export type { DayEvent, ScoredSleepCycle } from "./day";
export { getEventDisplayLabel, getEventDisplayLocation } from "./day";
