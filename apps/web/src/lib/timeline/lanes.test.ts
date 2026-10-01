import { describe, expect, it } from "vitest";
import { barHeight, bars, barWidth, fitWords, laneData, ribbon, untangle, waveform } from "./lanes";
import type { DerivedWindow, VoiceWindow } from "./inspector";

const M = 60_000;

describe("the lanes", () => {
	it("holds the last stretch across a gap over three minutes", () => {
		const r = ribbon([
			{ s: 0, e: 10 * M, kind: "place", title: "Home" },
			{ s: 12 * M, e: 20 * M, kind: "transit", title: "Driving" },
			{ s: 30 * M, e: 40 * M, kind: "place", title: "Work" },
		]);
		expect(r.map((x) => [x.kind, x.held])).toEqual([
			["place", false],
			["transit", false],
			["transit", true],
			["place", false],
		]);
	});

	it("cuts a name at a word, never mid-word", () => {
		expect(fitWords("Team standup", 20)).toBe("Team standup");
		expect(fitWords("Weekly planning with design", 15)).toBe("Weekly…");
		expect(fitWords("Retrospective", 6)).toBe("");
	});

	it("draws a closed silhouette for a conversation", () => {
		const d = waveform(0, 30, 10, 6, 1234);
		expect(d.startsWith("M0.0 ")).toBe(true);
		expect(d.endsWith("Z")).toBe(true);
		expect(waveform(0, 1, 10, 6, 0)).toBe("");
	});

	it("draws no bar in bed and zones the rest by the record's scale", () => {
		const b = bars(
			[
				{ t: 0, v: 50 },
				{ t: 10 * M, v: 400 },
				{ t: 20 * M, v: 1000 },
				{ t: 30 * M, v: 900 },
			],
			[{ s: 25 * M, e: 60 * M }],
			800,
		);
		expect(b.map((x) => x.zone)).toEqual(["still", "walking", "active"]);
		expect(barHeight(0, 20)).toBe(2.5);
		expect(barHeight(1, 20)).toBe(20);
		expect(barWidth([0, 10, 20])).toBeCloseTo(5, 6);
	});
});

describe("the lane data", () => {
	const iso = (m: number) => new Date(m * M).toISOString();
	const derived: DerivedWindow = {
		last_stay_before: null,
		places: [{ id: "p1", latitude: 0, longitude: -30, is_home: true, is_work: false, place_name: null }],
		spans: [
			{ id: "n", kind: "sleep", started_at: iso(0), ended_at: iso(60), timeline_place_id: null, metadata: {} },
			{ id: "s", kind: "stay", started_at: iso(0), ended_at: iso(120), timeline_place_id: "p1", metadata: {} },
		],
		moments: [{ id: "c", kind: "conversation", started_at: iso(70), ended_at: iso(90), title: "Dogs", metadata: {} }],
	};
	const win = (s: number, e: number, speakers: number, people: string[]): VoiceWindow => ({
		id: `${s}`,
		started_at: iso(s),
		ended_at: iso(e),
		speaker_count: speakers,
		title: null,
		text: null,
		people,
	});

	it("reads the stretches, the nights and who a conversation mentioned", () => {
		const l = laneData(derived, [win(70, 80, 2, ["Ann"]), win(80, 90, 3, ["Ann", "Bo"]), win(100, 105, 1, [])], null, 0, 120 * M);
		expect(l.ribbon.map((r) => [r.kind, r.title])).toEqual([["place", "Home"]]);
		expect(l.nights).toEqual([{ s: 0, e: 60 * M }]);
		expect(l.conversations).toEqual([{ s: 70 * M, e: 90 * M, title: "Dogs", speakers: 3, people: ["Ann", "Bo"] }]);
		expect(l.voice).toHaveLength(3);
	});

	it("keeps a night whole when it began before the window", () => {
		const l = laneData(derived, [], null, 30 * M, 120 * M);
		expect(l.nights).toEqual([{ s: 0, e: 60 * M }]);
	});

	it("splits overlapping events down the middle, never stacking them", () => {
		const u = untangle([
			{ s: 60 * M, e: 120 * M },
			{ s: 0, e: 60 * M },
			{ s: 90 * M, e: 150 * M },
		]);
		expect(u.map((x) => x.pieces.map(([a, b]) => [a / M, b / M]))).toEqual([[[0, 60]], [[60, 105]], [[105, 150]]]);
		// Real spans stay for the peek.
		expect(u[1].e).toBe(120 * M);
	});

	it("lets a long event resume after a shorter one inside it", () => {
		const u = untangle([
			{ s: 9 * 60 * M, e: 17 * 60 * M },
			{ s: 15 * 60 * M, e: 16 * 60 * M },
		]);
		const h = (x: number) => x / (60 * M);
		expect(u.map((x) => x.pieces.map(([a, b]) => [h(a), h(b)]))).toEqual([[[9, 15.5], [16, 17]], [[15.5, 16]]]);
	});


	it("knows nothing of a source whose lanes didn't load", () => {
		const l = laneData(null, [], null, 0, M);
		expect([l.hasCalendar, l.hasFinance]).toEqual([null, null]);
	});
});
