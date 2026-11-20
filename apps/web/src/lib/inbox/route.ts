/**
 * Today as a route, as plain functions so it can be tested without a page.
 *
 * The solid line is where you have been: the day's recorded path and the
 * places you stopped. The dotted line is the rest of the plan: calendar
 * entries still ahead, where your server can tell where they are. A calendar
 * entry carries a place's name and no coordinates, so one is put on the map
 * only when its name matches a place you stopped at today. An entry that has
 * passed is never drawn as somewhere you were.
 *
 * Under the route sits the day's clock, which shows the same day by time:
 * the stops so far, the entries ahead, and each thing at its own time.
 */
import type { TimelineDay } from '$lib/timeline/day';
import { cleanTrack, dropSpikes, flagHoles, splitTrack, toFixes } from '$lib/timeline/track';
import { placeLabel, realPlaceName } from '$lib/wiki/placeName';
import type { InboxItem } from './items';

export const MAP_W = 860;
export const MAP_H = 480;
/** The room kept clear around the day's places: at the sides for a stop's name, and above for the caption. */
export const FRAME = { x: 150, top: 96, bottom: 48 } as const;
/** The closest the map zooms: about three metres to a pixel, so a day spent in one building is a dot. */
const MAX_PX_PER_DEGREE = 111_320 / 3;

export type Pt = [number, number];
/** Where a place on the ground falls in the drawing. */
export type Project = (lat: number, lng: number) => Pt;

/** The ground the day covers so far. */
export interface GeoBox {
	lat0: number;
	lat1: number;
	lng0: number;
	lng1: number;
}

export interface Stop {
	key: string;
	name: string;
	x: number;
	y: number;
	from: number;
	to: number;
	/** Still ahead, from the calendar. */
	planned: boolean;
}

export interface Route {
	stops: Stop[];
	/** The recorded path, as the lines to draw solid. */
	runs: Pt[][];
	/** Stretches nothing recorded, between two runs. */
	bridges: Pt[][];
	/** From where you are through the planned stops, in order, as an SVG path. Empty when nothing ahead can be placed. */
	ahead: string;
	you: { x: number; y: number } | null;
	/** Where a moment falls on the route, or null when the route does not reach it. */
	at: (ms: number) => { x: number; y: number } | null;
}

const NO_ROUTE: Route = { stops: [], runs: [], bridges: [], ahead: '', you: null, at: () => null };
const norm = (s: string) => s.trim().toLowerCase();
const lerp = (a: Pt, b: Pt, f: number) => ({ x: a[0] + (b[0] - a[0]) * f, y: a[1] + (b[1] - a[1]) * f });

/** The point at `ms` on a path of timed points, held at its ends. */
function along(path: { t: number; p: Pt }[], ms: number): { x: number; y: number } {
	if (ms <= path[0].t) return { x: path[0].p[0], y: path[0].p[1] };
	for (let i = 1; i < path.length; i++) {
		if (ms <= path[i].t) {
			const span = path[i].t - path[i - 1].t;
			return lerp(path[i - 1].p, path[i].p, span > 0 ? (ms - path[i - 1].t) / span : 1);
		}
	}
	const last = path[path.length - 1].p;
	return { x: last[0], y: last[1] };
}

/**
 * A planned leg bows to one side, so it stays visible where it goes back the
 * way you came. This is the curve's control point.
 */
function bow(a: Pt, b: Pt): Pt {
	const dx = b[0] - a[0];
	const dy = b[1] - a[1];
	const len = Math.hypot(dx, dy);
	if (len === 0) return a;
	const off = Math.min(48, len * 0.2);
	return [(a[0] + b[0]) / 2 - (dy / len) * off, (a[1] + b[1]) / 2 + (dx / len) * off];
}

/** The point at `ms` on the planned path, along its bowed legs. */
function alongBowed(path: { t: number; p: Pt }[], ms: number): { x: number; y: number } {
	for (let i = 1; i < path.length; i++) {
		if (ms > path[i].t) continue;
		const a = path[i - 1];
		const b = path[i];
		const f = b.t > a.t ? (ms - a.t) / (b.t - a.t) : 1;
		const c = bow(a.p, b.p);
		const g = 1 - f;
		return { x: g * g * a.p[0] + 2 * g * f * c[0] + f * f * b.p[0], y: g * g * a.p[1] + 2 * g * f * c[1] + f * f * b.p[1] };
	}
	const last = path[path.length - 1].p;
	return { x: last[0], y: last[1] };
}

