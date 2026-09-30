/**
 * anchor.ts - where a moment sits on the map: where the track says you were at
 * its midpoint in time, never an average of positions, which lands off the
 * road (dayback/src/main.js:1022-1035; ledger 4.7, 10.4).
 */
import { metres, type Fix } from "./track";

/** Two bracketing fixes closer than this are joined by a straight line; farther
 *  apart (a GPS hole, highway speed), the nearer real fix stands in. */
const INTERPOLATE_M = 200;
/** A moment whose track covers more than this is on the move. */
const MOVING_M = 500;

/** The position at the midpoint of [s, e), from fixes in time order; null
 *  when there are none. */
export function anchorAt(fixes: Fix[], s: number, e: number): { lat: number; lng: number } | null {
	const mid = (s + e) / 2;
	let before: Fix | undefined;
	let after: Fix | undefined;
	for (const f of fixes) {
		if (f.t <= mid) {
			if (!before || f.t > before.t) before = f;
		} else if (!after || f.t < after.t) {
			after = f;
		}
	}
	if (before && after) {
		if (metres(before, after) <= INTERPOLATE_M) {
			const u = (mid - before.t) / (after.t - before.t || 1);
			return { lat: before.lat + (after.lat - before.lat) * u, lng: before.lng + (after.lng - before.lng) * u };
		}
		const nearer = mid - before.t <= after.t - mid ? before : after;
		return { lat: nearer.lat, lng: nearer.lng };
	}
	const only = before ?? after;
	return only ? { lat: only.lat, lng: only.lng } : null;
}

/** The track length over [s, e], in metres (main.js:2529 `trackDist`). */
export function trackMetres(fixes: Fix[], s: number, e: number): number {
	const inside = fixes.filter((f) => f.t >= s && f.t <= e);
	let d = 0;
	for (let i = 1; i < inside.length; i++) d += metres(inside[i - 1], inside[i]);
	return d;
}

/** Whether a moment moved: over half a kilometre of track while it ran. */
export function isMoving(fixes: Fix[], s: number, e: number): boolean {
	return trackMetres(fixes, s, e) > MOVING_M;
}
