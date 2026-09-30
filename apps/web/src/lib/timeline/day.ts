/**
 * day.ts - one local day of the record, fetched whole.
 *
 * /api/timeline/day cuts its days at UTC midnight, so a local day spans two of
 * them. The Timeline fetches every UTC day the local day touches, plus the one
 * before, so the day's first fix can be judged against the fix before it (a
 * hole across midnight is still a hole). Visits come back under the UTC day
 * they started in, so neighbouring days never repeat one.
 *
 * The day is the viewer's local day for now. A day recorded while travelling
 * belongs to that day's own zone (Virtues' timezone model); that comes later.
 */
import { apiGet } from "$lib/api/client";
import { getDayFacts, getDaySources, type TimelineDayLocationChunk, type TimelineDayPoint, type TimelineDayView } from "$lib/wiki/api";

const DAY_MS = 86_400_000;

/** The local day a YYYY-MM-DD slug names, as [startMs, endMs). */
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

const MOS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const fmtT = (d: Date) => `${d.getHours() % 12 || 12}:${String(d.getMinutes()).padStart(2, "0")} ${d.getHours() < 12 ? "AM" : "PM"}`;

/** When location was last measured, in the prototype's words (`honestWhere`,
 *  dayback/src/main.js:958): "last measured at 6:00 PM" on the same day,
 *  "last measured Sep 23 · 6:00 PM" on an earlier one. */
export function lastMeasured(ms: number, sameDay: boolean): string {
	const d = new Date(ms);
	return `last measured ${sameDay ? "at " + fmtT(d) : `${MOS[d.getMonth()]} ${d.getDate()} · ${fmtT(d)}`}`;
}

/** What the record did while location was quiet - the prototype's verdict
 *  (`gapWhy`, dayback/src/main.js:2536): talk means the mic was on; any other
 *  sign of the phone (a fix, steps) means on but idle; neither is nothing. */
export function gapVerdict(x: { fixes: number; talk: boolean; steps: boolean }): string {
	if (!x.fixes && !x.talk && !x.steps) return "Nothing recorded";
	if (x.talk) return "Phone on, mic active";
	return "Phone on but idle";
}

/** `gapVerdict` for a whole day with no fixes: the mic's recorded minutes and
 *  whether any steps arrived. */
export async function quietDayVerdict(slug: string): Promise<string> {
	const [facts, sources] = await Promise.all([getDayFacts(slug), getDaySources(slug)]);
	return gapVerdict({
		fixes: 0,
		talk: (facts?.recorded_minutes ?? 0) > 0,
		steps: sources.some((s) => s.source_type === "steps"),
	});
}
