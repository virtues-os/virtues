import { describe, expect, it } from "vitest";
import { contentWords, markRanges, messageBody, messageSender, nearTurns, sharedRanges, stem } from "./recordWords";

const CHUNK =
	"[Speaker 1]: Coats on, then. [Speaker 2]: How was the sailing lesson? [Speaker 1]: Windy, mostly knots and ropes. [Speaker 2]: Nice. [Speaker 1]: We coiled the lines after. [Speaker 2]: Okay.";

describe("nearTurns", () => {
	it("centers the window on the turn sharing the sentence's words", () => {
		const n = nearTurns(CHUNK, "The lesson was windy, mostly knots and ropes.");
		expect(n.matched).toBe(true);
		expect(n.turns.map((t) => t.text)).toEqual(["How was the sailing lesson?", "Windy, mostly knots and ropes.", "Nice.", "We coiled the lines after."]);
		expect(n.turns.filter((t) => t.near).map((t) => t.text)).toEqual(["Windy, mostly knots and ropes."]);
	});

	it("shows the opening turns, unmarked, when nothing overlaps", () => {
		const n = nearTurns(CHUNK, "The bus came at 4:10.");
		expect(n.matched).toBe(false);
		expect(n.turns[0].text).toBe("Coats on, then.");
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

describe("contentWords", () => {
	it("reads a sentence's export as its markdown: escapes go with what they escape", () => {
		const written = "Lunch at [⟦Marlowe's⟧](/place/p) with Q&A and ~20 people from user_name.";
		const exported = "Lunch at [⟦Marlowe\\'s⟧](/place/p) with Q\\&A and \\~20 people from user\\_name.";
		expect(contentWords(exported)).toEqual(contentWords(written));
		expect(sharedRanges("We stayed for Q&A at lunch", exported)).toEqual(sharedRanges("We stayed for Q&A at lunch", written));
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
		expect(stem("Glass")).toBe("glass");
		expect(stem("bus")).toBe("bus");
		expect(stem("need")).toBe("need");
		expect(stem("called")).toBe("call");
	});
});

describe("sharedRanges", () => {
	const words = (text: string, sentence: string) => sharedRanges(text, sentence).map(([s, e]) => text.slice(s, e));

	it("marks the record's words the sentence shares, case and punctuation aside", () => {
		expect(words("Okay, we'll just get a stepladder there, I think.", "You borrowed a stepladder.")).toEqual(["stepladder"]);
	});

	it("joins shared words across up to two stopwords", () => {
		const text = "I am on a tram to Lakeport with Nick. We just got out of the gallery.";
		expect(words(text, "David said you were on a tram to Lakeport, just out of the gallery.")).toEqual(["tram to Lakeport", "gallery"]);
		expect(words("Windy, mostly knots and ropes.", "The lesson was windy, mostly knots and ropes.")).toEqual(["Windy, mostly knots and ropes"]);
	});

	it("never joins across a word the sentence doesn't have", () => {
		expect(words("The bus to Lakeport moved", "The bus moved.")).toEqual(["bus", "moved"]);
	});

	it("matches inflections through the stem", () => {
		expect(words("We painted the fence after.", "You paint the fence.")).toEqual(["painted the fence"]);
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
