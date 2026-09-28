import { describe, expect, it } from "vitest";
import { abstractOf, parseDayArticle } from "./dayArticle";

const PAGE = `The day was [Nick](/person/person_n): a walk and a long talk.

## The walk home[^cx-1]

You crossed Fifth and Main.[^ev-1] You got home.[^ev-2]

A few of the questions:

| Question | Answer |
|---|---|
| Music or movies? | Movies |

[^ev-3]

[^cx-1]: 5:14–7:40 PM
[^ev-1]: Recording · 5:14 PM · data_communication_transcription:tr_a
[^ev-2]: Message · 7:40 PM · data_communication_message:msg_1
[^ev-3]: Recording · 5:39 PM · data_communication_transcription:tr_b
`;

describe("parseDayArticle", () => {
	it("takes the first paragraph as the Abstract", () => {
		const a = parseDayArticle(PAGE);
		expect(a.abstract).toBe("The day was [Nick](/person/person_n): a walk and a long talk.");
		expect(a.blocks[0]).toMatchObject({ kind: "heading", markdown: "## The walk home" });
	});

	it("puts context and evidence notes beside their blocks", () => {
		const a = parseDayArticle(PAGE);
		expect(a.blocks[0].notes).toEqual([{ kind: "cx", label: "5:14–7:40 PM", ref: null }]);
		expect(a.blocks[1].markdown).toBe("You crossed Fifth and Main. You got home.");
		expect(a.blocks[1].notes.map((n) => n.label)).toEqual(["Recording · 5:14 PM", "Message · 7:40 PM"]);
		expect(a.blocks[1].notes[1].ref).toBe("data_communication_message:msg_1");
	});

	it("gives a table the evidence marker on the line after it", () => {
		const a = parseDayArticle(PAGE);
		const table = a.blocks.find((b) => b.kind === "table");
		expect(table?.notes).toEqual([
			{ kind: "ev", label: "Recording · 5:39 PM", ref: "data_communication_transcription:tr_b" },
		]);
		expect(a.blocks.some((b) => b.markdown === "")).toBe(false);
	});

	it("reads a page written before footnotes as a lede and a body", () => {
		const a = parseDayArticle("A quiet day.\n\n## Morning\n\nCoffee.");
		expect(a.abstract).toBe("A quiet day.");
		expect(a.blocks.map((b) => b.kind)).toEqual(["heading", "paragraph"]);
		expect(a.blocks.every((b) => b.notes.length === 0)).toBe(true);
	});

	it("strips links from another day's Abstract for a card", () => {
		expect(abstractOf(PAGE)).toBe("The day was Nick: a walk and a long talk.");
		expect(abstractOf(null)).toBe("");
	});
});
