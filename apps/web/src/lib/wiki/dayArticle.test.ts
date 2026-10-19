import { describe, expect, it } from "vitest";
import {
	abstractOf,
	clockMinutes,
	dayStartIn,
	figureRefs,
	parseDayArticle,
	placeMarks,
	recordingGaps,
	timeRange,
	veilMarks,
	type FirstMark,
} from "./dayArticle";

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
			"Lede.\n\nYou added this yourself. The ferry from St. Quillon left late.[^ev-1]\n\n" +
				"[^ev-1]: Recording · 3:48 PM · data_communication_transcription:t1",
		);
		const [mine, sourced] = a.blocks[0].sentences;
		expect(mine).toEqual({ markdown: "You added this yourself.", evidence: [], space: false });
		expect(sourced.markdown).toBe("The ferry from St. Quillon left late.");
		expect(sourced.evidence.map((n) => n.ref)).toEqual(["data_communication_transcription:t1"]);
	});

	it("never splits inside a link, a veil mark or an initial", () => {
		const a = parseDayArticle(
			"Lede.\n\nYou met [⟦David O. Okafor⟧](/person/p) at ⟦P.F. Chang's⟧ by Mt. Hesper.[^ev-1] You ate. Nick called.[^ev-2]\n\n" +
				"[^ev-1]: Message · 9:00 AM · data_communication_message:m1\n" +
				"[^ev-2]: Message · 9:30 AM · data_communication_message:m2",
		);
		expect(a.blocks[0].sentences.map((s) => s.markdown)).toEqual([
			"You met [⟦David O. Okafor⟧](/person/p) at ⟦P.F. Chang's⟧ by Mt. Hesper.",
			"You ate.",
			"Nick called.",
		]);
		expect(a.blocks[0].sentences.map((s) => s.evidence.length)).toEqual([1, 0, 1]);
	});

	it("reads an escaped bracket or star as text: it opens nothing and a sentence may start with one", () => {
		const a = parseDayArticle(
			"Lede.\n\nYou paid \\[about\\] \\[20. The rest waited.[^ev-1] It rained. \\*Twenty\\* came.\n\n" +
				"[^ev-1]: Message · 9:00 AM · data_communication_message:m1",
		);
		expect(a.blocks[0].sentences.map((s) => s.markdown)).toEqual([
			"You paid \\[about\\] \\[20.",
			"The rest waited.",
			"It rained.",
			"\\*Twenty\\* came.",
		]);
		expect(a.blocks[0].sentences.map((s) => s.evidence.length)).toEqual([0, 1, 0, 0]);
	});

	it("reads another day's Abstract as plain words, its escapes read", () => {
		expect(abstractOf("A night of Q\\&A with [⟦Nick⟧](/person/p) and \\~20 others.[^ev-1]\n\n[^ev-1]: Message · 9:00 PM · data_communication_message:m1")).toBe(
			"A night of Q&A with Nick and ~20 others.",
		);
	});

	it("ends a heading at its line, as its export parts it from the paragraph under it", () => {
		const a = parseDayArticle("Lede.\n\n## Morning[^cx-1]\nYou walked.[^ev-1] You ate.\n\n[^cx-1]: 8:00–9:00 AM\n[^ev-1]: Message · 8:10 AM · data_communication_message:m1");
		expect(a.blocks.map((b) => b.kind)).toEqual(["heading", "paragraph"]);
		expect(a.blocks[0].notes.map((n) => n.label)).toEqual(["8:00–9:00 AM"]);
		expect(a.blocks[1].sentences.map((s) => [s.markdown, s.evidence.length])).toEqual([
			["You walked.", 1],
			["You ate.", 0],
		]);
	});

	it("reads an image alone on its line as a block, and a name written as one as a name", () => {
		const a = parseDayArticle("Lede.\n\n![The harbour](/api/drive/files/f_1/download)\nThe boats came in.[^ev-1]\n\n![@Nick](/person/p)\nwaved.\n\n[^ev-1]: Recording · 9:00 AM · data_communication_transcription:t1");
		expect(a.blocks.map((b) => b.markdown)).toEqual([
			"![The harbour](/api/drive/files/f_1/download)",
			"The boats came in.",
			"![@Nick](/person/p)\nwaved.",
		]);
		expect(a.blocks[1].sentences[0]).toMatchObject({ markdown: "The boats came in.", evidence: [{ ref: "data_communication_transcription:t1" }] });
	});

	it("reads rows with no rule under the first as a paragraph of pipes, as its blocks hold them", () => {
		const rows = parseDayArticle("Lede.\n\n| Question | Answer |\n| Music or movies? | Movies |");
		expect(rows.blocks.map((b) => b.kind)).toEqual(["paragraph"]);
		const table = parseDayArticle("Lede.\n\n| Question | Answer |\n| :--- | ---: |\n| Music or movies? | Movies |");
		expect(table.blocks.map((b) => b.kind)).toEqual(["table"]);
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

	it("gives each phrase as the page draws it: in markdown with its escapes read, in a field as written", () => {
		expect(veilMarks("You called ⟦AT\\&T⟧ about ⟦the \\[old\\] clinic⟧.").phrases).toEqual(["AT&T", "the [old] clinic"]);
		expect(veilMarks("A night at ⟦AT\\&T Stadium⟧.", "text").phrases).toEqual(["AT\\&T Stadium"]);
	});
});

