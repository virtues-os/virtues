import { describe, expect, it } from "vitest";
import { parseDayArticle } from "./dayArticle";
import { placeNotes, plainSentence } from "./dayNotes";
import plainSentences from "./plain-sentences.json";

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
	// The server reads a note's sentence the same way before it moves an
	// article to blocks (`block_move::plain_sentence`), and its test holds it
	// to these answers too: a looser reading there misses a note this page
	// would lose.
	it("gives the answers the server is held to", () => {
		for (const { markdown, plain } of plainSentences) expect(plainSentence(markdown), markdown).toBe(plain);
	});

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

/**
 * One day page as its writer wrote it and as its block page exports it: the
 * export escapes what would read as markup, and a note's anchor lands on the
 * same sentence in either.
 */
const WRITTEN = `A night of R&B at [⟦Ruth's⟧](/place/p_1).[^ev-1]

## Late set[^cx-1]

About ~20 people stayed for user_name's encore.[^ev-2] [⟦Nick⟧](/person/p_2) sang *one* more.[^ev-3]

[^cx-1]: 9:00–11:40 PM
[^ev-1]: Message · 9:02 PM · data_communication_message:m_1
[^ev-2]: Recording · 10:15 PM · data_communication_transcription:t_1
[^ev-3]: Recording · 11:30 PM · data_communication_transcription:t_2
`;

const EXPORTED = `A night of R\\&B at [⟦Ruth's⟧](/place/p_1).[^ev-1]

## Late set[^cx-1]

About \\~20 people stayed for user\\_name's encore.[^ev-2] [⟦Nick⟧](/person/p_2) sang *one* more.[^ev-3]

[^ev-1]: Message · 9:02 PM · data_communication_message:m_1
[^cx-1]: 9:00–11:40 PM
[^ev-2]: Recording · 10:15 PM · data_communication_transcription:t_1
[^ev-3]: Recording · 11:30 PM · data_communication_transcription:t_2
`;

describe("a note on a page that became blocks", () => {
	const notes = [
		{ anchor: { quote: "About ~20 people stayed for user_name's encore.", sentence: 0 } },
		{ anchor: { quote: "Nick sang one more.", sentence: 1 } },
		{ anchor: { quote: "A sentence nobody wrote." } },
	];

	it("lands on the same sentence in the writer's markdown and in the export", () => {
		const written = placeNotes(parseDayArticle(WRITTEN).blocks, notes);
		const exported = placeNotes(parseDayArticle(EXPORTED).blocks, notes);
		expect(exported.map((p) => [p.at, p.lost])).toEqual(written.map((p) => [p.at, p.lost]));
		expect(written.map((p) => p.at)).toEqual([{ block: 1, sentence: 0 }, { block: 1, sentence: 1 }, null]);
	});

	it("keeps the words of an escaped sentence as its reader sees them", () => {
		const s = parseDayArticle(EXPORTED).blocks[1].sentences[0].markdown;
		expect(plainSentence(s)).toBe("About ~20 people stayed for user_name's encore.");
	});
});
