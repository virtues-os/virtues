import { describe, expect, it } from "vitest";
import { anchorAt, isMoving, trackMetres } from "./anchor";
import type { Fix } from "./track";

const MIN = 60_000;
const fix = (minute: number, lat: number, lng = -97): Fix => ({ t: minute * MIN, lat, lng, bridge: false });

describe("a moment's anchor", () => {
	it("interpolates between close fixes at the midpoint in time", () => {
		// 111 m apart: close enough to join.
		const a = anchorAt([fix(0, 30.0), fix(10, 30.001)], 0, 10 * MIN);
		expect(a!.lat).toBeCloseTo(30.0005, 6);
	});

	it("snaps to the nearer real fix across a hole, never cutting the corner", () => {
		// 2.2 km apart: a hole or highway speed.
		const fixes = [fix(0, 30.0), fix(10, 30.02)];
		expect(anchorAt(fixes, 0, 8 * MIN)).toEqual({ lat: 30.0, lng: -97 });
		expect(anchorAt(fixes, 4 * MIN, 16 * MIN)).toEqual({ lat: 30.02, lng: -97 });
	});

	it("uses the only fix there is, and nothing when there is none", () => {
		expect(anchorAt([fix(0, 30.0)], 10 * MIN, 20 * MIN)).toEqual({ lat: 30.0, lng: -97 });
		expect(anchorAt([], 0, MIN)).toBeNull();
	});

	it("calls a moment moving over half a kilometre of track", () => {
		const walk = [fix(0, 30.0), fix(5, 30.003), fix(10, 30.006)]; // ~667 m
		expect(trackMetres(walk, 0, 10 * MIN)).toBeGreaterThan(600);
		expect(isMoving(walk, 0, 10 * MIN)).toBe(true);
		expect(isMoving(walk, 0, 5 * MIN)).toBe(false);
	});
});
