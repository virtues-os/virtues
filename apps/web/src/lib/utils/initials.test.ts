import { describe, expect, it } from "vitest";
import { initials } from "./initials";

describe("initials", () => {
	it("takes the first and last words", () => {
		expect(initials("David Ade Okafor")).toBe("DO");
		expect(initials("  nick   example ")).toBe("NE");
	});

	it("takes two letters of a one-word name", () => {
		expect(initials("Nick")).toBe("NI");
		expect(initials("N")).toBe("N");
	});

	it("never splits a character in two", () => {
		expect(initials("𝒩ick")).toBe("𝒩I");
	});

	it("draws a question mark for no name at all", () => {
		expect(initials("   ")).toBe("?");
	});
});
