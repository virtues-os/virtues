/**
 * The day line: one day's events on the day's own clock, each as tall as it
 * was unlike the owner's usual, with the record's coverage streams under it.
 *
 * Everything here is positioned by INSTANT, as a fraction of the day's real
 * window from local midnight to the next local midnight. A DST day is 23 or 25
 * hours long and the axis is too; an event that ends at the next midnight sits
 * at the right edge, never back at zero.
 */
import { zoneOffset } from "$lib/timeline/scale";
import type { LifelineLane } from "./api";
import { getEventDisplayLabel, type DayEvent } from "./types";

/** A day's window, `[startMs, endMs)`, and the zone that cut it. */
export interface DayWindow {
	startMs: number;
	endMs: number;
	zone: string | null;
}

/** A zone the browser can format in, else null (the browser's own zone). */
function usableZone(zone: string | null): string | null {
	if (!zone) return null;
	try {
		new Intl.DateTimeFormat("en-US", { timeZone: zone });
		return zone;
	} catch {
		return null;
	}
}

/**
 * The instant a wall-clock hour on `slug` falls at in `zone`; hour 24 is the
 * next day's midnight. Two passes settle the zone's offset across a DST change.
 */
export function localInstant(slug: string, hour: number, zone: string | null): number {
	const [y, m, d] = slug.split("-").map(Number);
	const z = usableZone(zone);
	if (!z) return new Date(y, m - 1, d, hour).getTime();
	const wall = Date.UTC(y, m - 1, d, hour);
	const first = wall - zoneOffset(wall, z);
	return wall - zoneOffset(first, z);
}

/** The day `slug` names, local midnight to local midnight in `zone`. */
export function dayWindow(slug: string, zone: string | null): DayWindow {
	return { startMs: localInstant(slug, 0, zone), endMs: localInstant(slug, 24, zone), zone: usableZone(zone) };
}

/** Where an instant falls across the day, 0 at the first midnight and 1 at the next. */
export function dayFraction(ms: number, w: DayWindow): number {
	const span = w.endMs - w.startMs;
	if (span <= 0) return 0;
	return Math.min(1, Math.max(0, (ms - w.startMs) / span));
}

/** "9:44 PM" on the day's clock. */
export function clockLabel(ms: number, zone: string | null): string {
	return new Date(ms).toLocaleTimeString("en-US", {
		hour: "numeric",
		minute: "2-digit",
		timeZone: usableZone(zone) ?? undefined,
	});
}

/** Hour marks every `every` hours of local clock time, labelled "6 AM". */
export function hourTicks(slug: string, zone: string | null, every = 3): { at: number; label: string }[] {
	const w = dayWindow(slug, zone);
	const out: { at: number; label: string }[] = [];
	for (let h = 0; h <= 24; h += every) {
		const h12 = h % 12 === 0 ? 12 : h % 12;
		out.push({ at: dayFraction(localInstant(slug, h, zone), w), label: `${h12} ${h % 24 < 12 ? "AM" : "PM"}` });
	}
	return out;
}

// ── how unlike your usual ─────────────────────────────────────────────────

/** The z past which an event reads as unlike the owner's usual. */
export const UNLIKE_USUAL_Z = 1;
/** The chart's ceiling and floor, in standard deviations. */
export const Z_LIMIT = 3;

/**
 * How unlike the owner's usual an event was: the local score (off-pattern for
 * its kind) when there is one, else the global one. One number, so the day
 * line and the event list never disagree about which event stood out.
 */
export function usualScore(e: DayEvent): number | null {
	return e.localNoveltyZ ?? e.noveltyZ ?? null;
}

/** The words a score earns on hover. */
export function usualWords(z: number | null): string {
	if (z === null) return "Your server hasn't scored this";
	return z >= UNLIKE_USUAL_Z ? "Unlike your usual" : "Like your usual";
}