describe("a hard line break", () => {
	// A block page's export writes one as a backslash ending its line, where
	// the writer wrote two spaces.
	it("parts two sentences, the backslash read, and a source before it stays with the first", () => {
		const ev = "[^ev-2]: Recording · 7:10 PM\n";
		const page = parseDayArticle(`Lead.\n\nYou cooked dinner.[^ev-2]\\\nThen you read a book. It was late.\n\n${ev}`);
		const said = page.blocks[0].sentences.map((s) => [s.markdown, s.evidence.length]);
		expect(said).toEqual([
			["You cooked dinner.", 1],
			["Then you read a book.", 0],
			["It was late.", 0],
		]);
		const bare = parseDayArticle("Lead.\n\nYou cooked dinner.\\\nThen you read a book. It was late.\n");
		expect(bare.blocks[0].sentences.map((s) => s.markdown)).toEqual(["You cooked dinner.", "Then you read a book.", "It was late."]);
	});

	it("leaves an escaped backslash at a line's end a backslash", () => {
		const page = parseDayArticle("Lead.\n\nThe path was C:\\\\\nthen home.\n");
		expect(page.blocks[0].sentences.map((s) => s.markdown)).toEqual(["The path was C:\\\\ then home."]);
	});
});

describe("a list numbered with parentheses", () => {
	it("is a list, as the export writes a second list beside one of its kind", () => {
		const page = parseDayArticle("Lead.\n\n1. one\n\n1) two\n");
		expect(page.blocks.map((b) => b.kind)).toEqual(["other", "other"]);
	});
});

describe("figures", () => {
	it("reads a fenced figure block as its fields, keys lowercase, split at the first colon", () => {
		const a = parseDayArticle(
			"Lede.\n\nYou ate.[^ev-1]\n\n```figure\nkind: quote\nText: The bread was still warm.\nwho: In a recording\nref: data_communication_transcription:t1\ntime: 4:05 PM\n```\n\n" +
				"[^ev-1]: Recording · 3:48 PM · data_communication_transcription:t1",
		);
		const fig = a.blocks[1];
		expect(fig.kind).toBe("figure");
		expect(fig.sentences).toEqual([]);
		expect(fig.fields).toEqual({
			kind: "quote",
			text: "The bread was still warm.",
			who: "In a recording",
			ref: "data_communication_transcription:t1",
			time: "4:05 PM",
		});
	});

	it("keeps a figure whole across a blank line inside its fence", () => {
		const a = parseDayArticle("Lede.\n\n```figure\nkind: thread\n\nrefs: data_communication_message:a, data_communication_message:b\n```\n\nAfter.");
		expect(a.blocks.map((b) => b.kind)).toEqual(["figure", "paragraph"]);
		expect(a.blocks[0].fields).toEqual({ kind: "thread", refs: "data_communication_message:a, data_communication_message:b" });
	});

	it("names a figure's records: a thread's refs, any other's ref", () => {
		expect(figureRefs({ kind: "thread", refs: "data_communication_message:a, data_communication_message:b" })).toEqual([
			"data_communication_message:a",
			"data_communication_message:b",
		]);
		expect(figureRefs({ kind: "quote", ref: "data_communication_transcription:t1" })).toEqual(["data_communication_transcription:t1"]);
		expect(figureRefs({ kind: "route" })).toEqual([]);
	});

	it("leaves any other fence as it was", () => {
		const a = parseDayArticle("Lede.\n\n```\ncode: here\n```");
		expect(a.blocks[0]).toMatchObject({ kind: "other" });
		expect(a.blocks[0].fields).toBeUndefined();
	});
});

