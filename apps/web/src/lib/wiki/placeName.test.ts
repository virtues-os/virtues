import { describe, expect, it } from "vitest";
import { placeLabel, realPlaceName } from "./placeName";

describe("realPlaceName", () => {
	it("keeps a name someone gave", () => {
		expect(realPlaceName("Okafor's Corner Bakery")).toBe("Okafor's Corner Bakery");
		expect(realPlaceName("  Home ")).toBe("Home");
	});

	it("refuses the coordinate stub, Unknown and nothing", () => {
		expect(realPlaceName("Location 38.7, -9.1")).toBeNull();
		expect(realPlaceName("Location 38.7223, -9.1393")).toBeNull();
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
		expect(placeLabel("Location 38.7, -9.1")).toBe("Unnamed place");
		expect(placeLabel("Fairhaven Airport")).toBe("Fairhaven Airport");
	});
});
