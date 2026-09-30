import { describe, expect, it } from "vitest";
import { clusterPoints, placeLabels, CLUSTER_PX } from "./bubbles";
import { positionAt } from "./anchor";

describe("clusterPoints", () => {
	it("folds dots within 48 px into one knot around their running centre", () => {
		const c = clusterPoints([
			{ x: 0, y: 0 },
			{ x: 30, y: 0 },
			{ x: 200, y: 0 },
		]);
		expect(c.map((k) => k.items)).toEqual([[0, 1], [2]]);
		expect(c[0].x).toBe(15);
	});
	it("keeps dots exactly 48 px apart separate", () => {
		expect(clusterPoints([{ x: 0, y: 0 }, { x: CLUSTER_PX, y: 0 }])).toHaveLength(2);
	});
});

describe("placeLabels", () => {
	const lim = { l: 0, t: 0, r: 1000, b: 1000 };
	it("puts a lone label above its dot", () => {
		const [[ox, oy]] = placeLabels([{ x: 500, y: 500, w: 100, h: 30 }], lim, []);
		expect([ox, oy]).toEqual([0, -(14 + 15)]);
	});
	it("moves a second label to a free slot rather than overlap the first", () => {
		const slots = placeLabels(
			[
				{ x: 500, y: 500, w: 100, h: 30 },
				{ x: 505, y: 500, w: 100, h: 30 },
			],
			lim,
			[],
		);
		expect(slots[0]).toEqual([0, -29]);
		expect(slots[1]).toEqual([0, 29]);
	});
	it("keeps a label inside the clear area", () => {
		const [[, oy]] = placeLabels([{ x: 500, y: 20, w: 100, h: 30 }], lim, []);
		expect(oy).toBeGreaterThan(0);
	});
});

describe("positionAt", () => {
	const fix = (t: number, lat: number) => ({ t, lat, lng: -97, bridge: false });
	it("slides along the track between two fixes and holds at its ends", () => {
		const track = [fix(0, 30), fix(10, 31)];
		expect(positionAt(track, 5)!.lat).toBeCloseTo(30.5, 9);
		expect(positionAt(track, -5)!.lat).toBe(30);
		expect(positionAt(track, 50)!.lat).toBe(31);
		expect(positionAt([], 5)).toBeNull();
	});
});
