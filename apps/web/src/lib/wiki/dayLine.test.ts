import { describe, expect, it } from "vitest";
import type { LifelineLane } from "./api";
import type { DayEvent } from "./types";
import {
	coverageLanes,
	dayFraction,
	dayLineBlocks,
	dayWindow,
	hourTicks,
	mostUnlikeUsual,
	usualScore,
	usualWords,
} from "./dayLine";

const CHICAGO = "America/Chicago";
const HOUR = 3_600_000;

function ev(id: string, start: string, end: string, over: Partial<DayEvent> = {}): DayEvent {
	const s = new Date(start);
	const e = new Date(end);
	return {
		id,
		startTime: s,
		endTime: e,
		durationMinutes: Math.round((e.getTime() - s.getTime()) / 60_000),
		autoLabel: id,
		sourceIds: [],
		noveltyZ: null,
		localNoveltyZ: null,
		autonomicZ: null,
		avgHr: null,
		hrZ: null,
		topics: [],
		eventSummary: null,
		agentAction: null,
		isSleep: false,
		userHidden: false,
		entities: [],
		entityNames: {},
		topicNovelty: null,
		entityNovelty: null,
		entityTimestamps: null,
		isUserAdded: false,
		isUserEdited: false,
		...over,
	};
}

function lane(id: string, density: number[], firstSeen: string | null = "2020-01-01T00:00:00Z"): LifelineLane {
	return {
		id,
		sources: [],
		density,
		peak: Math.max(0, ...density),
		floor: 0,
		first_seen: firstSeen,
		measure: "records",
		measure_label: "Records",
		unit: "",
		kind: "total",
		available: [],
	};
}

describe("dayWindow", () => {
	it("runs local midnight to local midnight", () => {
		const w = dayWindow("2026-10-04", CHICAGO);
		expect(new Date(w.startMs).toISOString()).toBe("2026-10-04T05:00:00.000Z");
		expect(w.endMs - w.startMs).toBe(24 * HOUR);
	});

	it("is 23 hours on the spring-forward day and 25 on the fall-back day", () => {
		const spring = dayWindow("2026-03-08", CHICAGO);
		expect(new Date(spring.startMs).toISOString()).toBe("2026-03-08T06:00:00.000Z");
		expect(spring.endMs - spring.startMs).toBe(23 * HOUR);
		const fall = dayWindow("2026-11-01", CHICAGO);
		expect(new Date(fall.startMs).toISOString()).toBe("2026-11-01T05:00:00.000Z");
		expect(fall.endMs - fall.startMs).toBe(25 * HOUR);
	});

	it("labels hours by the local clock on a DST day", () => {
		const ticks = hourTicks("2026-03-08", CHICAGO, 6);
		expect(ticks.map((t) => t.label)).toEqual(["12 AM", "6 AM", "12 PM", "6 PM", "12 AM"]);
		// 6 AM is five real hours after midnight on the day that loses one.
		expect(ticks[1].at).toBeCloseTo(5 / 23, 6);
		expect(ticks[4].at).toBe(1);
	});
});

describe("dayLineBlocks", () => {
	const w = dayWindow("2026-10-04", CHICAGO);

	it("ends an event that ends at the next midnight at the right edge, never at zero", () => {
		const [b] = dayLineBlocks([ev("late", "2026-10-05T03:00:00Z", "2026-10-05T05:00:00Z")], w);
		expect(b.x1).toBeCloseTo(22 / 24, 6);
		expect(b.x2).toBe(1);
	});

	it("leaves unknown and hidden stretches out, and keeps an unscored event", () => {
		const blocks = dayLineBlocks(
			[
				ev("gap", "2026-10-04T05:00:00Z", "2026-10-04T12:00:00Z", { isUnknown: true }),
				ev("hidden", "2026-10-04T12:00:00Z", "2026-10-04T13:00:00Z", { userHidden: true }),
				ev("unscored", "2026-10-04T13:00:00Z", "2026-10-04T14:00:00Z"),
			],
			w,
		);
		expect(blocks.map((b) => [b.id, b.z])).toEqual([["unscored", null]]);
	});

	it("prefers the local score and clamps it to the chart", () => {
		const [b] = dayLineBlocks(
			[ev("mass", "2026-10-04T16:00:00Z", "2026-10-04T17:00:00Z", { noveltyZ: 0.2, localNoveltyZ: 4.8 })],
			w,
		);
		expect(b.z).toBe(3);
	});

	it("clamps an event that began the day before to the first midnight", () => {
		const [b] = dayLineBlocks([ev("night", "2026-10-04T02:00:00Z", "2026-10-04T08:00:00Z")], w);
		expect(b.x1).toBe(0);
		expect(b.x2).toBeCloseTo(3 / 24, 6);
	});
});

