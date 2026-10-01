import { describe, expect, it } from "vitest";
import { buildFolds, clampView, DAY, FOLD_W, HOUR, MIN, ticks, tierOf, tierView, unwarp, warp, weekOf, zoneOffset } from "./scale";

describe("the folded axis", () => {
	it("folds each night, and 1-6 AM on a date no night touches", () => {
		const md1 = 0;
		const md2 = DAY;
		const folds = buildFolds([{ s: md1 - HOUR, e: md1 + 7 * HOUR }], [md1, md2]);
		expect(folds).toEqual([
			{ a: -HOUR, b: 7 * HOUR, est: false },
			{ a: DAY + HOUR, b: DAY + 6 * HOUR, est: true },
		]);
	});

	it("compresses a fold to a sliver and maps back exactly", () => {
		const folds = [{ a: 2 * HOUR, b: 8 * HOUR, est: false }];
		expect(warp(folds, HOUR)).toBe(HOUR);
		expect(warp(folds, 8 * HOUR)).toBe(2 * HOUR + FOLD_W);
		expect(warp(folds, 10 * HOUR)).toBe(4 * HOUR + FOLD_W);
		for (const t of [HOUR, 5 * HOUR, 9 * HOUR]) expect(unwarp(folds, warp(folds, t))).toBeCloseTo(t, 6);
	});
});

describe("tiers and the view", () => {
	it("reads a span as its tier and gives each tier its span", () => {
		expect([tierOf(14 * MIN), tierOf(3 * HOUR), tierOf(DAY), tierOf(7 * DAY)]).toEqual(["min", "hour", "day", "week"]);
		const week = { s: -3 * DAY, e: 4 * DAY };
		expect(tierView("hour", { s: 0, e: DAY }, week, 12 * HOUR)).toEqual([10.5 * HOUR, 13.5 * HOUR]);
		expect(tierView("week", { s: 0, e: DAY }, week, 0)).toEqual([-3 * DAY, 4 * DAY]);
	});

	it("takes the calendar week, Monday to Sunday, holding a date", () => {
		// 2026-06-11 is a Thursday; its week runs Mon Jun 8 to Sun Jun 14.
		const w = weekOf("2026-06-11", "America/Chicago");
		expect(new Date(w.s).toISOString()).toBe("2026-06-08T05:00:00.000Z");
		expect(new Date(w.e).toISOString()).toBe("2026-06-15T05:00:00.000Z");
		// A Monday's week starts that day; a Sunday's started six days before.
		expect(weekOf("2026-06-08", "America/Chicago")).toEqual(w);
		expect(weekOf("2026-06-14", "America/Chicago")).toEqual(w);
	});

	it("keeps a view wide enough and inside its bounds", () => {
		expect(clampView(10 * MIN, 11 * MIN, 0, DAY)).toEqual([9 * MIN, 12 * MIN]);
		expect(clampView(-HOUR, HOUR, 0, DAY)).toEqual([0, 2 * HOUR]);
	});
});

describe("the ruler", () => {
	it("labels a day every three hours in local time", () => {
		const offset = -5 * HOUR; // CDT
		const start = 5 * HOUR; // local midnight
		const labels = ticks(start, start + DAY, 900, offset).map(([, l]) => l);
		expect(labels.slice(0, 3)).toEqual(["12AM", "3AM", "6AM"]);
	});

	it("reads a zone's offset", () => {
		expect(zoneOffset(Date.UTC(2026, 6, 28, 12), "America/Chicago")).toBe(-5 * HOUR);
		expect(zoneOffset(Date.UTC(2026, 0, 15, 12), "America/Chicago")).toBe(-6 * HOUR);
	});
});