/** The event furthest from the owner's usual, when one is far enough to say so. */
export function mostUnlikeUsual(events: DayEvent[]): string | null {
	let best: { id: string; z: number } | null = null;
	for (const e of events) {
		if (e.userHidden || e.isUnknown) continue;
		const z = usualScore(e);
		if (z !== null && z >= UNLIKE_USUAL_Z && (!best || z > best.z)) best = { id: e.id, z };
	}
	return best?.id ?? null;
}

/** One event on the day line. `z` is clamped to the chart; null is unscored. */
export interface DayLineBlock {
	id: string;
	label: string;
	startMs: number;
	endMs: number;
	x1: number;
	x2: number;
	z: number | null;
}

/**
 * The day's known events as blocks, oldest first. Unknown stretches and hidden
 * events are left out: the line stays empty where the record could not say
 * what happened.
 */
export function dayLineBlocks(events: DayEvent[], w: DayWindow): DayLineBlock[] {
	return events
		.filter((e) => !e.userHidden && !e.isUnknown)
		.map((e) => {
			const z = usualScore(e);
			return {
				id: e.id,
				label: getEventDisplayLabel(e),
				startMs: e.startTime.getTime(),
				endMs: e.endTime.getTime(),
				x1: dayFraction(e.startTime.getTime(), w),
				x2: dayFraction(e.endTime.getTime(), w),
				z: z === null ? null : Math.max(-Z_LIMIT, Math.min(Z_LIMIT, z)),
			};
		})
		.filter((b) => b.x2 > b.x1)
		.sort((a, b) => a.startMs - b.startMs);
}

// ── coverage streams ──────────────────────────────────────────────────────

/**
 * The streams under the day line, each read from the lifeline lanes named.
 * `bridgeMin` is the silence a SAMPLED stream keeps between readings while it
 * is still listening: a phone that fixes its location every twenty minutes
 * covered the whole stretch, and drawing it as a comb would say otherwise.
 * A stream of discrete things (a message) bridges nothing.
 */
export const COVERAGE_STREAMS: { label: string; lanes: string[]; bridgeMin: number }[] = [
	{ label: "Audio", lanes: ["communication/transcription"], bridgeMin: 10 },
	{ label: "Location", lanes: ["location"], bridgeMin: 30 },
	{ label: "Messages", lanes: ["communication/message"], bridgeMin: 0 },
	{ label: "Health", lanes: ["health"], bridgeMin: 0 },
	{ label: "Screen", lanes: ["activity"], bridgeMin: 0 },
];

/** The lifeline lanes `COVERAGE_STREAMS` reads must be split out of. */
export const COVERAGE_EXPAND = ["communication"];

/** A stream and the stretches of the day it recorded, as `[from, to]` fractions. */
export interface CoverageLane {
	label: string;
	runs: [number, number][];
}

/**
 * Each stream that was collecting by the day's end, with the buckets it
 * recorded in merged into runs across its own sampling gaps. A stream that
 * had not started yet is left out; one that had started and recorded nothing
 * shows as an empty lane, because a silent stream on a watched day is
 * something the reader should see.
 */
export function coverageLanes(lanes: LifelineLane[], w: DayWindow): CoverageLane[] {
	const out: CoverageLane[] = [];
	for (const stream of COVERAGE_STREAMS) {
		const members = lanes.filter((l) => stream.lanes.includes(l.id));
		const watching = members.some((l) => l.first_seen !== null && Date.parse(l.first_seen) < w.endMs);
		if (!watching) continue;
		const n = Math.max(0, ...members.map((l) => l.density.length));
		const cellMin = n > 0 ? (w.endMs - w.startMs) / n / 60_000 : 0;
		const bridge = cellMin > 0 ? Math.floor(stream.bridgeMin / cellMin) : 0;
		const cells: [number, number][] = [];
		for (let i = 0; i < n; i++) {
			if (!members.some((l) => (l.density[i] ?? 0) > 0)) continue;
			const last = cells[cells.length - 1];
			if (last && i - last[1] <= bridge) last[1] = i + 1;
			else cells.push([i, i + 1]);
		}
		out.push({ label: stream.label, runs: cells.map(([a, b]) => [a / n, b / n]) });
	}
	return out;
}
