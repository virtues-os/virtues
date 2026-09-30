import { describe, expect, it } from "vitest";
import { audioTag, buildRail, fmtDur, placeTitle, transitTitle, type DerivedWindow } from "./rail";

const at = (hhmm: string) => `2026-09-22T${hhmm}:00Z`;
const day = { start: Date.parse(at("00:00")), end: Date.parse("2026-09-23T00:00:00Z") };

const window: DerivedWindow = {
	is_built: true,
	places: [
		{ id: "home", is_home: true, is_work: false, place_name: "Location 30.0000, -97.0000" },
		{ id: "cafe", is_home: false, is_work: false, place_name: "Corner Cafe" },
	],
	spans: [
		{ id: "a", kind: "stay", started_at: "2026-09-21T20:00:00Z", ended_at: at("08:00"), timeline_place_id: "home", metadata: {} },
		{ id: "n", kind: "sleep", started_at: "2026-09-21T23:00:00Z", ended_at: at("06:00"), timeline_place_id: null, metadata: { source: "healthkit" } },
		{ id: "b", kind: "transit", started_at: at("08:00"), ended_at: at("08:20"), timeline_place_id: null, metadata: { peak_kmh: 60 } },
		{ id: "c", kind: "stay", started_at: at("08:20"), ended_at: at("12:00"), timeline_place_id: "cafe", metadata: {} },
		{
			id: "d",
			kind: "unknown",
			started_at: at("12:00"),
			ended_at: at("15:00"),
			timeline_place_id: null,
			metadata: { fix_count: 0, conversation_count: 2, step_bin_count: 1 },
		},
	],
	moments: [
		// Starts a minute before the cafe stay, but its middle is inside it.
		{ id: "m1", kind: "conversation", started_at: at("08:19"), ended_at: at("08:40"), title: "Coffee plans", metadata: { window_ids: ["w1", "w2"] } },
		{ id: "m2", kind: "walk", started_at: at("10:00"), ended_at: at("10:30"), title: null, metadata: { path_meters: 2100, kmh: 4.2 } },
	],
};

describe("the rail", () => {
	it("titles sections the prototype's way", () => {
		const rail = buildRail(window, day.start, day.end);
		expect(rail.map((s) => [s.kind, s.title, s.dur])).toEqual([
			["sleep", "In Bed", "7h"], // a night keeps its full length, and leads a stay opening at midnight too
			["place", "Home", "8h"], // clipped to the day
			["transit", "Driving", "20m"],
			["place", "Corner Cafe", "3h 40m"],
			["gap", "Signal gap", "3h"],
		]);
		expect(rail[4].notes).toEqual(["likely still at Corner Cafe", "Phone on, mic active"]);
	});

	it("files a row under the section holding its middle", () => {
		const rail = buildRail(window, day.start, day.end);
		expect(rail[3].rows.map((r) => r.title)).toEqual(["Coffee plans", "Walk · 2.1 km"]);
		expect(rail[3].rows[0].windowIds).toEqual(["w1", "w2"]);
		expect(rail[2].rows).toEqual([]);
	});

	it("never shows the wiki's coordinate stub as a name", () => {
		expect(placeTitle({ id: "x", is_home: false, is_work: false, place_name: "Location 1.0, 2.0" })).toBe("Unnamed place");
	});

	it("calls the most-dwelt place Work / frequent until it has a real name", () => {
		expect(placeTitle({ id: "w", is_home: false, is_work: true, place_name: "Location 1.0, 2.0" })).toBe("Work / frequent");
		expect(placeTitle({ id: "w", is_home: false, is_work: true, place_name: "The Studio" })).toBe("The Studio");
	});

	it("gives a conversation its windows, a night its source, a quiet stay its audio note", () => {
		const win = (id: string, from: string, to: string, speakers: number) => ({
			id,
			started_at: at(from),
			ended_at: at(to),
			speaker_count: speakers,
			title: null,
			text: null,
			people: [],
		});
		const voice = [
			win("w1", "08:19", "08:24", 2),
			win("w2", "08:24", "08:29", 3),
			win("w3", "08:30", "08:35", 1), // one voice: not a conversation's window
		];
		const rail = buildRail(window, day.start, day.end, voice);
		expect(rail[3].rows[0].convs.map((c) => c.id)).toEqual(["w1", "w2"]);
		expect(rail[0].src).toBe("healthkit");
		// Home, 8 h with no conversation filed and the mic barely on: "no audio".
		expect(rail[1].atag).toBe("no audio");
		// The cafe has rows, so no note.
		expect(rail[3].atag).toBeNull();
	});

	it("tags a stay only when its audio is clear-cut", () => {
		const h = 3_600_000;
		expect(audioTag(0, h, [{ s: 0, e: 0.05 * h }])).toBe("no audio");
		expect(audioTag(0, h, [{ s: 0, e: 0.7 * h }])).toBe("silent");
		expect(audioTag(0, h, [{ s: 0, e: 0.3 * h }])).toBeNull();
	});

	it("names a long ground drive for what it is", () => {
		expect(transitTitle(90, 3 * 3_600_000)).toBe("Out · no stay recorded");
		expect(transitTitle(800, 3 * 3_600_000)).toBe("Flying");
		expect(transitTitle(20, 600_000)).toBe("In transit");
		expect(fmtDur(125 * 60_000)).toBe("2h 5m");
	});
});
