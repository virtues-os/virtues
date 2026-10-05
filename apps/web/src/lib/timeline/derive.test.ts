import { describe, expect, it } from "vitest";
import type { TimelineDay } from "./day";
import { deriveDay, isTravel, joinDays, trackOver, uncovered } from "./derive";
import { toFixes } from "./track";

// Open ocean, so no real place, and a way to step north by metres.
const LNG = -30;
const north = (m: number) => m / 111_195;
const T0 = Date.UTC(2026, 0, 1, 6); // local midnight in a UTC-6 zone
const iso = (min: number) => new Date(T0 + min * 60_000).toISOString();
const fix = (min: number, m = 0) => ({
	latitude: north(m),
	longitude: LNG,
	timestamp: iso(min),
	horizontal_accuracy: 5,
	speed: null,
});

function day(over: Partial<TimelineDay> = {}): TimelineDay {
	return {
		date: "2026-01-01",
		zone: "America/Chicago",
		started_at: iso(0),
		ended_at: iso(24 * 60),
		stays: [],
		points: [],
		last_point_before: null,
		nights: [],
		sessions: [],
		steps: [],
		step_scale: 100,
		calendar: [],
		...over,
	};
}
const stay = (id: string, s: number, e: number, m = 0) => ({
	id,
	started_at: iso(s),
	ended_at: iso(e),
	latitude: north(m),
	longitude: LNG,
	place_id: `place_${id}`,
	place_name: null,
	place_is_named: false,
});
const sources = { calendar: true, finance: false };

describe("uncovered", () => {
	it("returns the pieces of a window no span covers", () => {
		expect(uncovered(0, 100, [{ s: 10, e: 20 }, { s: 15, e: 40 }, { s: 90, e: 120 }])).toEqual([
			{ s: 0, e: 10 },
			{ s: 40, e: 90 },
		]);
	});
});

describe("travel between stays", () => {
	it("is a drive when the track moved at driving speed", () => {
		const fixes = toFixes([fix(0), fix(5, 2000), fix(10, 5000), fix(15, 8000)]);
		const t = trackOver(fixes, 1 * 60_000 + T0, 14 * 60_000 + T0);
		expect(t.peakKmh).toBeGreaterThan(30);
		expect(isTravel(T0, T0 + 13 * 60_000, t)).toBe(true);
	});
	it("is a signal gap when the phone went quiet in one place and came back in another", () => {
		// 2 km apart, 3 hours later: slower than a walk.
		const fixes = toFixes([fix(0), fix(180, 2000)]);
		const t = trackOver(fixes, T0 + 60_000, T0 + 179 * 60_000);
		expect(isTravel(T0 + 60_000, T0 + 179 * 60_000, t)).toBe(false);
	});
});

describe("deriveDay", () => {
	it("files the time between two stays as a drive or a gap, and leaves seams out", () => {
		const d = deriveDay(
			day({
				stays: [stay("a", 0, 8 * 60), stay("b", 8 * 60 + 2, 9 * 60, 50), stay("c", 9 * 60 + 30, 24 * 60, 9000)],
				points: [fix(8 * 60 + 55, 50), fix(9 * 60 + 5, 3000), fix(9 * 60 + 15, 6000), fix(9 * 60 + 31, 9000)],
			}),
			sources,
		);
		const kinds = d.derived.spans.map((s) => s.kind);
		// The 2-minute seam between a and b is no stretch of its own.
		expect(kinds).toEqual(["stay", "stay", "transit", "stay"]);
	});

	it("tells a quiet stretch what the phone did meanwhile", () => {
		const d = deriveDay(
			day({
				stays: [stay("a", 0, 6 * 60), stay("b", 10 * 60, 24 * 60)],
				sessions: [
					{ id: "s1", started_at: iso(7 * 60), ended_at: iso(7 * 60 + 20), speaker_mode: 2, title: "Weekend plans", content: "We talked." },
				],
			}),
			sources,
		);
		const gap = d.derived.spans.find((s) => s.kind === "unknown");
		expect(gap?.metadata.conversation_count).toBe(1);
		expect(d.derived.moments.map((m) => m.title)).toEqual(["Weekend plans"]);
	});

	it("keeps a night as a night, and never calls its silence a gap", () => {
		const d = deriveDay(
			day({
				stays: [stay("a", 7 * 60, 24 * 60)],
				nights: [{ started_at: iso(-60), ended_at: iso(7 * 60), asleep_minutes: 470 }],
			}),
			sources,
		);
		expect(d.derived.spans.map((s) => s.kind)).toEqual(["sleep", "stay"]);
	});

	it("leaves declined, cancelled and all-day events off the calendar lane", () => {
		const ev = (id: string, extra: object) => ({
			id,
			title: id,
			started_at: iso(60),
			ended_at: iso(120),
			is_all_day: false,
			calendar_name: null,
			location_name: null,
			status: "confirmed",
			response_status: "accepted",
			...extra,
		});
		const d = deriveDay(
			day({
				calendar: [
					ev("kept", {}),
					ev("declined", { response_status: "declined" }),
					ev("cancelled", { status: "cancelled" }),
					ev("allday", { is_all_day: true }),
					ev("invite", { response_status: "needsAction" }),
				],
			}),
			sources,
		);
		expect(d.lanes.calendar.map((e) => [e.id, e.is_unanswered])).toEqual([
			["kept", false],
			["invite", true],
		]);
	});
});

describe("joinDays", () => {
	it("counts a stay across midnight once", () => {
		const a = deriveDay(day({ stays: [stay("x", 20 * 60, 30 * 60)] }), sources);
		const b = deriveDay(day({ stays: [stay("x", 20 * 60, 30 * 60)] }), sources);
		expect(joinDays([a, b]).derived.spans.filter((s) => s.kind === "stay")).toHaveLength(1);
	});
});
