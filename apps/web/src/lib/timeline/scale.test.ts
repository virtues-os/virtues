import { describe, expect, it } from "vitest";
import { zoneOffset } from "./scale";

const HOUR = 3_600_000;

describe("a zone's offset", () => {
	it("follows daylight time", () => {
		expect(zoneOffset(Date.UTC(2026, 6, 28, 12), "America/Chicago")).toBe(-5 * HOUR);
		expect(zoneOffset(Date.UTC(2026, 0, 15, 12), "America/Chicago")).toBe(-6 * HOUR);
	});
});
