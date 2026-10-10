import { describe, expect, it } from "vitest";
import { placeLabel, realPlaceName } from "./placeName";

describe("realPlaceName", () => {
	it("keeps a name someone gave", () => {
		expect(realPlaceName("St. Paul's Cathedral")).toBe("St. Paul's Cathedral");
		expect(realPlaceName("  Home ")).toBe("Home");
	});

	it("refuses the coordinate stub, Unknown and nothing", () => {
		expect(realPlaceName("Location 41.9, -87.6")).toBeNull();
		expect(realPlaceName("Location 41.8781, -87.6298")).toBeNull();
		expect(realPlaceName("Location -33.8688,151.2093")).toBeNull();
		expect(realPlaceName("Unknown")).toBeNull();
		expect(realPlaceName("")).toBeNull();
		expect(realPlaceName(null)).toBeNull();
	});

	it("keeps a real name that happens to start with Location", () => {
		expect(realPlaceName("Location Bar")).toBe("Location Bar");
	});
});

describe("placeLabel", () => {
	it("reads Unnamed place for a stop with no name", () => {
		expect(placeLabel("Location 41.9, -87.6")).toBe("Unnamed place");
		expect(placeLabel("Fairhaven Airport")).toBe("Fairhaven Airport");
	});
});
