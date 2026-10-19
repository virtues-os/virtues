import { describe, expect, it } from "vitest";
import type { TimelineDayLocationChunk, TimelineDayPoint, TimelineDayView } from "./api";
import { dayRoute, LONG_MOVE_M, realPlaceName, spanMetres } from "./dayRoute";

const at = (h: number, m: number) => new Date(Date.UTC(2026, 4, 16, h, m)).toISOString();
const point = (h: number, m: number, latitude: number, longitude: number): TimelineDayPoint => ({
	latitude,
	longitude,
	timestamp: at(h, m),
	horizontal_accuracy: 10,
	speed: null,
});
const stop = (h: number, m: number, hours: number, latitude: number, longitude: number, place_name: string | null): TimelineDayLocationChunk => ({
	type: "location",
	start_time: at(h, m),
	end_time: new Date(Date.parse(at(h, m)) + hours * 3_600_000).toISOString(),
	place_name,
	latitude,
	longitude,
	place_id: null,
	duration_minutes: hours * 60,
	place_category: null,
});
const view = (points: TimelineDayPoint[], chunks: TimelineDayLocationChunk[] = []): TimelineDayView => ({
	date: "2026-05-16",
	chunks,
	points,
	last_point_before: null,
});

/** A walk north through town from 10:00, a fix a minute, then quiet until 11:30 two kilometres on. */
function townDay(): TimelineDayPoint[] {
	const walk = Array.from({ length: 20 }, (_, k) => point(10, k, 38.7 + 0.001 * k, -9.14));
	return [...walk, point(11, 30, 38.74, -9.14), point(11, 31, 38.7405, -9.14)];
}

describe("dayRoute", () => {
	it("draws nothing for a day with no location", () => {
		expect(dayRoute(null, null)).toBeNull();
		expect(dayRoute(view([]), null)).toBeNull();
	});

	it("draws one map for a day in town, dashed across the silence", () => {
		const route = dayRoute(view(townDay(), [stop(12, 0, 2, 38.74, -9.14, "Location 38.7400, -9.1400")]), "Europe/Lisbon");
		expect(route?.long).toBeNull();
		expect(route?.quiet).toBe(true);
		const bridges = route?.city?.track.filter((p) => p.bridge) ?? [];
		expect(bridges).toHaveLength(1);
		expect(bridges[0].lat).toBeCloseTo(38.74);
		// The wiki's coordinate stub is no name.
		expect(route?.city?.stops[0].label).toBe("Unnamed place · 1:00 PM");
	});

	it("draws the long move on its own map when the day crossed one", () => {
		const town = townDay();
		const train = [point(20, 45, 41.14, -8.6), point(20, 50, 41.15, -8.61)];
		const route = dayRoute(
			view(
				[...town, ...train],
				[stop(12, 0, 3, 38.74, -9.14, "The Example Museum"), stop(20, 50, 1, 41.15, -8.61, "Example Hotel")],
			),
			"Europe/Lisbon",
		);
		// The city is where the day mostly was; the far end of the train ride is not on its map.
		expect(route?.city?.track.every((p) => p.lat < 39)).toBe(true);
		expect(route?.city?.stops.map((s) => s.label)).toEqual(["The Example Museum · 1:00 PM"]);
		// The long move runs from the last fix in the city to the day's end, dashed across the ride.
		const long = route?.long?.track ?? [];
		expect(long[0].lat).toBeCloseTo(38.7405);
		expect(long[long.length - 1].lat).toBeCloseTo(41.15);
		expect(long[1].bridge).toBe(true);
		expect(route?.long?.stops.map((s) => s.label)).toEqual(["12:31 PM", "9:50 PM"]);
	});

	it("leaves the city map out when the long move left it one fix and no stop", () => {
		// Three cities hours apart: no two fixes share a city, so the city is the first fix alone.
		const route = dayRoute(view([point(6, 0, 38.72, -9.14), point(8, 0, 40.42, -3.7), point(10, 0, 41.39, 2.17)]), "Europe/Lisbon");
		expect(route?.city).toBeNull();
		expect(route?.long?.track).toHaveLength(3);
	});

	it("measures how far apart the fixes lie", () => {
		expect(spanMetres([])).toBe(0);
		expect(spanMetres([{ lat: 38.774, lng: -9.134 }, { lat: 60.194, lng: 11.1 }])).toBeGreaterThan(LONG_MOVE_M);
	});

	it("knows a real place name from the wiki's stub", () => {
		expect(realPlaceName("Location 38.7, -9.1")).toBeNull();
		expect(realPlaceName("Unknown")).toBeNull();
		expect(realPlaceName("  ")).toBeNull();
		expect(realPlaceName("Fairhaven")).toBe("Fairhaven");
	});
});
