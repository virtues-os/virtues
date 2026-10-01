/**
 * track.ts - the day's GPS path, as the Timeline draws it.
 *
 * Ported from the Dayback prototype: the hole flag and clean_track in its
 * pipeline (dayback/build.py), and trailSplit in its viewer (dayback/src/main.js).
 * The path is two drawings of one track: solid runs of recorded fixes, and
 * straight bridges across the holes in the recording.
 *
 * A bridge is ours, not the phone's. The phone went quiet for two minutes or
 * more and came back 300 m or more away: it moved, but nothing recorded how,
 * and a solid line across the hop would claim a route nobody saw. People turn
 * location off, phones sleep and tunnels swallow signal, so holes are common,
 * and the map says so rather than papering over them.
 */
import type { TimelineDayPoint } from "$lib/wiki/api";

/** A fix this far from both its time-neighbours, while they sit together, is a
 *  spike: the phone jumped out and back, a jump no human speed explains. */
export const SPIKE_M = 300;
/** A silence at least this long... */
export const HOLE_MIN_MS = 2 * 60_000;
/** ...that ends at least this far away is a hole in the recording. */
export const HOLE_MIN_M = 300;
/** Fixes this close to a spot's first fix are GPS jitter at that spot. */
export const SPOT_R_M = 30;
/** A spot held longer than this keeps its leave time as a second point. */
const SPOT_LEAVE_MS = 60_000;
/** A fix the phone itself rates at or worse than this is a cell tower's
 *  guess, not GPS (dayback/build.py:290). */
export const GPS_ACCURACY_MAX_M = 100;

/** One fix. `bridge`: it arrived across a hole. */
export type Fix = { t: number; lat: number; lng: number; bridge: boolean };
export type Line = [number, number][];

/** Great-circle distance in metres. */
export function metres(a: { lat: number; lng: number }, b: { lat: number; lng: number }): number {
	const R = 6_371_000;
	const rad = Math.PI / 180;
	const h =
		Math.sin(((b.lat - a.lat) * rad) / 2) ** 2 +
		Math.cos(a.lat * rad) * Math.cos(b.lat * rad) * Math.sin(((b.lng - a.lng) * rad) / 2) ** 2;
	return 2 * R * Math.asin(Math.min(1, Math.sqrt(h)));
}

/**
 * The endpoint's GPS fixes, in time order. A fix the phone rates worse than
 * 100 m is dropped: the prototype's rule for movement (dayback/build.py:290),
 * here for the drawn line too. A still phone indoors reports such guesses
 * hundreds of metres to kilometres off and snaps back within a minute, and
 * each drew a straight line out and back (one sparse day: 51 of 665). The
 * prototype's July track rarely had them, so its line never needed the rule.
 */
export function toFixes(points: TimelineDayPoint[]): Fix[] {
	return points
		.filter((p) => p.horizontal_accuracy === null || p.horizontal_accuracy < GPS_ACCURACY_MAX_M)
		.map((p) => ({ t: Date.parse(p.timestamp), lat: p.latitude, lng: p.longitude, bridge: false }))
		.filter((f) => Number.isFinite(f.t))
		.sort((a, b) => a.t - b.t);
}

/**
 * The fixes with isolated spikes dropped - the prototype's rule
 * (dayback/resolve.py SPIKE_M; LOCATION-RESOLUTION.md): a fix far from BOTH
 * its time-neighbours is a bad fix, found by neighbour consistency and never
 * by iOS's accuracy flag, which calls some 1.7 km misses accurate. One guard
 * for a moving track: the two neighbours must sit together. On a drive the
 * fixes run hundreds of metres apart and the bare rule would erase the drive;
 * the prototype never met that, since the rule only fed stay detection.
 */
export function dropSpikes(fixes: Fix[]): Fix[] {
	return fixes.filter((f, i) => {
		const prev = fixes[i - 1];
		const next = fixes[i + 1];
		if (!prev || !next) return true;
		return !(metres(prev, f) > SPIKE_M && metres(f, next) > SPIKE_M && metres(prev, next) < SPIKE_M);
	});
}

/**
 * The fixes, each flagged when it arrived across a hole. Pass the whole fetched
 * window, not just the day, with its spikes already dropped: the day's first fix
 * is judged against the last fix before it, and a spike is no evidence of a hop.
 */
export function flagHoles(fixes: Fix[]): Fix[] {
	return fixes.map((q, i) => {
		const p = fixes[i - 1];
		return { ...q, bridge: !!p && q.t - p.t >= HOLE_MIN_MS && metres(p, q) >= HOLE_MIN_M };
	});
}

/**
 * Stationary jitter collapsed to one point per spot: fixes within SPOT_R_M of
 * a spot's first fix are that spot, drawn at their centre (with a second point
 * at the leave time when it was held over a minute), so a morning at home is a
 * dot rather than a scribble while the path between spots stays intact. A hole
 * flag stays with the spot it arrived at.
 */
export function cleanTrack(fixes: Fix[]): Fix[] {
	const out: Fix[] = [];
	let spot: Fix[] = [];
	const flush = () => {
		if (!spot.length) return;
		const lat = spot.reduce((s, f) => s + f.lat, 0) / spot.length;
		const lng = spot.reduce((s, f) => s + f.lng, 0) / spot.length;
		const first = spot[0];
		const last = spot[spot.length - 1];
		out.push({ t: first.t, lat, lng, bridge: first.bridge });
		if (spot.length > 1 && last.t - first.t > SPOT_LEAVE_MS) out.push({ t: last.t, lat, lng, bridge: false });
	};
	for (const f of fixes) {
		if (spot.length && metres(spot[0], f) <= SPOT_R_M) spot.push(f);
		else {
			flush();
			spot = [f];
		}
	}
	flush();
	return out;
}

/** The track as its two drawings: solid runs, and the bridges between them. */
export function splitTrack(track: Fix[]): { runs: Line[]; bridges: Line[] } {
	const runs: Line[] = [];
	const bridges: Line[] = [];
	let run: Line = [];
	track.forEach((f, i) => {
		const here: [number, number] = [f.lng, f.lat];
		if (f.bridge && i > 0) {
			const prev = track[i - 1];
			bridges.push([[prev.lng, prev.lat], here]);
			if (run.length >= 2) runs.push(run);
			run = [here];
		} else run.push(here);
	});
	if (run.length >= 2) runs.push(run);
	return { runs, bridges };
}
