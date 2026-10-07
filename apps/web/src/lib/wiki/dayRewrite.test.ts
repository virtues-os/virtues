import { describe, expect, it } from "vitest";
import type { DayRewriteStatus } from "./api";
import { parseDayArticle } from "./dayArticle";
import {
	FRESH_MS,
	IDLE,
	PUT_BACK_UNSEEN,
	SHOW_FAILED,
	STOPPED,
	StatusAsks,
	UNKNOWN,
	afterUnansweredStart,
	anchoredNoteCount,
	confirmBody,
	failedCopy,
	keepsAsking,
	movedNoteCount,
	movedNotesCopy,
	putBackCopy,
	readRefusal,
	readStatus,
	type RewriteLine,
} from "./dayRewrite";

const NOW = Date.parse("2026-10-07T12:00:00Z");
const ago = (ms: number) => new Date(NOW - ms).toISOString();

const status = (s: Partial<DayRewriteStatus>): DayRewriteStatus => ({
	state: "idle",
	has_page: true,
	has_your_edits: false,
	...s,
});

const WATCHED = { watched: true };
const UNWATCHED = { watched: false };

describe("readStatus", () => {
	it("shows a running rewrite whether or not this client started it", () => {
		expect(readStatus(status({ state: "running" }), UNWATCHED, NOW)).toEqual({ kind: "running" });
		expect(readStatus(status({ state: "running" }), WATCHED, NOW)).toEqual({ kind: "running" });
	});

	it("shows a finished rewrite this client watched, however long ago it finished", () => {
		const s = status({ state: "done", finished_at: ago(60 * 60_000), before_version: 7, after_version: 8 });
		expect(readStatus(s, WATCHED, NOW)).toEqual({ kind: "done", beforeVersion: 7 });
	});

	it("shows a finished rewrite it didn't watch only while it is fresh, before any start here", () => {
		const recent = status({ state: "done", finished_at: ago(FRESH_MS - 1000), before_version: 3 });
		expect(readStatus(recent, UNWATCHED, NOW)).toEqual({ kind: "done", beforeVersion: 3 });
		const old = status({ state: "done", finished_at: ago(FRESH_MS + 1000), before_version: 3 });
		expect(readStatus(old, UNWATCHED, NOW)).toEqual(IDLE);
		expect(readStatus(status({ state: "done", finished_at: null }), UNWATCHED, NOW)).toEqual(IDLE);
	});

	it("reads a missing before_version as null", () => {
		expect(readStatus(status({ state: "done", finished_at: ago(0) }), WATCHED, NOW)).toEqual({
			kind: "done",
			beforeVersion: null,
		});
	});

	it("shows a failure the same way, by its code", () => {
		const s = status({ state: "failed", code: "thin_draft", finished_at: ago(1000) });
		expect(readStatus(s, UNWATCHED, NOW)).toEqual({ kind: "failed", code: "thin_draft" });
		const stale = status({ state: "failed", code: "thin_draft", finished_at: ago(FRESH_MS * 2) });
		expect(readStatus(stale, UNWATCHED, NOW)).toEqual(IDLE);
		expect(readStatus(stale, WATCHED, NOW)).toEqual({ kind: "failed", code: "thin_draft" });
		expect(readStatus(status({ state: "failed", finished_at: ago(0) }), WATCHED, NOW)).toEqual({
			kind: "failed",
			code: "failed",
		});
	});

	it("says a watched rewrite stopped when the server comes back without it", () => {
		expect(readStatus(status({ state: "idle" }), WATCHED, NOW)).toEqual({ kind: "stopped" });
		expect(readStatus(status({ state: "idle" }), UNWATCHED, NOW)).toEqual(IDLE);
	});
});

