/**
 * dayRoute.ts - the route figure on a day page: where the phone was, as one
 * map of the day's city and, when the day crossed a long way, a second map of
 * the long move itself.
 *
 * The fixes are the Timeline's (`lib/timeline/track.ts`): GPS only, spikes
 * dropped. A line into a fix that arrived more than ten minutes after the one
 * before, and far enough away to see, is dashed: the phone went quiet, and
 * nothing recorded the way it went.
 */
import type { TimelineDayLocationChunk, TimelineDayView } from "$lib/wiki/api";
import { cleanTrack, dropSpikes, HOLE_MIN_M, metres, toFixes, type Fix } from "$lib/timeline/track";

/** Fixes this far apart are a day that crossed a long way: a second map draws the move. */
export const LONG_MOVE_M = 200_000;
/** A silence longer than this, ending somewhere else, is drawn dashed. */
export const QUIET_MS = 10 * 60_000;
/** Fixes and stops this close to where the day mostly was are its city. */
const CITY_M = 50_000;

/** A point as `MovementMap` draws it. */
export interface RoutePoint {
	lat: number;
	lng: number;
	timeMs?: number;
	label?: string;
	bridge?: boolean;
}

export interface RoutePanel {
	track: RoutePoint[];
	stops: RoutePoint[];
}

export interface DayRoute {
	/** The day's city: its track and its stops. Null when it holds under two
	 *  fixes and no stop, which a map can only draw as its default view. */
	city: RoutePanel | null;
	/** The long move, the phone's own fixes, when the day's fixes lie LONG_MOVE_M apart. */
	long: RoutePanel | null;
	/** Whether either map has a dashed stretch. */
	quiet: boolean;
}

/** "10:37 AM" in the day's zone (the viewer's when the day has none). */
export function clock(ms: number, timeZone: string | null): string {
	const opts: Intl.DateTimeFormatOptions = { hour: "numeric", minute: "2-digit" };
	try {
		return new Intl.DateTimeFormat("en-US", { ...opts, timeZone: timeZone ?? undefined }).format(ms);
	} catch {
		// An unknown zone name: the viewer's clock rather than no time.
		return new Intl.DateTimeFormat("en-US", opts).format(ms);
	}
}

/** A place's name when it has a real one: not the wiki's "Location 41.9, -87.6" stub, not "Unknown". */
export function realPlaceName(name: string | null | undefined): string | null {
	const n = (name ?? "").trim();
	if (!n || /^unknown$/i.test(n) || /^location\s+-?\d/i.test(n)) return null;
	return n;
}

function isStop(c: TimelineDayView["chunks"][number]): c is TimelineDayLocationChunk {
	return c?.type === "location" && Number.isFinite((c as TimelineDayLocationChunk).latitude) && Number.isFinite((c as TimelineDayLocationChunk).longitude);
}

function stopPoint(s: TimelineDayLocationChunk, timeZone: string | null): RoutePoint {
	const at = Date.parse(s.start_time);
	const name = realPlaceName(s.place_name) ?? "Unnamed place";
	return { lat: s.latitude, lng: s.longitude, label: Number.isFinite(at) ? `${name} · ${clock(at, timeZone)}` : name };
}

/** The fixes as map points, each flagged when it arrived across a silence. */
export function quietTrack(fixes: Fix[]): RoutePoint[] {
	return fixes.map((f, i) => {
		const prev = fixes[i - 1];
		const bridge = !!prev && f.t - prev.t > QUIET_MS && metres(prev, f) >= HOLE_MIN_M;
		return { lat: f.lat, lng: f.lng, timeMs: f.t, bridge };
	});
}

/** How far apart the fixes lie: the farthest from the first, then the farthest from that. */
export function spanMetres(fixes: { lat: number; lng: number }[]): number {
	if (fixes.length < 2) return 0;
	const farthest = (from: { lat: number; lng: number }) =>
		fixes.reduce((best, f) => (metres(from, f) > metres(from, best) ? f : best), fixes[0]);
	const a = farthest(fixes[0]);
	return metres(a, farthest(a));
}

/** Where the day mostly was: the stop with the most time spent at stops near
 *  it, else the fix with the most fixes near it. */
function centre(stops: TimelineDayLocationChunk[], fixes: Fix[]): { lat: number; lng: number } {
	const dwell = (s: TimelineDayLocationChunk) => Math.max(0, Date.parse(s.end_time) - Date.parse(s.start_time)) || 0;
	const at = (s: TimelineDayLocationChunk) => ({ lat: s.latitude, lng: s.longitude });
	if (stops.length) {
		let best = stops[0];
		let most = -1;
		for (const s of stops) {
			const near = stops.filter((o) => metres(at(s), at(o)) <= CITY_M).reduce((sum, o) => sum + dwell(o), 0);
			if (near > most) [best, most] = [s, near];
		}
		return at(best);
	}
	// Every tenth fix is a candidate: plenty to find a city, a tenth of the work.
	let best = fixes[0];
	let most = -1;
	for (let i = 0; i < fixes.length; i += 10) {
		const near = fixes.filter((f) => metres(fixes[i], f) <= CITY_M).length;
		if (near > most) [best, most] = [fixes[i], near];
	}
	return best;
}

/** A city map, when it has something to frame: two fixes, or a stop. */
function cityPanel(track: RoutePoint[], stops: RoutePoint[]): RoutePanel | null {
	return track.length < 2 && !stops.length ? null : { track, stops };
}

/** The route figure's maps for a day, or null when the day has no location. */
export function dayRoute(view: TimelineDayView | null, timeZone: string | null): DayRoute | null {
	if (!view) return null;
	const fixes = dropSpikes(toFixes(view.points ?? []));
	const stops = (view.chunks ?? []).filter(isStop);
	if (fixes.length < 2 && !stops.length) return null;

	if (spanMetres(fixes) < LONG_MOVE_M) {
		const track = quietTrack(cleanTrack(fixes));
		const city = cityPanel(track, stops.map((s) => stopPoint(s, timeZone)));
		if (!city) return null;
		return { city, long: null, quiet: track.some((p) => p.bridge) };
	}

	const home = centre(stops, fixes);
	const near = (p: { lat: number; lng: number }) => metres(home, p) <= CITY_M;
	const city = quietTrack(cleanTrack(fixes.filter(near)));
	// The long move: from the last fix in the city before it left to the
	// first back, or to the day's end.
	const first = fixes.findIndex((f) => !near(f));
	let last = first;
	for (let i = fixes.length - 1; i > first; i--) {
		if (!near(fixes[i])) {
			last = i;
			break;
		}
	}
	const move = fixes.slice(Math.max(0, first - 1), Math.min(fixes.length, last + 2));
	const long = quietTrack(move);
	const ends = [move[0], move[move.length - 1]].map((f) => ({ lat: f.lat, lng: f.lng, label: clock(f.t, timeZone) }));
	return {
		city: cityPanel(city, stops.filter((s) => near({ lat: s.latitude, lng: s.longitude })).map((s) => stopPoint(s, timeZone))),
		long: { track: long, stops: ends },
		quiet: city.some((p) => p.bridge) || long.some((p) => p.bridge),
	};
}
