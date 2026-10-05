/**
 * day.ts - one local day of the record, fetched whole.
 *
 * `/api/timeline/days/:date` serves the day in the zone it woke up in (local
 * midnight to local midnight): its visits, fixes, nights, audio sessions,
 * step bins and calendar, all read from the tables every other room uses.
 * `derive.ts` turns that into what the inspector and the lanes draw.
 */
import { apiGet } from "$lib/api/client";
import type { TimelineDayPoint } from "$lib/wiki/api";

/** A local day's bounds and the zone it is read in. */
export interface DayBounds {
	startMs: number;
	endMs: number;
	zone: string;
}

/** One local day, as `/api/timeline/days/:date` serves it. */
export interface TimelineDay {
	date: string;
	zone: string;
	started_at: string;
	ended_at: string;
	/** Visits overlapping the day, unclipped. */
	stays: {
		id: string;
		started_at: string;
		ended_at: string;
		latitude: number;
		longitude: number;
		place_id: string | null;
		place_name: string | null;
	}[];
	points: TimelineDayPoint[];
	last_point_before: TimelineDayPoint | null;
	/** Nights overlapping the day, each joined from all of its records. */
	nights: { started_at: string; ended_at: string; asleep_minutes: number }[];
	/** speaker_mode: 0 ambient, 1 one voice, 2 a conversation, 3 a group. */
	sessions: { id: string; started_at: string; ended_at: string; speaker_mode: number; content: string | null }[];
	/** Ten-minute bins, stamped at their middle. */
	steps: { at: string; steps: number }[];
	step_scale: number;
	calendar: {
		id: string;
		title: string;
		started_at: string;
		ended_at: string;
		is_all_day: boolean;
		calendar_name: string | null;
		location_name: string | null;
		status: string | null;
		response_status: string | null;
	}[];
}

export async function fetchDay(slug: string): Promise<TimelineDay> {
	return apiGet<TimelineDay>(`/timeline/days/${slug}`);
}

/** The days in `from`..`to` (YYYY-MM-DD, both included) with a location
 *  fix or a recorded conversation, each read over its own local day
 *  (`/timeline/recorded`). */
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

/** An instant on the day's own clock: "6:00 PM" on the same day, "Jun 3 ·
 *  6:00 PM" on an earlier one. */
export function when(ms: number, sameDay: boolean, zone: string): string {
	const d = new Date(ms);
	const time = d.toLocaleTimeString("en-US", { timeZone: zone, hour: "numeric", minute: "2-digit" });
	const date = d.toLocaleDateString("en-US", { timeZone: zone, month: "short", day: "numeric" });
	return sameDay ? time : `${date} · ${time}`;
}

/** What the record did while location was quiet - the prototype's verdict
 *  (`gapWhy`, dayback/src/main.js:2536): talk means the mic was on; any other
 *  sign of the phone (a fix, steps) means on but idle; neither is nothing. */
export function gapVerdict(x: { fixes: number; talk: boolean; steps: boolean }): string {
	if (!x.fixes && !x.talk && !x.steps) return "Nothing recorded";
	if (x.talk) return "Phone on, mic active";
	return "Phone on but idle";
}

/** What a day with no fix says: that no location was recorded; when it was
 *  last measured, and at which stay when that last fix fell inside one; then
 *  `gapVerdict` over the day, when its inputs are known (talk is a window of
 *  two or more voices, as the inspector's signal gap counts it, main.js:2538).
 *  Where you probably were is left to the reader (the owner's call; the
 *  prototype's `honestWhere`, main.js:962-965, titled the day with the last
 *  stay's place). */
export function quietDay(
	lastFix: number | null,
	lastStay: { title: string; s: number; e: number } | null,
	zone: string,
	day: { talk: boolean; steps: boolean } | null,
): { title: string; lines: string[] } {
	const lines: string[] = [];
	if (lastFix !== null) {
		// A stay ends at its last fix, give or take the fix the spine kept.
		const inStay = lastStay && lastFix >= lastStay.s && lastFix <= lastStay.e + 60_000;
		lines.push(`Last measured${inStay ? ` at ${lastStay.title},` : ""} ${when(lastFix, false, zone)}`);
	}
	if (day) lines.push(gapVerdict({ fixes: 0, ...day }));
	return { title: "No location recorded", lines };
}