describe("readStatus after a start here", () => {
	const S1 = "2026-10-07T11:00:00Z";
	const S2 = "2026-10-07T11:50:00Z";

	it("shows this start's finish however long ago it finished, while its start got no answer", () => {
		// The start's answer was lost, the phone slept, and the rewrite
		// finished long before the next ask got through.
		const late = status({ state: "done", started_at: S2, finished_at: ago(FRESH_MS * 3), before_version: 4 });
		expect(readStatus(late, { watched: false, before: S1 }, NOW)).toEqual({ kind: "done", beforeVersion: 4 });
		const failed = status({ state: "failed", code: "billing", started_at: S2, finished_at: ago(FRESH_MS * 3) });
		expect(readStatus(failed, { watched: false, before: S1 }, NOW)).toEqual({ kind: "failed", code: "billing" });
	});

	it("doesn't take the finish the page read before the start for this start's, however fresh", () => {
		// A rewrite finished a minute ago; the owner taps Rewrite again and
		// that start never reaches your server.
		const earlier = status({ state: "done", started_at: S1, finished_at: ago(60_000), before_version: 4 });
		expect(readStatus(earlier, { watched: false, before: S1 }, NOW)).toEqual(IDLE);
		const failed = status({ state: "failed", code: "thin_draft", started_at: S1, finished_at: ago(1000) });
		expect(readStatus(failed, { watched: false, before: S1 }, NOW)).toEqual(IDLE);
	});

	it("takes the finish of a run this page saw running as news, even when the start before it was that run", () => {
		// A start's answer is lost; a poll then reads the run that start began
		// (S1) as running, the owner taps again and is told it's in progress.
		const s = status({ state: "done", started_at: S1, finished_at: ago(FRESH_MS * 3), before_version: 5 });
		expect(readStatus(s, { watched: true, before: S1 }, NOW)).toEqual({ kind: "done", beforeVersion: 5 });
		const failed = status({ state: "failed", code: "not_enough", started_at: S1, finished_at: ago(1000) });
		expect(readStatus(failed, { watched: true, before: S1 }, NOW)).toEqual({ kind: "failed", code: "not_enough" });
	});

	it("takes any finish as new when the page read nothing running or finished before the start", () => {
		const s = status({ state: "done", started_at: S2, finished_at: ago(FRESH_MS * 3), before_version: 2 });
		expect(readStatus(s, { watched: false, before: null }, NOW)).toEqual({ kind: "done", beforeVersion: 2 });
	});

	it("keeps an idle answer idle while the start's outcome is unknown, and says a watched one stopped", () => {
		expect(readStatus(status({ state: "idle" }), { watched: false, before: S1 }, NOW)).toEqual(IDLE);
		expect(readStatus(status({ state: "idle" }), { watched: true, before: S1 }, NOW)).toEqual({ kind: "stopped" });
	});

	it("shows a running rewrite as running", () => {
		const s = status({ state: "running", started_at: S2 });
		expect(readStatus(s, { watched: false, before: S1 }, NOW)).toEqual({ kind: "running" });
	});
});

describe("afterUnansweredStart", () => {
	it("says the start didn't land only when the answer reads as idle", () => {
		expect(afterUnansweredStart(IDLE)).toEqual({ kind: "failed", code: "failed" });
	});

	it("keeps a running, finished, or failed answer exactly as read", () => {
		const read: RewriteLine[] = [
			{ kind: "running" },
			{ kind: "done", beforeVersion: 4 },
			{ kind: "failed", code: "billing" },
			{ kind: "failed", code: "not_enough" },
		];
		for (const line of read) expect(afterUnansweredStart(line)).toEqual(line);
	});
});

describe("keepsAsking", () => {
	it("asks while a rewrite runs or a start's outcome is unknown, and at no other time", () => {
		const asking: RewriteLine[] = [{ kind: "running" }, { kind: "unknown" }];
		const settled: RewriteLine[] = [
			IDLE,
			{ kind: "stopped" },
			{ kind: "done", beforeVersion: 2 },
			{ kind: "shown", beforeVersion: 2, moved: 0 },
			{ kind: "putBack", message: "Your server put that version back." },
			{ kind: "putBackFailed" },
			{ kind: "failed", code: "failed" },
		];
		for (const l of asking) expect(keepsAsking(l)).toBe(true);
		for (const l of settled) expect(keepsAsking(l)).toBe(false);
	});
});

