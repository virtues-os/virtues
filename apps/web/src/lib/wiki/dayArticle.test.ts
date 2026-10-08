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

describe("figures", () => {
	it("reads a fenced figure block as its fields, keys lowercase, split at the first colon", () => {
		const a = parseDayArticle(
			"Lede.\n\nYou ate.[^ev-1]\n\n```figure\nkind: quote\nText: We've all been to the Vatican.\nwho: In a recording\nref: data_communication_transcription:t1\ntime: 1:17 PM\n```\n\n" +
				"[^ev-1]: Recording · 1:02 PM · data_communication_transcription:t1",
		);
		const fig = a.blocks[1];
		expect(fig.kind).toBe("figure");
		expect(fig.sentences).toEqual([]);
		expect(fig.fields).toEqual({
			kind: "quote",
			text: "We've all been to the Vatican.",
			who: "In a recording",
			ref: "data_communication_transcription:t1",
			time: "1:17 PM",
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
			"Nothing was recorded between 00:25 and 10:22.",
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
		expect(clockMinutes(at("2026-04-18T15:22:00Z"), ZONE, end)).toBe(10 * 60 + 22);
		expect(clockMinutes(end, ZONE, end)).toBe(1440);
	});

	it("writes a range with one AM/PM when both ends share it", () => {
		expect(timeRange(at("2026-04-18T05:25:00Z"), at("2026-04-18T15:22:00Z"), ZONE)).toBe("12:25–10:22 AM");
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
				["2026-04-18T05:00:00Z", "2026-04-18T05:25:00Z"],
				["2026-04-18T15:22:00Z", "2026-04-18T15:40:00Z"],
				["2026-04-18T16:00:00Z", "2026-04-18T23:12:00Z"],
			],
			start,
			end,
			later,
		);
		expect(gaps.map((g) => timeRange(g.start, g.end, ZONE))).toEqual(["12:25–10:22 AM", "6:12 PM–12:00 AM"]);
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
		"Lede.\n\n## Morning\n\nYou went to Mass.[^ev-1] You took a cab.[^ev-2]\n\nBrunch ran long.[^ev-3]\n\n## Evening\n\nYou flew home.[^ev-4]\n\n" +
			"[^ev-1]: Recording · 10:37 AM · data_communication_transcription:a\n" +
			"[^ev-2]: Recording · 12:22 PM · data_communication_transcription:b\n" +
			"[^ev-3]: Recording · 1:02 PM · data_communication_transcription:c\n" +
			"[^ev-4]: Recording · 9:24 PM · data_communication_transcription:d",
	);
	// Blocks: 0 "## Morning", 1 Mass and the cab, 2 brunch, 3 "## Evening", 4 the flight.
	const [morning, brunch, evening] = [1, 2, 4];
	const first = (atMin: number, rank = 0, title: string | null = null): FirstMark => ({
		at: atMin,
		title,
		line: "First visit in your record",
		veil: [],
		rank,
	});

	it("puts a gap beside the first paragraph whose evidence starts at or after it ends", () => {
		const m = placeMarks(page.blocks, [
			{ end: 10 * 60 + 22, label: "12:25–10:22 AM" },
			{ end: 21 * 60 + 24, label: "6:12–9:24 PM" },
		], []);
		expect(m.get(morning)).toEqual([{ kind: "gap", spans: ["12:25–10:22 AM"] }]);
		expect(m.get(evening)).toEqual([{ kind: "gap", spans: ["6:12–9:24 PM"] }]);
	});

	it("merges gaps that land on one paragraph, and sends a late gap to the last", () => {
		const m = placeMarks(page.blocks, [
			{ end: 12 * 60 + 40, label: "12:30–12:40 PM" },
			{ end: 12 * 60 + 55, label: "12:45–12:55 PM" },
			{ end: 23 * 60 + 50, label: "10:00–11:50 PM" },
		], []);
		expect(m.get(brunch)).toEqual([{ kind: "gap", spans: ["12:30–12:40 PM", "12:45–12:55 PM"] }]);
		expect(m.get(evening)).toEqual([{ kind: "gap", spans: ["10:00–11:50 PM"] }]);
	});

	it("puts a first beside the paragraph spanning its moment, else the next one", () => {
		const m = placeMarks(page.blocks, [], [first(11 * 60, 0, "St. Paul's Cathedral"), first(15 * 60)]);
		expect(m.get(morning)).toEqual([{ kind: "first", title: "St. Paul's Cathedral", line: "First visit in your record", veil: [] }]);
		expect(m.get(evening)?.[0]).toMatchObject({ kind: "first", title: null });
	});

	it("shows one first to a paragraph and three to a page, the lowest rank first", () => {
		const m = placeMarks(page.blocks, [], [
			first(10 * 60 + 40, 1),
			first(10 * 60 + 45, 0, "Named"),
			first(13 * 60, 1),
			first(21 * 60, 1),
			first(21 * 60 + 5, 1),
		]);
		expect(m.get(morning)).toEqual([{ kind: "first", title: "Named", line: "First visit in your record", veil: [] }]);
		expect([...m.values()].flat()).toHaveLength(3);
	});

	it("says an unnamed first visit once a section", () => {
		const m = placeMarks(page.blocks, [], [first(10 * 60 + 40), first(13 * 60), first(21 * 60)]);
		expect(m.get(morning)?.[0]).toMatchObject({ kind: "first", title: null });
		expect(m.get(brunch)).toBeUndefined();
		expect(m.get(evening)?.[0]).toMatchObject({ kind: "first", title: null });
	});

	it("has nowhere to go without a paragraph", () => {
		expect(placeMarks(parseDayArticle("Lede.\n\n## Only a heading").blocks, [{ end: 60, label: "x" }], []).size).toBe(0);
	});
});
