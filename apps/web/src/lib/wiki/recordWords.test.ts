import { describe, expect, it } from "vitest";
import { contentWords, markRanges, messageBody, messageSender, nearTurns, sharedRanges, stem } from "./recordWords";

const CHUNK =
	"[Speaker 1]: Let's get going. [Speaker 2]: How was Mass? [Speaker 1]: It was great, all in Latin. [Speaker 2]: Yeah. [Speaker 1]: We walked around after. [Speaker 2]: Okay.";

describe("nearTurns", () => {
	it("centers the window on the turn sharing the sentence's words", () => {
		const n = nearTurns(CHUNK, "Mass was called great, all in Latin.");
		expect(n.matched).toBe(true);
		expect(n.turns.map((t) => t.text)).toEqual(["How was Mass?", "It was great, all in Latin.", "Yeah.", "We walked around after."]);
		expect(n.turns.filter((t) => t.near).map((t) => t.text)).toEqual(["It was great, all in Latin."]);
	});

	it("shows the opening turns, unmarked, when nothing overlaps", () => {
		const n = nearTurns(CHUNK, "The flight moved to 5:52.");
		expect(n.matched).toBe(false);
		expect(n.turns[0].text).toBe("Let's get going.");
		expect(n.turns.some((t) => t.near)).toBe(false);
	});
});

describe("messages", () => {
	it("names the sender and the group", () => {
		expect(messageSender({ metadata: { is_from_me: true } })).toBe("You");
		expect(messageSender({ metadata: { is_from_me: false, group_title: "Family" }, from_name: "Nick", is_group_message: true })).toBe("Nick, in Family");
		expect(messageSender({ metadata: {} })).toBe("Someone");
	});

	it("trims a long body and drops attachment placeholders", () => {
		expect(messageBody({ body: "Look￼ here" })).toBe("Look here");
		expect(messageBody({ body: "x".repeat(500) }).length).toBe(421);
	});
});

describe("stem", () => {
	it("brings -s, -es, -ed and -ing to one stem", () => {
		for (const group of [
			["walk", "walks", "walked", "walking"],
			["love", "loves", "loved", "loving"],
			["stop", "stops", "stopped", "stopping"],
			["box", "boxes"],
			["use", "uses", "used", "using"],
		]) {
			expect(new Set(group.map(stem)), group.join(" ")).toEqual(new Set([stem(group[0])]));
		}
	});

	it("leaves words that only look inflected", () => {
		expect(stem("Mass")).toBe("mass");
		expect(stem("bus")).toBe("bus");
		expect(stem("need")).toBe("need");
		expect(stem("called")).toBe("call");
	});
});

describe("sharedRanges", () => {
	const words = (text: string, sentence: string) => sharedRanges(text, sentence).map(([s, e]) => text.slice(s, e));

	it("marks the record's words the sentence shares, case and punctuation aside", () => {
		expect(words("Okay, we'll just get a cab there, I think.", "You took a cab.")).toEqual(["cab"]);
	});

	it("joins shared words across up to two stopwords", () => {
		const text = "I am in a cab in Boston with Nick. We just got out of church.";
		expect(words(text, "David said you were in a cab in Boston, just out of church.")).toEqual(["cab in Boston", "church"]);
		expect(words("It was great, all in Latin.", "Mass was called great, all in Latin.")).toEqual(["great, all in Latin"]);
	});

	it("never joins across a word the sentence doesn't have", () => {
		expect(words("The flight to Denver moved", "The flight moved.")).toEqual(["flight", "moved"]);
	});

	it("matches inflections through the stem", () => {
		expect(words("We walked around after.", "You walk around.")).toEqual(["walked around"]);
	});

	it("shares nothing on stopwords alone", () => {
		expect(sharedRanges("You get home safe?", "She said it was there when you were.")).toEqual([]);
		expect(contentWords("She said it was there when you were.")).toEqual(new Set());
	});

	it("reads the sentence's markdown as words: link text, no veil marks", () => {
		expect(words("Dinner with Nick at eight.", "You met [⟦Nick⟧](/person/p) for **dinner**.")).toEqual(["Dinner with Nick"]);
	});
});

describe("markRanges", () => {
	it("cuts the text into marked and unmarked pieces, in order", () => {
		expect(markRanges("ab cd ef", [[3, 5]])).toEqual([
			{ text: "ab ", marked: false },
			{ text: "cd", marked: true },
			{ text: " ef", marked: false },
		]);
		expect(markRanges("ab", [])).toEqual([{ text: "ab", marked: false }]);
	});
});