describe("StatusAsks", () => {
	it("reads an answer about the day it asked for, since the latest start", () => {
		const asks = new StatusAsks();
		const a = asks.send(1)!;
		expect(asks.landed(a, 1)).toEqual({ current: true, again: false });
	});

	it("drops an answer to an ask sent before a start", () => {
		// The page arrives and asks; the owner starts a rewrite while that ask
		// is out. Its answer predates the start, so it can't say how it went.
		const asks = new StatusAsks();
		const before = asks.send(1)!;
		asks.started();
		expect(asks.landed(before, 1).current).toBe(false);
		const after = asks.send(1)!;
		expect(asks.landed(after, 1).current).toBe(true);
	});

	it("keeps one ask out per day, and the one out asks again for the one that waited", () => {
		const asks = new StatusAsks();
		const out = asks.send(1)!;
		asks.started();
		// The poll after the start finds an ask still out.
		expect(asks.send(1)).toBeNull();
		expect(asks.landed(out, 1)).toEqual({ current: false, again: true });
		// Once it lands, the next ask goes out, and nothing waits behind it.
		const next = asks.send(1)!;
		expect(asks.landed(next, 1)).toEqual({ current: true, again: false });
	});

	it("drops an answer to an ask sent before a refused start, so it can't erase the refusal", () => {
		// The page's arrival ask is out on a slow link; the start comes back
		// busy and the page says so. The arrival answer lands after that.
		const asks = new StatusAsks();
		const arrival = asks.send(1)!;
		asks.started();
		expect(asks.landed(arrival, 1).current).toBe(false);
	});

	it("drops an answer for a day the page has left, without disturbing the new day's ask", () => {
		const asks = new StatusAsks();
		const oldDay = asks.send(1)!;
		const newDay = asks.send(2)!;
		expect(asks.send(2)).toBeNull();
		expect(asks.landed(oldDay, 2)).toEqual({ current: false, again: false });
		// The new day's ask is still the one out, and still owes the one that waited.
		expect(asks.send(2)).toBeNull();
		expect(asks.landed(newDay, 2)).toEqual({ current: true, again: true });
	});
});

describe("readRefusal", () => {
	it("asks again with the edits line on needs_consent", () => {
		expect(readRefusal(409, "needs_consent")).toEqual({ kind: "consent" });
	});

	it("treats a rewrite another device started as running", () => {
		expect(readRefusal(409, "rewrite_in_progress")).toEqual({ kind: "running" });
	});

	it("doesn't watch when another writer holds the day", () => {
		expect(readRefusal(409, "busy")).toEqual({ kind: "failed", code: "busy" });
	});

	it("asks the server before saying anything when no answer came back", () => {
		expect(readRefusal(null, undefined)).toEqual({ kind: "ask" });
		// A gateway in front of the server answered, not the server.
		for (const status of [502, 503, 504]) expect(readRefusal(status, "Bad Gateway")).toEqual({ kind: "ask" });
	});

	it("maps the other refusals to failure codes", () => {
		expect(readRefusal(422, "not_over")).toEqual({ kind: "failed", code: "not_over" });
		expect(readRefusal(404, "no_page")).toEqual({ kind: "failed", code: "no_page" });
		expect(readRefusal(500, "Internal error")).toEqual({ kind: "failed", code: "failed" });
		expect(readRefusal(409, "something_else")).toEqual({ kind: "failed", code: "failed" });
	});
});

