import { describe, expect, it } from "vitest";
import type { TimelineDayPoint } from "$lib/wiki/api";
import { cleanTrack, dropSpikes, flagHoles, HOLE_MIN_M, HOLE_MIN_MS, metres, splitTrack, toFixes } from "./track";
import { stepDay } from "./day";

// A fixed spot (open ocean, so no real place) and a way to step north by metres.
const LAT = 0;
const LNG = -30;
const north = (m: number) => LAT + m / 111_195;
const T0 = Date.UTC(2026, 0, 1, 12);
const at = (minutes: number, metresNorth = 0): TimelineDayPoint => ({
	latitude: north(metresNorth),
	longitude: LNG,
	timestamp: new Date(T0 + minutes * 60_000).toISOString(),
	horizontal_accuracy: 5,
	speed: null,
});
/** The whole pipeline up to drawing: spikes out, then holes flagged. */
const track = (...pts: TimelineDayPoint[]) => flagHoles(dropSpikes(toFixes(pts)));

describe("metres", () => {
	it("measures a known north step", () => {
		expect(metres({ lat: north(0), lng: LNG }, { lat: north(1000), lng: LNG })).toBeCloseTo(1000, 0);
	});
});

describe("toFixes", () => {
	it("puts fixes in time order", () => {
		expect(toFixes([at(10), at(0)]).map((f) => f.t)).toEqual([T0, T0 + 10 * 60_000]);
	});
	it("drops a fix the phone rates at 100 m or worse, a cell tower's guess", () => {
		const guess = { ...at(1, 1400), horizontal_accuracy: 1414 };
		const edge = { ...at(2, 300), horizontal_accuracy: 100 };
		const unrated = { ...at(3), horizontal_accuracy: null };
		expect(toFixes([at(0), guess, edge, unrated, at(4)]).map((f) => f.t)).toEqual([T0, T0 + 3 * 60_000, T0 + 4 * 60_000]);
	});
	it("drops a still phone's fix rated worse than 20 m, a Wi-Fi guess, but not a moving one's", () => {
		const wifi = { ...at(1, 100), horizontal_accuracy: 45, speed: 0 };
		const sharp = { ...at(2), horizontal_accuracy: 20, speed: 0 };
		const driving = { ...at(3, 300), horizontal_accuracy: 45, speed: 12 };
		expect(toFixes([wifi, sharp, driving]).map((f) => f.t)).toEqual([T0 + 2 * 60_000, T0 + 3 * 60_000]);
	});
	it("keeps a fix with no speed at all to the 100 m bar: nothing says it was still", () => {
		const unspeeded = { ...at(1, 100), horizontal_accuracy: 45, speed: null };
		expect(toFixes([unspeeded])).toHaveLength(1);
	});
});

describe("dropSpikes", () => {
	it("drops a fix that jumps out and back", () => {
		expect(track(at(0, 0), at(1, 2000), at(2, 10)).map((f) => f.t)).toEqual([T0, T0 + 2 * 60_000]);
	});
	it("keeps a drive, whose fixes run far apart but keep going", () => {
		expect(track(at(0, 0), at(0.5, 500), at(1, 1000))).toHaveLength(3);
	});
	it("keeps the first and last fixes, which have one neighbour", () => {
		expect(track(at(0, 0), at(1, 5000))).toHaveLength(2);
	});
	it("leaves no bridge behind a dropped spike", () => {
		const fixes = track(at(0, 0), at(10, 2000), at(10.5, 5));
		expect(fixes.map((f) => f.bridge)).toEqual([false, false]);
	});
});

describe("flagHoles", () => {
	it("flags a fix that arrives after two minutes of silence, 300 m or more away", () => {
		expect(track(at(0), at(HOLE_MIN_MS / 60_000, HOLE_MIN_M + 5)).map((f) => f.bridge)).toEqual([false, true]);
	});
	it("does not flag a short silence, however far", () => {
		expect(track(at(0), at(1.9, 5000))[1].bridge).toBe(false);
	});
	it("flags a hop no one could travel, however short the silence", () => {
		// 8.9 km in 17 seconds: 1,885 km/h.
		expect(track(at(0), at(17 / 60, 8900)).map((f) => f.bridge)).toEqual([false, true]);
	});
	it("does not flag a fast but real hop, or a short one", () => {
		expect(track(at(0), at(1, 1000))[1].bridge).toBe(false); // 60 km/h
		expect(track(at(0), at(0.1, 200))[1].bridge).toBe(false); // too short to matter
	});
	it("does not flag a long silence that ends where it began", () => {
		expect(track(at(0), at(90, HOLE_MIN_M - 20))[1].bridge).toBe(false);
	});
});

describe("cleanTrack", () => {
	it("collapses jitter at one spot into its centre, keeping the leave time", () => {
		const clean = cleanTrack(track(at(0, 0), at(1, 10), at(2, 20), at(5, 0)));
		expect(clean).toHaveLength(2);
		expect(clean[0].t).toBe(T0);
		expect(clean[1].t).toBe(T0 + 5 * 60_000);
		expect(clean[0].lat).toBeCloseTo(north(7.5), 6);
	});
	it("keeps a move between spots", () => {
		expect(cleanTrack(track(at(0, 0), at(1, 500), at(2, 1000)))).toHaveLength(3);
	});
	it("keeps the hole flag on the spot the fix arrived at", () => {
		const clean = cleanTrack(track(at(0, 0), at(30, 2000), at(31, 2005)));
		expect(clean.map((f) => f.bridge)).toEqual([false, true]);
	});
});

describe("splitTrack", () => {
	it("draws a hole as a bridge between two runs", () => {
		const { runs, bridges } = splitTrack(track(at(0, 0), at(1, 100), at(30, 3000), at(31, 3100)));
		expect(runs).toHaveLength(2);
		expect(bridges).toHaveLength(1);
		expect(bridges[0][0]).toEqual(runs[0][runs[0].length - 1]);
		expect(bridges[0][1]).toEqual(runs[1][0]);
	});
	it("draws nothing for a single fix", () => {
		expect(splitTrack(track(at(0)))).toEqual({ runs: [], bridges: [] });
	});
});

describe("day", () => {
	it("steps across month and year ends", () => {
		expect(stepDay("2026-09-30", 1)).toBe("2026-10-01");
		expect(stepDay("2026-03-01", -1)).toBe("2026-02-28");
		expect(stepDay("2026-12-31", 1)).toBe("2027-01-01");
	});
});