/** The stops and the cleaned path up to now. */
function soFar(day: TimelineDay, nowMs: number) {
	const stays = day.stays
		.map((s) => ({ ...s, from: Date.parse(s.started_at), to: Date.parse(s.ended_at) }))
		.filter((s) => s.from <= nowMs)
		.sort((a, b) => a.from - b.from);
	const fixes = cleanTrack(flagHoles(dropSpikes(toFixes(day.points)))).filter((f) => f.t <= nowMs);
	return { stays, fixes };
}

function boxOf(geo: { lat: number; lng: number }[]): GeoBox | null {
	if (geo.length === 0) return null;
	const lats = geo.map((g) => g.lat);
	const lngs = geo.map((g) => g.lng);
	return { lat0: Math.min(...lats), lat1: Math.max(...lats), lng0: Math.min(...lngs), lng1: Math.max(...lngs) };
}

/** The ground the day covers so far, or null on a day with no location. */
export function geoBox(day: TimelineDay, nowMs: number): GeoBox | null {
	const { stays, fixes } = soFar(day, nowMs);
	return boxOf([...stays.map((s) => ({ lat: s.latitude, lng: s.longitude })), ...fixes]);
}

/**
 * A plain fit of the ground to the drawing, for when no map is under it.
 * North is up and distances keep their proportions: longitude is narrowed by
 * the latitude, then everything is fitted inside the frame.
 */
function plainFit({ lat0, lat1, lng0, lng1 }: GeoBox): Project {
	const narrow = Math.cos((((lat0 + lat1) / 2) * Math.PI) / 180);
	const wide = (lng1 - lng0) * narrow;
	const tall = lat1 - lat0;
	const fit = Math.min(
		wide > 0 ? (MAP_W - 2 * FRAME.x) / wide : Infinity,
		tall > 0 ? (MAP_H - FRAME.top - FRAME.bottom) / tall : Infinity,
		MAX_PX_PER_DEGREE,
	);
	const cx = MAP_W / 2;
	const cy = FRAME.top + (MAP_H - FRAME.top - FRAME.bottom) / 2;
	return (lat, lng) => [cx + (lng - (lng0 + lng1) / 2) * narrow * fit, cy - (lat - (lat0 + lat1) / 2) * fit];
}

/**
 * The day's route. `onMap` is how the map under the drawing places a point,
 * so the route lies on the map's own ground. Without one the route gets a
 * plain fit.
 */
export function buildRoute(day: TimelineDay, nowMs: number, onMap?: Project): Route {
	const { stays, fixes } = soFar(day, nowMs);
	const box = boxOf([...stays.map((s) => ({ lat: s.latitude, lng: s.longitude })), ...fixes]);
	if (!box) return NO_ROUTE;
	const project = onMap ?? plainFit(box);

	// One stop per place, at the time you first got there.
	const stops: Stop[] = [];
	for (const s of stays) {
		const [x, y] = project(s.latitude, s.longitude);
		const same = stops.find((q) => Math.hypot(q.x - x, q.y - y) < 6);
		if (same) {
			same.to = Math.max(same.to, s.to);
			continue;
		}
		const title = s.place_id ? day.place_titles[s.place_id] : undefined;
		const name = title && title.source !== 'none' ? title.title : placeLabel(s.place_name);
		stops.push({ key: `stay:${s.id}`, name, x, y, from: s.from, to: s.to, planned: false });
	}

	const track = fixes.map((f) => ({ t: f.t, p: project(f.lat, f.lng) }));
	const stayPath = stays.flatMap((s) => {
		const p = project(s.latitude, s.longitude);
		return [
			{ t: s.from, p },
			{ t: Math.min(s.to, nowMs), p },
		];
	});
	const been = track.length > 0 ? track : stayPath;
	const split = splitTrack(fixes);
	const line = (l: [number, number][]) => l.map(([lng, lat]) => project(lat, lng));
	const runs = track.length > 0 ? split.runs.map(line) : stops.length > 1 ? [stayPath.map((s) => s.p)] : [];
	const bridges = track.length > 0 ? split.bridges.map(line) : [];
	const here = been[been.length - 1].p;
	const you = { x: here[0], y: here[1] };

	// The plan: entries still ahead whose place is one of today's stops.
	const planned: Stop[] = [];
	for (const e of [...day.calendar].sort((a, b) => a.started_at.localeCompare(b.started_at))) {
		const from = Date.parse(e.started_at);
		if (e.is_all_day || from <= nowMs || e.status === 'cancelled' || e.response_status === 'declined') continue;
		const place = e.location_name ? norm(e.location_name) : '';
		const where = place ? stops.find((s) => realPlaceName(s.name) && place.includes(norm(s.name))) : undefined;
		if (where) planned.push({ key: `plan:${e.id}`, name: e.title, x: where.x, y: where.y, from, to: Date.parse(e.ended_at), planned: true });
	}
	const plan = [{ t: nowMs, p: here }, ...planned.flatMap((s) => [{ t: s.from, p: [s.x, s.y] as Pt }, { t: s.to, p: [s.x, s.y] as Pt }])];
	const n = (v: number) => v.toFixed(1);
	let ahead = '';
	let from = here;
	for (const s of planned) {
		const to: Pt = [s.x, s.y];
		const c = bow(from, to);
		if (from[0] !== to[0] || from[1] !== to[1]) ahead += `M${n(from[0])} ${n(from[1])} Q${n(c[0])} ${n(c[1])} ${n(to[0])} ${n(to[1])} `;
		from = to;
	}

	return {
		stops: [...stops, ...planned],
		runs,
		bridges,
		ahead: ahead.trim(),
		you,
		at: (ms) => {
			if (ms <= nowMs) return along(been, ms);
			if (planned.length === 0 || ms > plan[plan.length - 1].t) return null;
			return alongBowed(plan, ms);
		},
	};
}