describe("gap prose", () => {
	it("drops a paragraph that only says what wasn't recorded", () => {
		for (const line of [
			"Nothing was recorded between 7:41 and 12:39.",
			"Nothing was recorded between 00:40 and 9:17.",
			"Nothing was recorded between 8:41 and 5.",
			"Nothing was recorded before 12:02 a.m.",
			"Nothing was recorded before 9:34.",
			"Nothing was recorded after 21:10.",
		]) {
			const a = parseDayArticle(`Lede.\n\n## Morning\n\n${line}\n\nYou woke.`);
			expect(a.blocks.map((b) => b.markdown), line).toEqual(["## Morning", "You woke."]);
		}
	});

	it("keeps a sentence that says more than that", () => {
		const a = parseDayArticle("Lede.\n\nNothing was recorded before 9:34, when you woke.");
		expect(a.blocks).toHaveLength(1);
	});
});

const ZONE = "America/Chicago";
const at = (iso: string) => Date.parse(iso);

describe("the day's clock", () => {
	it("starts the day at local midnight, a daylight-saving day included", () => {
		expect(new Date(dayStartIn("2026-04-18", ZONE)).toISOString()).toBe("2026-04-18T05:00:00.000Z");
		expect(new Date(dayStartIn("2026-03-08", ZONE)).toISOString()).toBe("2026-03-08T06:00:00.000Z");
		expect(new Date(dayStartIn("2026-11-01", ZONE)).toISOString()).toBe("2026-11-01T05:00:00.000Z");
	});

	it("reads minutes on the local clock, and the next midnight as 24:00", () => {
		const end = dayStartIn("2026-04-19", ZONE);
		expect(clockMinutes(at("2026-04-18T14:17:00Z"), ZONE, end)).toBe(9 * 60 + 17);
		expect(clockMinutes(end, ZONE, end)).toBe(1440);
	});

	it("writes a range with one AM/PM when both ends share it", () => {
		expect(timeRange(at("2026-04-18T05:40:00Z"), at("2026-04-18T14:17:00Z"), ZONE)).toBe("12:40–9:17 AM");
		expect(timeRange(at("2026-04-18T16:20:00Z"), at("2026-04-18T18:05:00Z"), ZONE)).toBe("11:20 AM–1:05 PM");
	});
});

describe("recordingGaps", () => {
	const start = dayStartIn("2026-04-18", ZONE);
	const end = dayStartIn("2026-04-19", ZONE);
	const later = end + 86_400_000;

	it("finds every silence of half an hour or more, the day's edges included", () => {
		const gaps = recordingGaps(
			[
				["2026-04-18T05:00:00Z", "2026-04-18T05:40:00Z"],
				["2026-04-18T14:17:00Z", "2026-04-18T14:40:00Z"],
				["2026-04-18T15:00:00Z", "2026-04-18T23:48:00Z"],
			],
			start,
			end,
			later,
		);
		expect(gaps.map((g) => timeRange(g.start, g.end, ZONE))).toEqual(["12:40–9:17 AM", "6:48 PM–12:00 AM"]);
	});

	it("stops at now on a day still going, and says nothing without coverage", () => {
		const now = at("2026-04-18T20:00:00Z");
		const gaps = recordingGaps([["2026-04-18T05:00:00Z", "2026-04-18T19:45:00Z"]], start, end, now);
		expect(gaps).toEqual([]);
		expect(recordingGaps([], start, end, later)).toEqual([]);
		expect(recordingGaps(null, start, end, later)).toEqual([]);
	});
});

