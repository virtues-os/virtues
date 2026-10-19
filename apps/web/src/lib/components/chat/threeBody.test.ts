import { describe, expect, it } from "vitest";
import { FIGURE_EIGHT_PERIOD, advance, energy, figureEight } from "./threeBody";

describe("three-body figure-eight", () => {
	it("closes on itself after one period", () => {
		const start = figureEight();
		const b = figureEight();
		advance(b, FIGURE_EIGHT_PERIOD);
		for (let i = 0; i < 3; i++) {
			for (let k = 0; k < 3; k++) {
				expect(Math.abs(b.pos[i][k] - start.pos[i][k])).toBeLessThan(5e-3);
			}
		}
	});

	it("holds its energy over ten orbits", () => {
		const b = figureEight();
		const e0 = energy(b);
		advance(b, FIGURE_EIGHT_PERIOD * 10);
		expect(Math.abs((energy(b) - e0) / e0)).toBeLessThan(1e-4);
	});
});
