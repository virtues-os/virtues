import { describe, expect, it } from "vitest";
import { abstractOf, parseDayArticle, veilMarks } from "./dayArticle";

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

	it("splits a paragraph into sentences, each with the evidence that closes it", () => {
		const a = parseDayArticle(PAGE);
		expect(a.blocks[1].sentences).toEqual([
			{
				markdown: "You crossed Fifth and Main.",
				evidence: [{ kind: "ev", label: "Recording · 5:14 PM", ref: "data_communication_transcription:tr_a" }],
				space: false,
			},
			{
				markdown: "You got home.",
				evidence: [{ kind: "ev", label: "Message · 7:40 PM", ref: "data_communication_message:msg_1" }],
				space: true,
			},
		]);
		const lead = a.blocks.find((b) => b.markdown === "A few of the questions:");
		expect(lead?.sentences).toEqual([{ markdown: "A few of the questions:", evidence: [], space: false }]);
		expect(a.blocks.find((b) => b.kind === "table")?.sentences).toEqual([]);
	});

	it("keeps links and veil marks inside a sentence, and joins a marker run", () => {
		const a = parseDayArticle(
			"Lede.\n\nYou met [⟦Nick⟧](/person/p) at ⟦the clinic⟧.[^ev-1][^ev-2] Then\nhome. [^cx-1]\n\n" +
				"[^ev-1]: Message · 9:00 AM · data_communication_message:m1\n" +
				"[^ev-2]: Recording · 9:05 AM · data_communication_transcription:t1\n" +
				"[^cx-1]: 97° at 5 PM",
		);
		const [first, second] = a.blocks[0].sentences;
		expect(first.markdown).toBe("You met [⟦Nick⟧](/person/p) at ⟦the clinic⟧.");
		expect(first.evidence.map((n) => n.ref)).toEqual([
			"data_communication_message:m1",
			"data_communication_transcription:t1",
		]);
		expect(second).toEqual({ markdown: "Then home.", evidence: [], space: true });
		expect(a.blocks[0].notes.map((n) => n.kind)).toEqual(["ev", "ev", "cx"]);
	});

	it("splits off a sentence with no marker, so it never borrows the next one's evidence", () => {
		const a = parseDayArticle(
			"Lede.\n\nYou added this yourself. Mass at St. John was in Latin.[^ev-1]\n\n" +
				"[^ev-1]: Recording · 1:02 PM · data_communication_transcription:t1",
		);
		const [mine, sourced] = a.blocks[0].sentences;
		expect(mine).toEqual({ markdown: "You added this yourself.", evidence: [], space: false });
		expect(sourced.markdown).toBe("Mass at St. John was in Latin.");
		expect(sourced.evidence.map((n) => n.ref)).toEqual(["data_communication_transcription:t1"]);
	});

	it("never splits inside a link, a veil mark or an initial", () => {
		const a = parseDayArticle(
			"Lede.\n\nYou met [⟦David O. Okafor⟧](/person/p) at ⟦P.F. Chang's⟧ by Mt. Bonnell.[^ev-1] You ate. Nick called.[^ev-2]\n\n" +
				"[^ev-1]: Message · 9:00 AM · data_communication_message:m1\n" +
				"[^ev-2]: Message · 9:30 AM · data_communication_message:m2",
		);
		expect(a.blocks[0].sentences.map((s) => s.markdown)).toEqual([
			"You met [⟦David O. Okafor⟧](/person/p) at ⟦P.F. Chang's⟧ by Mt. Bonnell.",
			"You ate.",
			"Nick called.",
		]);
		expect(a.blocks[0].sentences.map((s) => s.evidence.length)).toEqual([1, 0, 1]);
	});

	it("keeps the paragraph's own spacing around a marker", () => {
		const a = parseDayArticle("Lede.\n\nShe said yes[^cx-1], then left.\n\n[^cx-1]: 5:00 PM");
		expect(a.blocks[0].sentences.map((s) => [s.markdown, s.space])).toEqual([
			["She said yes", false],
			[", then left.", false],
		]);
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

describe("veilMarks", () => {
	it("removes the marks and keeps what they held", () => {
		const v = veilMarks("You met [⟦Nick⟧](/person/p) at ⟦the clinic⟧ and talked about ⟦an old injury⟧.");
		expect(v.markdown).toBe("You met [Nick](/person/p) at the clinic and talked about an old injury.");
		expect(v.phrases).toEqual(["Nick", "the clinic", "an old injury"]);
	});

	it("leaves an unmarked page alone", () => {
		expect(veilMarks("A quiet day.")).toEqual({ markdown: "A quiet day.", phrases: [] });
	});
});