// ── the day's clock ─────────────────────────────────────────────────────

export interface DayBounds {
	startMs: number;
	endMs: number;
}

export const dayBounds = (day: Pick<TimelineDay, 'started_at' | 'ended_at'>): DayBounds => ({
	startMs: Date.parse(day.started_at),
	endMs: Date.parse(day.ended_at),
});

export interface Bar {
	key: string;
	label: string;
	from: number;
	to: number;
	planned: boolean;
}

/** The clock's bars: where you stopped so far, then the calendar entries not yet over. */
export function dayBars(day: TimelineDay, nowMs: number): Bar[] {
	const { startMs, endMs } = dayBounds(day);
	const bars: Bar[] = [];
	for (const s of day.stays) {
		const from = Math.max(Date.parse(s.started_at), startMs);
		const to = Math.min(Date.parse(s.ended_at), nowMs, endMs);
		if (to <= from) continue;
		const title = s.place_id ? day.place_titles[s.place_id] : undefined;
		bars.push({ key: `stay:${s.id}`, label: title && title.source !== 'none' ? title.title : placeLabel(s.place_name), from, to, planned: false });
	}
	for (const e of day.calendar) {
		const from = Math.max(Date.parse(e.started_at), startMs);
		const to = Math.min(Date.parse(e.ended_at), endMs);
		if (e.is_all_day || to <= nowMs || to <= from || e.status === 'cancelled' || e.response_status === 'declined') continue;
		bars.push({ key: `plan:${e.id}`, label: e.title, from, to, planned: true });
	}
	return bars.sort((a, b) => a.from - b.from);
}

/**
 * When a thing falls today: a task at the time it is due, an insight at the
 * time it stops mattering. Null when it has no time of its own today, such
 * as a task due today at no set time, or something your server needs.
 */
export function itemTime(it: Pick<InboxItem, 'task' | 'note'>, day: DayBounds): number | null {
	const today = (iso: string | null | undefined) => {
		const t = iso ? Date.parse(iso) : NaN;
		return t >= day.startMs && t < day.endMs ? t : null;
	};
	return today(it.task?.due_at) ?? today(it.note?.until);
}

export const CLOCK_X0 = 24;
export const CLOCK_X1 = MAP_W - 24;

/**
 * The clock's scale. It starts at 6 in the morning, or earlier when
 * something that day is earlier, and ends at midnight.
 */
export function clockScale(day: DayBounds, times: number[]): { fromHour: number; x: (ms: number) => number } {
	const hour = (ms: number) => (ms - day.startMs) / 3_600_000;
	const fromHour = Math.max(0, Math.min(6, ...times.map((t) => Math.floor(hour(t)))));
	return {
		fromHour,
		x: (ms) => CLOCK_X0 + ((Math.min(24, Math.max(fromHour, hour(ms))) - fromHour) / (24 - fromHour)) * (CLOCK_X1 - CLOCK_X0),
	};
}

/** Marks along a line, moved down a row when the one before is too close. */
export function stack(marks: { key: string; x: number }[], gap = 22): Record<string, { x: number; row: number }> {
	const out: Record<string, { x: number; row: number }> = {};
	const ends: number[] = [];
	for (const m of [...marks].sort((a, b) => a.x - b.x)) {
		let row = ends.findIndex((end) => m.x - end >= gap);
		if (row < 0) row = ends.length;
		ends[row] = m.x;
		out[m.key] = { x: m.x, row };
	}
	return out;
}
