/**
 * day.ts - one local day of the record, fetched whole.
 *
 * /api/timeline/day cuts its days at UTC midnight, so a local day spans two of
 * them. The Timeline fetches every UTC day the local day touches, plus the one
 * before, so the day's first fix can be judged against the fix before it (a
 * hole across midnight is still a hole). Visits come back under the UTC day
 * they started in, so neighbouring days never repeat one.
 *
 * A day's bounds come from the server (`/timeline/day-window`): local midnight
 * in the zone the day woke up in, to where the next day begins - so a day
 * recorded while travelling keeps its own clock.
 */
import { apiGet } from "$lib/api/client";
import type { TimelineDayLocationChunk, TimelineDayPoint, TimelineDayView } from "$lib/wiki/api";
import type { DerivedWindow, VoiceWindow } from "./rail";
import type { LaneWindow } from "./lanes";

const DAY_MS = 86_400_000;

/** A local day's bounds and the zone it is read in. */
export interface DayBounds {
	startMs: number;
	endMs: number;
	zone: string;
}

/** The day a YYYY-MM-DD slug names, in the zone it woke up in. */
export async function fetchDayBounds(slug: string): Promise<DayBounds> {
	const w = await apiGet<{ zone: string; started_at: string; ended_at: string }>(`/timeline/day-window/${slug}`);
	return { startMs: Date.parse(w.started_at), endMs: Date.parse(w.ended_at), zone: w.zone };
}

/** The server's derived stays, drives, gaps, nights and moments over a day
 *  (`/timeline/derived`, rebuilt from the raw record every 15 minutes). */
export async function fetchDerived(b: DayBounds): Promise<DerivedWindow> {
	const q = new URLSearchParams({ start: new Date(b.startMs).toISOString(), end: new Date(b.endMs).toISOString() });
	return apiGet<DerivedWindow>(`/timeline/derived?${q}`);
}

/** Every transcription window over a day (`/timeline/voice`): the rail's
 *  conversations and transcripts, and whether the mic was on. */
export async function fetchVoice(b: DayBounds): Promise<VoiceWindow[]> {
	const q = new URLSearchParams({ start: new Date(b.startMs).toISOString(), end: new Date(b.endMs).toISOString() });
	return apiGet<VoiceWindow[]>(`/timeline/voice?${q}`);
}

/** The scrubber's Body, Calendar and Finance lanes over a window
 *  (`/timeline/lanes`). */
export async function fetchLanes(b: DayBounds): Promise<LaneWindow> {
	const q = new URLSearchParams({ start: new Date(b.startMs).toISOString(), end: new Date(b.endMs).toISOString() });
	return apiGet<LaneWindow>(`/timeline/lanes?${q}`);
}

/** The days in `from`..`to` (YYYY-MM-DD, both included) that hold any
 *  record - a location fix, a transcription window or a step reading - each
 *  read over its own local day (`/timeline/recorded`). */
export async function fetchRecorded(from: string, to: string): Promise<string[]> {
	return apiGet<string[]>(`/timeline/recorded?${new URLSearchParams({ from, to })}`);
}

/** The day a YYYY-MM-DD slug names in the browser's zone, as [startMs, endMs):
 *  for naming and stepping dates, never for cutting the record. */
export function localDay(slug: string): { startMs: number; endMs: number } {
	const [y, m, d] = slug.split("-").map(Number);
	return { startMs: new Date(y, m - 1, d).getTime(), endMs: new Date(y, m - 1, d + 1).getTime() };
}

/** The slug `n` days from `slug` (negative steps back). */
export function stepDay(slug: string, n: number): string {
	const [y, m, d] = slug.split("-").map(Number);
	const t = new Date(y, m - 1, d + n);
	return `${t.getFullYear()}-${String(t.getMonth() + 1).padStart(2, "0")}-${String(t.getDate()).padStart(2, "0")}`;
}

/** The endpoint's UTC dates covering [startMs, endMs), and the day before. */
export function utcDatesFor(startMs: number, endMs: number): string[] {
	const dates = new Set([startMs - DAY_MS, startMs, endMs - 1].map((t) => new Date(t).toISOString().slice(0, 10)));
	return [...dates].sort();
}

/** Every raw fix and visit across the window around a local day, and the last
 *  fix before the window, for a day that has none of its own. */
export async function fetchDayWindow(
	startMs: number,
	endMs: number,
): Promise<{ points: TimelineDayPoint[]; visits: TimelineDayLocationChunk[]; before: TimelineDayPoint | null }> {
	const days = await Promise.all(utcDatesFor(startMs, endMs).map((d) => apiGet<TimelineDayView>(`/timeline/day/${d}`)));
	return {
		points: days.flatMap((v) => v.points ?? []),
		visits: days.flatMap((v) => v.chunks ?? []).filter((c): c is TimelineDayLocationChunk => c.type === "location"),
		before: days[0]?.last_point_before ?? null,
	};
}

/** When location was last measured, in the prototype's words (`honestWhere`,
 *  dayback/src/main.js:958), on the day's own clock: "last measured at
 *  6:00 PM" on the same day, "last measured Sep 23 · 6:00 PM" on an earlier one. */
export function lastMeasured(ms: number, sameDay: boolean, zone: string): string {
	const d = new Date(ms);
	const time = d.toLocaleTimeString("en-US", { timeZone: zone, hour: "numeric", minute: "2-digit" });
	const date = d.toLocaleDateString("en-US", { timeZone: zone, month: "short", day: "numeric" });
	return `last measured ${sameDay ? `at ${time}` : `${date} · ${time}`}`;
}

/** What the record did while location was quiet - the prototype's verdict
 *  (`gapWhy`, dayback/src/main.js:2536): talk means the mic was on; any other
 *  sign of the phone (a fix, steps) means on but idle; neither is nothing. */
export function gapVerdict(x: { fixes: number; talk: boolean; steps: boolean }): string {
	if (!x.fixes && !x.talk && !x.steps) return "Nothing recorded";
	if (x.talk) return "Phone on, mic active";
	return "Phone on but idle";
}

/** What a day with no fix says (`honestWhere`, dayback/src/main.js:962-965):
 *  the last place you stayed and when you left it; with no stay before the
 *  day at all, that GPS was silent. Then `gapVerdict` over the day, when its
 *  inputs are known: talk is a window of two or more voices, as the rail's
 *  signal gap counts it (main.js:2538). */
export function quietDay(
	last: { title: string; e: number } | null,
	zone: string,
	day: { talk: boolean; steps: boolean } | null,
): { title: string; lines: string[] } {
	const where = last
		? { title: last.title, line: sentence(lastMeasured(last.e, false, zone)) }
		: { title: "No location fix", line: "GPS silent at this moment" };
	return { title: where.title, lines: day ? [where.line, gapVerdict({ fixes: 0, ...day })] : [where.line] };
}

const sentence = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);