describe("how unlike your usual", () => {
	it("falls back to the global score when the local one is missing", () => {
		expect(usualScore(ev("a", "2026-10-04T16:00:00Z", "2026-10-04T17:00:00Z", { noveltyZ: 1.4 }))).toBe(1.4);
	});

	it("says so in words", () => {
		expect(usualWords(2.1)).toBe("Unlike your usual");
		expect(usualWords(0.4)).toBe("Like your usual");
		expect(usualWords(-1)).toBe("Like your usual");
		expect(usualWords(null)).toBe("Your server hasn't scored this");
	});

	it("names the event furthest from usual, and none when nothing is far enough", () => {
		const day = [
			ev("brunch", "2026-10-04T17:00:00Z", "2026-10-04T18:00:00Z", { localNoveltyZ: 1.6 }),
			ev("flight", "2026-10-04T23:00:00Z", "2026-10-05T01:00:00Z", { noveltyZ: 0.3, localNoveltyZ: 2.7 }),
			ev("gap", "2026-10-04T05:00:00Z", "2026-10-04T12:00:00Z", { isUnknown: true, noveltyZ: 5 }),
		];
		expect(mostUnlikeUsual(day)).toBe("flight");
		expect(mostUnlikeUsual([ev("desk", "2026-10-04T14:00:00Z", "2026-10-04T15:00:00Z", { noveltyZ: 0.9 })])).toBeNull();
	});
});

describe("coverageLanes", () => {
	const w = dayWindow("2026-10-04", CHICAGO);

	it("merges recorded buckets into runs and joins a stream's lanes", () => {
		const lanes = coverageLanes(
			[
				lane("communication/transcription", [0, 1, 1, 0]),
				lane("activity", [1, 0, 0, 0]),
				lane("location", [0, 0, 0, 2]),
			],
			w,
		);
		expect(lanes).toEqual([
			{ label: "Audio", runs: [[0.25, 0.75]] },
			{ label: "Location", runs: [[0.75, 1]] },
			{ label: "Screen", runs: [[0, 0.25]] },
		]);
	});

	it("bridges a sampled stream's quiet between readings, never a stream of messages", () => {
		const cells = (on: number[]) => Array.from({ length: 144 }, (_, i) => (on.includes(i) ? 1 : 0));
		const [loc, msg] = coverageLanes(
			[lane("location", cells([0, 3, 8])), lane("communication/message", cells([0, 3]))],
			w,
		);
		// Ten-minute cells: twenty quiet minutes is one fix to the next; forty is a gap.
		expect(loc.runs).toEqual([
			[0, 4 / 144],
			[8 / 144, 9 / 144],
		]);
		expect(msg.runs).toEqual([
			[0, 1 / 144],
			[3 / 144, 4 / 144],
		]);
	});

	it("shows a watched stream that recorded nothing, and leaves out one not started yet", () => {
		const lanes = coverageLanes(
			[lane("communication/message", [0, 0]), lane("health", [0, 0], "2026-12-01T00:00:00Z")],
			w,
		);
		expect(lanes).toEqual([{ label: "Messages", runs: [] }]);
	});
});

describe("dayFraction", () => {
	it("stays inside the day", () => {
		const w = dayWindow("2026-10-04", CHICAGO);
		expect(dayFraction(w.startMs - HOUR, w)).toBe(0);
		expect(dayFraction(w.endMs + HOUR, w)).toBe(1);
	});
});
