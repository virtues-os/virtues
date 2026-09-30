/**
 * scale.ts - the scrubber's time axis (dayback/src/main.js:74-153, 209-232):
 * the span in view and its tier, the nights folded to thin bands so the
 * waking hours get the width, and the ruler's ticks.
 */

export const MIN = 60_000;
export const HOUR = 60 * MIN;
export const DAY = 24 * HOUR;
/** Each folded night takes this much of the axis: a sliver. */
export const FOLD_W = 40 * MIN;
/** The narrowest span a pinch can reach. */
export const MIN_SPAN = 3 * MIN;

/** A stretch of time drawn as a thin band; `est` when no night was detected
 *  and it stands in for one. */
export interface Fold {
	a: number;
	b: number;
	est: boolean;
}

export type Tier = "week" | "day" | "hour" | "min";

/** The folds: every detected night, plus 01:00-06:00 on a date none of them
 *  touches before 09:00, merged where they overlap (main.js:100-111).
 *  `midnights` are the local midnights of the dates in view. */
export function buildFolds(nights: { s: number; e: number }[], midnights: number[]): Fold[] {
	const all: Fold[] = nights.map((n) => ({ a: n.s, b: n.e, est: false }));
	for (const md of midnights) {
		if (!nights.some((n) => n.s < md + 9 * HOUR && n.e > md)) all.push({ a: md + HOUR, b: md + 6 * HOUR, est: true });
	}
	all.sort((p, q) => p.a - q.a);
	const merged: Fold[] = [];
	for (const f of all) {
		const last = merged.at(-1);
		if (last && f.a <= last.b) {
			last.b = Math.max(last.b, f.b);
			if (!f.est) last.est = false;
		} else merged.push({ ...f });
	}
	return merged;
}

/** Real time to axis time: each fold compresses to FOLD_W. */
export function warp(folds: Fold[], t: number): number {
	let off = 0;
	for (const f of folds) {
		if (t <= f.a) return t - off;
		if (t <= f.b) return f.a - off + ((t - f.a) / (f.b - f.a)) * FOLD_W;
		off += f.b - f.a - FOLD_W;
	}
	return t - off;
}

/** Axis time back to real time. */
export function unwarp(folds: Fold[], u: number): number {
	let off = 0;
	for (const f of folds) {
		const wa = f.a - off;
		const dur = f.b - f.a;
		if (u <= wa) return u + off;
		if (u <= wa + FOLD_W) return f.a + ((u - wa) / FOLD_W) * dur;
		off += dur - FOLD_W;
	}
	return u + off;
}

export const inFold = (folds: Fold[], t: number) => folds.some((f) => t > f.a && t < f.b);

/** The tier a span reads as (main.js:81). */
export function tierOf(span: number): Tier {
	if (span <= 20 * MIN) return "min";
	if (span <= 5 * HOUR) return "hour";
	if (span <= 40 * HOUR) return "day";
	return "week";
}

/** A tier's span (main.js:82-88): the week around the day, the day, three
 *  hours or fourteen minutes around the playhead. */
export function tierView(t: Tier, day: { s: number; e: number }, playT: number): [number, number] {
	if (t === "week") return [day.s - 3 * DAY, day.s + 4 * DAY];
	if (t === "day") return [day.s, day.e];
	const half = t === "hour" ? 1.5 * HOUR : 7 * MIN;
	return [playT - half, playT + half];
}

/** A view kept at least MIN_SPAN wide and inside [lo, hi] (main.js:90-98). */
export function clampView(a: number, b: number, lo: number, hi: number): [number, number] {
	let span = b - a;
	if (span < MIN_SPAN) {
		const c = (a + b) / 2;
		a = c - MIN_SPAN / 2;
		b = c + MIN_SPAN / 2;
		span = MIN_SPAN;
	}
	if (span >= hi - lo) {
		const c = (lo + hi) / 2;
		return [c - span / 2, c + span / 2];
	}
	if (a < lo) return [lo, lo + span];
	if (b > hi) return [hi - span, hi];
	return [a, b];
}

const DOW = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/** The ruler's labels (main.js:209-232): calendar-aligned steps for the
 *  span, in local time `offset` ms east of UTC, thinned to one per ~86 px. */
export function ticks(viewStart: number, viewEnd: number, width: number, offset: number): [number, string][] {
	const span = viewEnd - viewStart;
	const max = Math.max(2, Math.floor(width / 86));
	const out: [number, string][] = [];
	const at = (t: number) => new Date(t + offset);
	const h12 = (d: Date) => ((d.getUTCHours() + 11) % 12) + 1;
	const push = (t: number, label: string) => {
		if (t >= viewStart && t <= viewEnd) out.push([t, label]);
	};
	if (span <= 6 * HOUR) {
		const st = (span <= HOUR ? 10 : span <= 3 * HOUR ? 15 : 30) * MIN;
		for (let t = Math.ceil((viewStart + offset) / st) * st - offset; t <= viewEnd; t += st) {
			const d = at(t);
			push(t, `${h12(d)}:${String(d.getUTCMinutes()).padStart(2, "0")}`);
		}
	} else if (span <= 3 * DAY) {
		const st = (span <= 12 * HOUR ? 1 : span <= 36 * HOUR ? 3 : 6) * HOUR;
		for (let t = Math.ceil((viewStart + offset) / st) * st - offset; t <= viewEnd; t += st) {
			const d = at(t);
			push(t, `${h12(d)}${d.getUTCHours() < 12 ? "AM" : "PM"}`);
		}
	} else {
		for (let t = Math.floor((viewStart + offset) / DAY) * DAY - offset; t <= viewEnd; t += DAY) {
			const d = at(t);
			push(t, `${DOW[d.getUTCDay()]} ${d.getUTCDate()}`);
		}
	}
	if (out.length <= max) return out;
	const every = Math.ceil(out.length / max);
	return out.filter((_, i) => i % every === 0);
}

/** A zone's offset east of UTC at an instant, in ms. */
export function zoneOffset(t: number, zone: string): number {
	const parts = new Intl.DateTimeFormat("en-US", {
		timeZone: zone,
		hourCycle: "h23",
		year: "numeric",
		month: "2-digit",
		day: "2-digit",
		hour: "2-digit",
		minute: "2-digit",
		second: "2-digit",
	}).formatToParts(new Date(t));
	const n = (type: string) => Number(parts.find((p) => p.type === type)?.value);
	const asUtc = Date.UTC(n("year"), n("month") - 1, n("day"), n("hour") % 24, n("minute"), n("second"));
	return asUtc - Math.floor(t / 1000) * 1000;
}

/** A YYYY-MM-DD date's local midnight in `zone`, at the date's noon offset
 *  (the prototype's rule, dayback/build.py:261). */
export function midnightIn(slug: string, zone: string): number {
	const [y, m, d] = slug.split("-").map(Number);
	return Date.UTC(y, m - 1, d) - zoneOffset(Date.UTC(y, m - 1, d, 12), zone);
}
