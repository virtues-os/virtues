import { describe, expect, it } from "vitest";
import {
	assignLanes,
	columnLabel,
	hasValues,
	isDateColumn,
	isShowOutput,
	isTimeColumn,
	labelIndices,
	niceTicks,
	parseTime,
} from "./show";

describe("niceTicks", () => {
	it("covers the range in round steps", () => {
		expect(niceTicks(0, 7.4)).toEqual([0, 2, 4, 6, 8]);
		expect(niceTicks(0, 100)).toEqual([0, 25, 50, 75, 100]);
	});
	it("includes zero for a single value", () => {
		expect(niceTicks(5, 5)[0]).toBe(0);
		expect(niceTicks(0, 0)).toEqual([0, 1]);
	});
	it("handles negatives", () => {
		const t = niceTicks(-3, 9);
		expect(t[0]).toBeLessThanOrEqual(-3);
		expect(t[t.length - 1]).toBeGreaterThanOrEqual(9);
	});
});

describe("parseTime", () => {
	it("reads a bare date as local midnight, not UTC", () => {
		const t = parseTime("2026-09-08")!;
		expect([t.getFullYear(), t.getMonth(), t.getDate(), t.getHours()]).toEqual([2026, 8, 8, 0]);
	});
	it("reads RFC 3339 and the box's naive timestamps", () => {
		expect(parseTime("2026-09-08T14:00:00+00:00")?.toISOString()).toBe("2026-09-08T14:00:00.000Z");
		expect(parseTime("2026-09-08 14:30:00")?.getMinutes()).toBe(30);
	});
	it("refuses anything else", () => {
		expect(parseTime("Tuesday")).toBeNull();
		expect(parseTime(42)).toBeNull();
	});
});

describe("columns", () => {
	it("tells a date axis from a time axis from a category", () => {
		expect(isDateColumn(["2026-09-01", "2026-09-08"])).toBe(true);
		expect(isDateColumn(["2026-09-01 10:00:00"])).toBe(false);
		expect(isTimeColumn(["2026-09-01 10:00:00", null])).toBe(true);
		expect(isTimeColumn(["Mon", "Tue"])).toBe(false);
	});
});

describe("labelIndices", () => {
	it("keeps the ends and spreads the rest", () => {
		expect(labelIndices(3, 6)).toEqual([0, 1, 2]);
		expect(labelIndices(31, 4)).toEqual([0, 10, 20, 30]);
	});
});

describe("assignLanes", () => {
	it("stacks only what overlaps", () => {
		const lanes = assignLanes([
			{ start: 0, end: 10 },
			{ start: 5, end: 8 },
			{ start: 10, end: 12 },
		]);
		expect(lanes).toEqual([0, 1, 0]);
	});
});

describe("isShowOutput", () => {
	it("accepts the six kinds only", () => {
		expect(isShowOutput({ kind: "chart" })).toBe(true);
		expect(isShowOutput({ kind: "pie" })).toBe(false);
		expect(isShowOutput(null)).toBe(false);
	});
});

describe("columnLabel", () => {
	it("humanizes a schema name and leaves an alias alone", () => {
		expect(columnLabel("calendar_name")).toBe("Calendar name");
		expect(columnLabel("Hours asleep")).toBe("Hours asleep");
		expect(columnLabel("Week of")).toBe("Week of");
	});
});

describe("hasValues", () => {
	it("treats null, missing and empty text as no value", () => {
		expect(hasValues([{ a: null }, { a: "" }, {}], "a")).toBe(false);
		expect(hasValues([{ a: null }, { a: 0 }], "a")).toBe(true);
	});
});
