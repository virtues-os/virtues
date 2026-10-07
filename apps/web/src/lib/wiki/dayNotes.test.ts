import { describe, expect, it } from "vitest";
import { parseDayArticle } from "./dayArticle";
import { placeNotes, plainSentence } from "./dayNotes";

const PAGE = `Lede.

## Morning

You met [⟦Nick⟧](/person/p) at ⟦the clinic⟧.[^ev-1] You got home.[^ev-2]

## Evening

You got home.[^ev-3] It rained.

[^ev-1]: Message · 9:00 AM · data_communication_message:m1
[^ev-2]: Message · 9:30 AM · data_communication_message:m2
[^ev-3]: Message · 7:00 PM · data_communication_message:m3
`;

describe("plainSentence", () => {
	it("keeps the words and drops the markup", () => {
		expect(plainSentence("You met [⟦Nick⟧](/person/p) at ⟦the clinic⟧.")).toBe("You met Nick at the clinic.");
	});
});

describe("placeNotes", () => {
	const blocks = parseDayArticle(PAGE).blocks;

	it("puts a note beside the sentence whose words it kept", () => {
		const [p] = placeNotes(blocks, [{ anchor: { quote: "You met Nick at the clinic.", sentence: 0 } }]);
		expect(p.at).toEqual({ block: 1, sentence: 0 });
		expect(p.lost).toBeNull();
	});

	it("prefers the same index when the same words appear twice", () => {
		const [p] = placeNotes(blocks, [{ anchor: { quote: "You got home.", sentence: 0 } }]);
		expect(p.at).toEqual({ block: 3, sentence: 0 });
		const [q] = placeNotes(blocks, [{ anchor: { quote: "You got home.", sentence: 1 } }]);
		expect(q.at).toEqual({ block: 1, sentence: 1 });
	});

	it("sits a note beside the Abstract when its words are gone, and says what it was about", () => {
		const [p] = placeNotes(blocks, [{ anchor: { quote: "You walked to the river." } }]);
		expect(p).toMatchObject({ at: null, lost: "You walked to the river." });
	});

	it("sits a note with no passage beside the Abstract", () => {
		const [p] = placeNotes(blocks, [{ anchor: null }, {}]);
		expect(p).toMatchObject({ at: null, lost: null });
	});
});
