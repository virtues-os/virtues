/**
 * Per-day activity, shaped for the heatmap.
 *
 * The heatmap's range is the days it is handed, so the window is filled here:
 * the API returns only days that have a row, and a gap at either end would
 * otherwise shorten the calendar instead of drawing as empty squares.
 *
 * The levels are a GitHub-style relative scale: quartiles of the window's
 * non-zero event counts bound levels 1-4, so the coloring adapts to however
 * dense this particular life's record happens to be rather than assuming a
 * fixed "busy" threshold.
 */

import { getLocalDateSlug } from '$lib/utils/dateUtils';
import type { DayActivityApi } from './api';

/** One heatmap square: a calendar day as YYYY-MM-DD and how many events it holds. */
export interface ActivityDay {
	date: string;
	count: number;
}

/** Every day from `start` to `end` inclusive, oldest first, zero where the API had no row. */
export function toActivityDays(activity: DayActivityApi[], start: Date, end: Date): ActivityDay[] {
	const counts = new Map(activity.map((d) => [d.date, d.event_count]));
	const days: ActivityDay[] = [];
	const cursor = new Date(start.getFullYear(), start.getMonth(), start.getDate());
	const last = getLocalDateSlug(end);
	for (;;) {
		const date = getLocalDateSlug(cursor);
		days.push({ date, count: counts.get(date) ?? 0 });
		if (date >= last) break;
		cursor.setDate(cursor.getDate() + 1);
	}
	return days;
}

/** Upper bounds for levels 1-3, strictly increasing so the legend never reads "9 to 8 events". */
export function activityThresholds(days: ActivityDay[]): [number, number, number] {
	const counts = days
		.map((d) => d.count)
		.filter((c) => c > 0)
		.sort((a, b) => a - b);
	const quantile = (p: number) =>
		counts[Math.min(counts.length - 1, Math.floor(p * counts.length))] ?? 1;
	const a = Math.max(1, quantile(0.25));
	const b = Math.max(a + 1, quantile(0.5));
	const c = Math.max(b + 1, quantile(0.75));
	return [a, b, c];
}
