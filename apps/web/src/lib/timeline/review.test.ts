import { describe, expect, it } from "vitest";
import { askFor, focusZoom, homeOf, pickAnswer, weekly, type PlaceCandidate, type PlaceQuestion } from "./review";

const cand = (name: string, meters: number, category = "dining"): PlaceCandidate => ({ name, meters, kind: "cafe", category, lat: 30, lon: -97 });
const question = (over: Partial<Omit<PlaceQuestion, "naming">> & { naming?: Partial<PlaceQuestion["naming"]> } = {}): PlaceQuestion => ({
	ask: "place",
	changed: false,
	id: "p",
	latitude: 30,
	longitude: -97,
	hours: 2,
	stop_count: 1,
	overnight: false,
	rejected_match: null,
	title: "Riverton",
	recall: "",
	best_day: null,
	hours_a_week: 0,
	nights_a_week: 0,
	...over,
	naming: {
		tier: "none",
		name: null,
		category: null,
		candidates: [],
		hint: null,
		lodging: null,
		nearby: [],
		city: "Riverton",
		street_detail: true,
		...over.naming,
	},
});

describe("the place questions", () => {
	it("asks whether it's home, or where the person works, never assuming either", () => {
		const h = askFor(question({ ask: "home", overnight: true, naming: { tier: "confident", name: "Corner Cafe" } }));
		expect(h.text).toBe("Is this home?");
		expect(h.chips.map((c) => [c.label, c.answer])).toEqual([
			["Yes", { kind: "home" }],
			["No", { kind: "not_home" }],
		]);
		expect(h.line).toBe("Is this home?");
		expect(h.yes).toEqual({ kind: "home" });
		expect(h.other).toBeNull();
		expect(h.role).toBe("home");
		expect(askFor(question({ ask: "work" })).text).toBe("Is this where you work?");
		// A place that has led since Home was set elsewhere.
		expect(askFor(question({ ask: "home", changed: true })).text).toBe("Is this your home now?");
		expect(weekly(question({ ask: "home", nights_a_week: 5.6 }))).toBe("About 6 nights a week");
		expect(weekly(question({ ask: "work", hours_a_week: 10.2 }))).toBe("About 10 hours a week");
	});

	it("asks which of several close places, any number picked together", () => {
		const a = askFor(question({ naming: { tier: "ambiguous", candidates: [cand("Maple Diner", 20), cand("Pine Grill", 30)] } }));
		expect(a.text).toBe("Which of these were you at?");
		// In the day's list the question is one line: the count, not the names.
		expect(a.line).toBe("Which of 2 places nearby?");
		expect(a.yes).toBeUndefined();
		expect(a.picks.map((c) => c.name)).toEqual(["Maple Diner", "Pine Grill"]);
		expect(a.chips).toEqual([]);
		expect(a.shown).toHaveLength(2);
		expect(a.reachM).toBe(30);
		expect(pickAnswer(a.picks)).toEqual({ kind: "name", name: "Maple Diner, Pine Grill", category: "dining" });
		expect(pickAnswer([cand("Blue Door", 10), cand("Corner Park", 20, "outdoors")])).toEqual({ kind: "name", name: "Blue Door, Corner Park", category: null });
	});

	it("asks about one named place within reach by name, with a No that sets it aside", () => {
		const a = askFor(question({ naming: { candidates: [cand("Blue Door", 40)] } }));
		expect(a.text).toBe("Were you at Blue Door?");
		// A yes-or-no question answers from the list's line.
		expect(a.yes).toEqual({ kind: "name", name: "Blue Door", category: "dining" });
		expect(a.no).toEqual({ kind: "not_this", name: "Blue Door" });
		expect(a.chips.map((c) => c.label)).toEqual(["Yes, that’s it", "No"]);
		expect(a.other).toBe("place");
	});

	it("offers a named place further out only as a suggestion, never by name", () => {
		const a = askFor(question({ naming: { hint: cand("Country Club", 140, "outdoors"), nearby: ["Country Club", "Pine Grill"] } }));
		expect(a.text).toBe("Virtues couldn’t name this spot. What was it?");
		expect(a.yes).toBeUndefined();
		expect(a.chips.map((c) => c.label)).toEqual(["Country Club", "Pine Grill"]);
		expect(a.chips[0].answer).toEqual({ kind: "name", name: "Country Club", category: "outdoors" });
	});

	it("after a night, asks whose home, offering a nearby place too", () => {
		const a = askFor(question({ overnight: true }));
		expect(a.text).toBe("Looks like you stayed at a home. Whose place?");
		expect(a.line).toBe("Whose place was this?");
		expect(a.chips.map((c) => c.label)).toEqual(["Mine / family"]);
		expect(a.other).toBe("person");
		const b = askFor(question({ overnight: true, naming: { candidates: [cand("Blue Door", 40)] } }));
		expect(b.text).toBe("Were you at Blue Door, or someone’s home?");
		expect(b.other).toBe("person");
		expect(homeOf(" Sam ")).toBe("Sam’s place");
		// A night beside a hotel asks about the hotel, even with a nearer place.
		const h = askFor(question({ overnight: true, naming: { tier: "ambiguous", candidates: [cand("Maple Diner", 20), cand("Pine Grill", 30)], lodging: cand("Hotel Rowan", 90, "lodging") } }));
		expect(h.text).toBe("Were you at Hotel Rowan?");
		expect(h.chips[0].answer).toEqual({ kind: "name", name: "Hotel Rowan", category: "lodging" });
		// A hotel the map doesn't list is typed as a place, never as a person's home.
		expect(h.no).toEqual({ kind: "not_this", name: "Hotel Rowan" });
		expect(h.other).toBe("place");
		expect(askFor(question({ naming: { lodging: cand("Hotel Rowan", 90, "lodging") } })).text).not.toContain("Hotel Rowan");
		// Where the box holds no list of places, a night there is no evidence of a home.
		const c = askFor(question({ overnight: true, naming: { street_detail: false } }));
		expect(c.text).toBe("Where did you stay that night?");
		expect(c.other).toBe("place");
	});

	it("asks what it was when nothing is near, and never re-offers a rejected match", () => {
		const a = askFor(question());
		expect(a.text).toBe("Virtues couldn’t name this spot. What was it?");
		expect(a.line).toBe("What was this spot?");
		expect(a.chips.map((c) => c.label)).toEqual(["park", "office", "shop", "outdoors"]);
		expect(a.chips[0].answer).toEqual({ kind: "name", name: "park", category: null });
		const b = askFor(question({ rejected_match: "Riverside", naming: { tier: "confident", name: "Riverside", candidates: [cand("Riverside", 5), cand("Blue Door", 40)] } }));
		expect(b.text).toBe("Were you at Blue Door?");
	});

	it("frames the places asked about readably", () => {
		expect(focusZoom(30, 0, 14)).toBe(16.3);
		expect(focusZoom(30, 0, 17)).toBe(17);
		// A close pair about 25 m apart lands near z17 (main.js:2217); 100 m out, near z16.
		expect(focusZoom(30, 25, 14)).toBeCloseTo(17.27, 1);
		expect(focusZoom(30, 100, 14)).toBeCloseTo(16.12, 1);
		expect(focusZoom(30, 250, 14)).toBe(15.5);
	});
});
