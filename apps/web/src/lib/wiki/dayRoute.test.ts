import { describe, expect, it } from "vitest";
import type { TimelineDayLocationChunk, TimelineDayPoint, TimelineDayView } from "./api";
import { dayRoute, LONG_MOVE_M, realPlaceName, spanMetres } from "./dayRoute";

const at = (h: number, m: number) => new Date(Date.UTC(2026, 9, 4, h, m)).toISOString();
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
	date: "2026-10-04",
	chunks,
	points,
	last_point_before: null,
});

/** A walk north through town from 10:00, a fix a minute, then quiet until 11:30 two kilometres on. */
function townDay(): TimelineDayPoint[] {
	const walk = Array.from({ length: 20 }, (_, k) => point(10, k, 41.9 + 0.001 * k, -87.63));
	return [...walk, point(11, 30, 41.94, -87.63), point(11, 31, 41.9405, -87.63)];
}

describe("dayRoute", () => {
	it("draws nothing for a day with no location", () => {
		expect(dayRoute(null, null)).toBeNull();
		expect(dayRoute(view([]), null)).toBeNull();
	});

	it("draws one map for a day in town, dashed across the silence", () => {
		const route = dayRoute(view(townDay(), [stop(12, 0, 2, 41.94, -87.63, "Location 41.9400, -87.6300")]), "America/Chicago");
		expect(route?.long).toBeNull();
		expect(route?.quiet).toBe(true);
		const bridges = route?.city?.track.filter((p) => p.bridge) ?? [];
		expect(bridges).toHaveLength(1);
		expect(bridges[0].lat).toBeCloseTo(41.94);
		// The wiki's coordinate stub is no name.
		expect(route?.city?.stops[0].label).toBe("Unnamed place · 7:00 AM");
	});

	it("draws the long move on its own map when the day crossed one", () => {
		const chicago = townDay();
		const flight = [point(20, 45, 42.365, -71.01), point(20, 50, 42.38, -71.03)];
		const route = dayRoute(
			view(
				[...chicago, ...flight],
				[stop(12, 0, 3, 41.94, -87.63, "St. Example Church"), stop(20, 50, 1, 42.38, -71.03, "Home")],
			),
			"America/Chicago",
		);
		// The city is where the day mostly was; the far end of the flight is not on its map.
		expect(route?.city?.track.every((p) => p.lat > 40)).toBe(true);
		expect(route?.city?.stops.map((s) => s.label)).toEqual(["St. Example Church · 7:00 AM"]);
		// The long move runs from the last fix in the city to the day's end, dashed across the flight.
		const long = route?.long?.track ?? [];
		expect(long[0].lat).toBeCloseTo(41.9405);
		expect(long[long.length - 1].lat).toBeCloseTo(42.38);
		expect(long[1].bridge).toBe(true);
		expect(route?.long?.stops.map((s) => s.label)).toEqual(["6:31 AM", "3:50 PM"]);
	});

	it("leaves the city map out when the long move left it one fix and no stop", () => {
		// Three cities hours apart: no two fixes share a city, so the city is the first fix alone.
		const route = dayRoute(view([point(6, 0, 41.9, -87.63), point(8, 0, 35.15, -90.05), point(10, 0, 42.36, -71.0)]), "America/Chicago");
		expect(route?.city).toBeNull();
		expect(route?.long?.track).toHaveLength(3);
	});

	it("measures how far apart the fixes lie", () => {
		expect(spanMetres([])).toBe(0);
		expect(spanMetres([{ lat: 39.861, lng: -104.673 }, { lat: 42.365, lng: -71.01 }])).toBeGreaterThan(LONG_MOVE_M);
	});

	it("knows a real place name from the wiki's stub", () => {
		expect(realPlaceName("Location 41.9, -87.6")).toBeNull();
		expect(realPlaceName("Unknown")).toBeNull();
		expect(realPlaceName("  ")).toBeNull();
		expect(realPlaceName("Fairhaven")).toBe("Fairhaven");
	});
});