describe("placeMarks", () => {
	const page = parseDayArticle(
		"Lede.\n\n## Morning\n\nYou repotted the ferns.[^ev-1] You biked to the market.[^ev-2]\n\nThe market was crowded.[^ev-3]\n\n## Evening\n\nYou played board games with Nick.[^ev-4]\n\n" +
			"[^ev-1]: Recording · 8:20 AM · data_communication_transcription:a\n" +
			"[^ev-2]: Recording · 9:52 AM · data_communication_transcription:b\n" +
			"[^ev-3]: Recording · 11:50 AM · data_communication_transcription:c\n" +
			"[^ev-4]: Recording · 7:35 PM · data_communication_transcription:d",
	);
	// Blocks: 0 "## Morning", 1 the ferns and the ride, 2 the market, 3 "## Evening", 4 the games.
	const [morning, market, evening] = [1, 2, 4];
	const first = (atMin: number, rank = 0, title: string | null = null): FirstMark => ({
		at: atMin,
		title,
		line: "First visit in your record",
		veil: [],
		rank,
	});

	it("puts a gap beside the first paragraph whose evidence starts at or after it ends", () => {
		const m = placeMarks(page.blocks, [
			{ end: 7 * 60 + 55, label: "12:10–7:55 AM" },
			{ end: 19 * 60 + 35, label: "4:40–7:35 PM" },
		], []);
		expect(m.get(morning)).toEqual([{ kind: "gap", spans: ["12:10–7:55 AM"] }]);
		expect(m.get(evening)).toEqual([{ kind: "gap", spans: ["4:40–7:35 PM"] }]);
	});

	it("merges gaps that land on one paragraph, and sends a late gap to the last", () => {
		const m = placeMarks(page.blocks, [
			{ end: 10 * 60 + 45, label: "10:30–10:45 AM" },
			{ end: 11 * 60 + 20, label: "11:00–11:20 AM" },
			{ end: 23 * 60 + 30, label: "9:15–11:30 PM" },
		], []);
		expect(m.get(market)).toEqual([{ kind: "gap", spans: ["10:30–10:45 AM", "11:00–11:20 AM"] }]);
		expect(m.get(evening)).toEqual([{ kind: "gap", spans: ["9:15–11:30 PM"] }]);
	});

	it("puts a first beside the paragraph spanning its moment, else the next one", () => {
		const m = placeMarks(page.blocks, [], [first(9 * 60, 0, "The Glass House"), first(14 * 60)]);
		expect(m.get(morning)).toEqual([{ kind: "first", title: "The Glass House", line: "First visit in your record", veil: [] }]);
		expect(m.get(evening)?.[0]).toMatchObject({ kind: "first", title: null });
	});

	it("shows one first to a paragraph and three to a page, the lowest rank first", () => {
		const m = placeMarks(page.blocks, [], [
			first(8 * 60 + 30, 1),
			first(8 * 60 + 40, 0, "Named"),
			first(11 * 60 + 30, 1),
			first(19 * 60, 1),
			first(19 * 60 + 5, 1),
		]);
		expect(m.get(morning)).toEqual([{ kind: "first", title: "Named", line: "First visit in your record", veil: [] }]);
		expect([...m.values()].flat()).toHaveLength(3);
	});

	it("says an unnamed first visit once a section", () => {
		const m = placeMarks(page.blocks, [], [first(8 * 60 + 30), first(11 * 60 + 30), first(19 * 60)]);
		expect(m.get(morning)?.[0]).toMatchObject({ kind: "first", title: null });
		expect(m.get(market)).toBeUndefined();
		expect(m.get(evening)?.[0]).toMatchObject({ kind: "first", title: null });
	});

	it("has nowhere to go without a paragraph", () => {
		expect(placeMarks(parseDayArticle("Lede.\n\n## Only a heading").blocks, [{ end: 60, label: "x" }], []).size).toBe(0);
	});
});