describe("failedCopy", () => {
	it("has its own line for each code the server sends", () => {
		const codes = ["not_enough", "edited_while_writing", "thin_draft", "not_saved", "billing", "not_over", "busy"];
		const lines = codes.map(failedCopy);
		expect(new Set(lines).size).toBe(codes.length);
		for (const line of lines) expect(line).not.toBe(failedCopy("failed"));
	});

	it("says an interrupted rewrite stopped, without claiming the page is unchanged", () => {
		expect(failedCopy("interrupted")).toBe(STOPPED);
		expect(STOPPED).not.toMatch(/unchanged|hasn't changed/);
	});

	it("claims nothing about the page when the outcome is unknown, or another writer has the day", () => {
		for (const line of [UNKNOWN, failedCopy("busy")]) expect(line).not.toMatch(/unchanged|hasn't changed/);
	});

	it("says what happened and what to do when a page didn't come back", () => {
		expect(SHOW_FAILED).toMatch(/Try Show it again\.$/);
		expect(PUT_BACK_UNSEEN).toMatch(/Open this day again to see it\.$/);
	});

	it("says a new page that isn't saved yet is saved again without giving your server a mind", () => {
		expect(failedCopy("not_saved")).toBe(
			"Your server wrote the new page but couldn't save it yet. It saves it again on its own, so check back in a minute.",
		);
	});

	it("falls back to the general line for anything else", () => {
		expect(failedCopy("no_page")).toBe(failedCopy("failed"));
		expect(failedCopy("needs_consent")).toBe(failedCopy("failed"));
		expect(failedCopy("failed")).toMatch(/^Your server couldn't write this day's page again/);
	});
});

describe("putBackCopy", () => {
	it("says the earlier page is back in the day page's words", () => {
		expect(putBackCopy({ changed: true, saved: true }, true)).toBe("Your server put the earlier page back.");
		expect(putBackCopy({ changed: true, saved: true }, false)).toBe(PUT_BACK_UNSEEN);
	});

	it("says nothing changed when the earlier page was already back", () => {
		expect(putBackCopy({ changed: false, saved: true }, true)).toBe(
			"The earlier page was already back, so nothing changed.",
		);
		expect(putBackCopy({ changed: false, saved: true }, false)).toMatch(/Open this day again to see it\.$/);
	});

	it("says to check back when your server couldn't save it yet", () => {
		expect(putBackCopy({ changed: true, saved: false }, false)).toBe(
			"Your server put the earlier page back but couldn't save it yet. It saves it again on its own, so check back in a minute.",
		);
	});

	it("names no version, since the day page shows none", () => {
		for (const changed of [true, false])
			for (const saved of [true, false])
				for (const shown of [true, false]) expect(putBackCopy({ changed, saved }, shown)).not.toMatch(/version/);
	});
});

describe("the server in this page's lines", () => {
	it("never wants or tries", () => {
		const codes = ["not_enough", "edited_while_writing", "thin_draft", "not_saved", "billing", "not_over", "busy", "failed"];
		const lines = [
			...codes.map(failedCopy),
			UNKNOWN,
			STOPPED,
			SHOW_FAILED,
			PUT_BACK_UNSEEN,
			putBackCopy({ changed: true, saved: false }, false),
			confirmBody(true, 1),
		];
		for (const line of lines) expect(line).not.toMatch(/\b(tries|trying|wants|wanting)\b/);
	});
});

describe("confirmBody", () => {
	const base =
		"Your server writes the page again from this day's record, which can take a few minutes. It keeps the current page in History, so you can put it back.";

	it("says what happens and that the page can be put back", () => {
		expect(confirmBody(false, 0)).toBe(base);
	});

	it("adds the edits line only when the rewrite replaces your changes", () => {
		expect(confirmBody(true, 0)).toBe(`${base} It replaces any changes you made to this page, too.`);
	});

	it("adds the notes line only when a note sits on a sentence", () => {
		expect(confirmBody(false, 2)).toBe(
			`${base} Your notes stay. A note whose sentence changes moves to the top of the page.`,
		);
		expect(confirmBody(true, 1)).toBe(
			`${base} It replaces any changes you made to this page, too. Your notes stay. A note whose sentence changes moves to the top of the page.`,
		);
	});
});

const BEFORE = `You walked to the river.

## Morning

You met David Okafor at the clinic.[^ev-1] You got home.[^ev-2]

[^ev-1]: Message · 9:00 AM · data_communication_message:m1
[^ev-2]: Message · 9:30 AM · data_communication_message:m2
`;

const AFTER = `A quiet day by the river.

## Morning

You met David Okafor at the clinic.[^ev-1] You rested.[^ev-2]

[^ev-1]: Message · 9:00 AM · data_communication_message:m1
[^ev-2]: Message · 9:30 AM · data_communication_message:m2
`;

describe("notes and a rewrite", () => {
	const before = parseDayArticle(BEFORE).blocks;
	const after = parseDayArticle(AFTER).blocks;
	const notes = [
		{ anchor: { quote: "You met David Okafor at the clinic.", sentence: 0 } },
		{ anchor: { quote: "You got home.", sentence: 1 } },
		{ anchor: null },
		{ anchor: { quote: "A sentence no page has said." } },
	];

	it("counts the notes that sit beside a sentence", () => {
		expect(anchoredNoteCount(before, notes)).toBe(2);
		expect(anchoredNoteCount(before, [{ anchor: null }, {}])).toBe(0);
	});

	it("counts only notes the new page moved to the top", () => {
		// The clinic sentence survives; "You got home." does not. The note
		// with no passage and the one already lost were at the top before.
		expect(movedNoteCount(before, after, notes)).toBe(1);
		expect(movedNoteCount(before, before, notes)).toBe(0);
		expect(movedNoteCount(before, [], notes)).toBe(2);
	});

	it("says how many moved, and nothing when none did", () => {
		expect(movedNotesCopy(0)).toBeNull();
		expect(movedNotesCopy(1)).toBe("One of your notes no longer matches a sentence, so it's at the top of the page.");
		expect(movedNotesCopy(3)).toBe("3 of your notes no longer match a sentence, so they're at the top of the page.");
	});
});
