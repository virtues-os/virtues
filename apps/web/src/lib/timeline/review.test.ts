import { describe, expect, it } from "vitest";
import { askFor, distance, homeOf, placeIcon, pickAnswer, weekly, type PlaceCandidate, type PlaceQuestion } from "./review";

const cand = (name: string, meters: number, category = "dining"): PlaceCandidate => ({ name, meters, kind: "cafe", category, lat: 40, lon: -40 });
const question = (over: Partial<Omit<PlaceQuestion, "naming">> & { naming?: Partial<PlaceQuestion["naming"]> } = {}): PlaceQuestion => ({
	ask: "place",
	changed: false,
	id: "p",
	latitude: 40,
	longitude: -40,
	hours: 2,
	stop_count: 1,
	overnight: false,
	rejected_match: null,
	title: "Riverton",
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
		hotels: [],
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
		expect(h.kinds).toEqual([]);
		expect(h.role).toBe("home");
		expect(askFor(question({ ask: "work" })).text).toBe("Is this where you work?");
		// A place that has led since Home was set elsewhere.
		expect(askFor(question({ ask: "home", changed: true })).text).toBe("Is this your home now?");
		expect(weekly(question({ ask: "home", nights_a_week: 5.6 }))).toBe("About 6 nights a week");
		expect(weekly(question({ ask: "work", hours_a_week: 10.2 }))).toBe("About 10 hours a week");
	});

	it("asks which of several close places, any number picked together, with the kinds for none of these", () => {
		const a = askFor(question({ naming: { tier: "ambiguous", candidates: [cand("Maple Diner", 20), cand("Pine Grill", 30)] } }));
		expect(a.text).toBe("Which of these were you at?");
		expect(a.line).toBe("Which of 2 places nearby?");
		expect(a.picks.map((c) => c.name)).toEqual(["Maple Diner", "Pine Grill"]);
		expect(a.kinds.map((k) => k.label)).toContain("Restaurant");
		// "None of these" lists the named places further out, the close ones not again.
		const m = askFor(question({ naming: { tier: "ambiguous", candidates: [cand("Maple Diner", 20), cand("Pine Grill", 30)], nearby: [cand("Maple Diner", 20), cand("Pine Grill", 30), cand("Blue Door", 90)] } }));
		expect(m.more).toEqual(["Blue Door"]);
		expect(pickAnswer(a.picks)).toEqual({ kind: "name", name: "Maple Diner, Pine Grill", category: "dining" });
		expect(pickAnswer([cand("Blue Door", 10), cand("Corner Park", 20, "outdoors")])).toEqual({ kind: "name", name: "Blue Door, Corner Park", category: null });
	});

	it("asks about one named place within 60 m by name, with a No that sets it aside", () => {
		const a = askFor(question({ naming: { candidates: [cand("Blue Door", 40)] } }));
		expect(a.text).toBe("Were you at Blue Door?");
		expect(a.yes).toEqual({ kind: "name", name: "Blue Door", category: "dining" });
		expect(a.no).toEqual({ kind: "not_this", name: "Blue Door" });
		expect(a.chips.map((c) => c.label)).toEqual(["Yes, that’s it", "No"]);
	});

	it("never asks about a place further out by name: the kinds, and its name only as a suggestion", () => {
		const q = question({ naming: { hint: cand("Car Dealer", 175, "shopping"), nearby: [cand("Car Dealer", 175, "shopping"), cand("Pine Grill", 200)] } });
		const a = askFor(q);
		expect(a.text).toBe("Where were you?");
		expect(a.yes).toBeUndefined();
		expect(a.chips).toEqual([]);
		expect(a.kinds.map((k) => k.label)).toEqual(["Restaurant", "Café", "Bar", "Shop", "Park", "Work", "Someone’s home"]);
		expect(a.passing).toBe(true);
	});

	it("asks where the person stayed after a night: a hotel within 60 m by name, else the kinds of a night", () => {
		const hotel = cand("Riverton Inn", 40, "lodging");
		const h = askFor(question({ overnight: true, naming: { lodging: hotel, hotels: [hotel] } }));
		expect(h.text).toBe("Were you at Riverton Inn?");
		expect(h.no).toEqual({ kind: "not_this", name: "Riverton Inn" });
		expect(h.passing).toBe(false);
		// A hotel further out, or a restaurant close by, is no answer for a night.
		const far = cand("Riverton Inn", 146, "lodging");
		const q = question({ overnight: true, naming: { candidates: [cand("Wings", 44)], lodging: far, hotels: [far, cand("Far Inn", 900, "lodging")] } });
		const n = askFor(q);
		expect(n.text).toBe("Where did you stay that night?");
		expect(n.kinds.map((k) => k.label)).toEqual(["A hotel", "Someone’s home"]);
		expect(n.passing).toBe(false);
		expect(homeOf(" Sam ")).toBe("Sam’s place");
	});

	it("never re-offers a rejected match", () => {
		const b = askFor(question({ rejected_match: "Riverside", naming: { tier: "confident", name: "Riverside", candidates: [cand("Riverside", 5), cand("Blue Door", 40)] } }));
		expect(b.text).toBe("Were you at Blue Door?");
		const h = cand("Riverton Inn", 40, "lodging");
		expect(askFor(question({ overnight: true, rejected_match: "Riverton Inn", naming: { lodging: h, hotels: [h] } })).text).toBe("Where did you stay that night?");
	});

	it("marks each place by its kind, else its category, with its distance", () => {
		expect(placeIcon("cafe", "dining")).toBe("mynaui:coffee");
		expect(placeIcon("restaurant", "dining")).toBe("mynaui:bowl");
		expect(placeIcon("car_repair", "other")).toBe("mynaui:map-pin");
		expect(distance(42.8)).toBe("43 m");
		expect(distance(1240)).toBe("1.2 km");
	});
});
