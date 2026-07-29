/**
 * Shared mapping from per-day activity to heatmap intensity.
 *
 * GitHub-style relative scale: quartiles of the window's non-zero event
 * counts map to levels 1-4, so the coloring adapts to however dense this
 * particular life's record happens to be rather than assuming a fixed
 * "busy" threshold.
 */

import type { DayActivityApi } from './api';

export function toActivityLevels(days: DayActivityApi[]): Map<string, number> {
	const counts = days
		.map((d) => d.event_count)
		.filter((c) => c > 0)
		.sort((a, b) => a - b);
	const quantile = (p: number) =>
		counts[Math.min(counts.length - 1, Math.floor(p * counts.length))] ?? 1;
	const [q1, q2, q3] = [quantile(0.25), quantile(0.5), quantile(0.75)];

	const levels = new Map<string, number>();
	for (const day of days) {
		if (day.event_count <= 0) continue;
		const level =
			day.event_count > q3 ? 4
			: day.event_count > q2 ? 3
			: day.event_count > q1 ? 2
			: 1;
		levels.set(day.date, level);
	}
	return levels;
}
