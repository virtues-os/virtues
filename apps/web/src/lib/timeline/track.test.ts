import { describe, expect, it } from "vitest";
import type { TimelineDayPoint } from "$lib/wiki/api";
import { cleanTrack, dropSpikes, flagHoles, HOLE_MIN_M, HOLE_MIN_MS, metres, splitTrack, toFixes } from "./track";
import { gapVerdict, lastMeasured, stepDay, utcDatesFor } from "./day";

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

describe("the empty day, in the prototype's words", () => {
	it("gives gapWhy's three verdicts", () => {
		expect(gapVerdict({ fixes: 0, talk: false, steps: false })).toBe("Nothing recorded");
		expect(gapVerdict({ fixes: 0, talk: true, steps: true })).toBe("Phone on, mic active");
		expect(gapVerdict({ fixes: 0, talk: false, steps: true })).toBe("Phone on but idle");
		expect(gapVerdict({ fixes: 3, talk: false, steps: false })).toBe("Phone on but idle");
	});
	it("says when location was last measured, as honestWhere does, on the day's clock", () => {
		const t = Date.parse("2026-09-23T23:00:00Z"); // 6:00 PM in Chicago (CDT)
		expect(lastMeasured(t, false, "America/Chicago")).toBe("last measured Sep 23 · 6:00 PM");
		expect(lastMeasured(t, true, "America/Chicago")).toBe("last measured at 6:00 PM");
		// The same instant on a Seattle day reads two hours earlier.
		expect(lastMeasured(t, true, "America/Los_Angeles")).toBe("last measured at 4:00 PM");
		expect(lastMeasured(Date.parse("2026-01-02T06:05:00Z"), false, "America/Chicago")).toBe("last measured Jan 2 · 12:05 AM");
	});
});

describe("day", () => {
	it("steps across month and year ends", () => {
		expect(stepDay("2026-09-30", 1)).toBe("2026-10-01");
		expect(stepDay("2026-03-01", -1)).toBe("2026-02-28");
		expect(stepDay("2026-12-31", 1)).toBe("2027-01-01");
	});
	it("covers a local day west of UTC with the UTC days it touches, plus the one before", () => {
		// 2026-09-15 in a UTC-5 zone: 05:00Z on the 15th to 05:00Z on the 16th.
		const start = Date.UTC(2026, 8, 15, 5);
		expect(utcDatesFor(start, start + 86_400_000)).toEqual(["2026-09-14", "2026-09-15", "2026-09-16"]);
	});
});
