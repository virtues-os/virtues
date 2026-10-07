/**
 * Rewrite this page, on a day page: what the server's answer means on screen.
 *
 * The rewrite runs on your server, not in the page that asked for it, so the
 * page only ever reads where it stands (`getDayRewrite`). The server keeps
 * that in memory: a restart forgets a rewrite that was running, and a
 * finished one stays readable until then.
 *
 * Whose finish an answer reports comes from the answer itself. Once a start
 * came back here, answered or not, a finish whose `started_at` differs from
 * the status the page read before that start is a later rewrite's, so it is
 * news however long ago it finished; the same `started_at` is the one the
 * page already knew about. Before any start here, a finish is news only
 * to a client that watched it run, or when it finished a few minutes ago. A
 * rewrite this client watched that comes back idle stopped before it
 * finished, and its line doesn't claim the page is unchanged: the server may
 * have written it just before it stopped.
 *
 * Only the server's answer ends a rewrite. There is no client timer that
 * turns into a failure, and a start that got no answer says so and keeps
 * asking instead of guessing how it went.
 */

import type { ArticleBlock } from "./dayArticle";
import { placeNotes, type NoteAnchor } from "./dayNotes";
import type { DayRewriteStatus } from "./api";

/** How long a finished rewrite stays news to a client that neither started nor watched it. */
export const FRESH_MS = 5 * 60_000;

/** How often to ask while a rewrite runs and the page is on screen. */
export const POLL_MS = 3000;

/** The line above the Abstract. */
export type RewriteLine =
	| { kind: "idle" }
	| { kind: "running" }
	/** The start got no answer, so only the server can say whether it began. */
	| { kind: "unknown" }
	/** It was running here and the server no longer has it: a restart. */
	| { kind: "stopped" }
	/** `loadFailed`: Show it asked for the new page and didn't get it. */
	| { kind: "done"; beforeVersion: number | null; loadFailed?: boolean }
	/** The new page is on screen; the earlier one is `beforeVersion` in History. */
	| { kind: "shown"; beforeVersion: number | null; moved: number }
	/** Put the earlier page back answered; `message` says how it went (`putBackCopy`). */
	| { kind: "putBack"; message: string }
	| { kind: "putBackFailed" }
	| { kind: "failed"; code: string };

export const IDLE: RewriteLine = { kind: "idle" };

/** Lines that wait on the server's next answer: the page asks every few seconds, and at once on return. */
export function keepsAsking(line: RewriteLine): boolean {
	return line.kind === "running" || line.kind === "unknown";
}

/**
 * Status asks for one page, one at a time. An answer counts only for the day
 * it was asked about, and only when no start came back here after the ask
 * went out: an answer from before a start can't say how that start went,
 * and mustn't replace the line the start's own answer set, a refusal
 * included. `day` is the caller's own counter, bumped when the day changes.
 */
export class StatusAsks {
	private starts = 0;
	/** The day an ask is out for. */
	private out: number | null = null;
	/** Another ask came while one was out. */
	private again = false;

	/**
	 * A start came back here: started, refused, or unanswered. Call it before
	 * reading the start's answer; answers to asks already out stop counting.
	 */
	started(): void {
		this.starts++;
	}

	/**
	 * Send an ask about `day`, or null when one is already out for it. The
	 * one out reports that when it lands (`again`), so the caller can still
	 * ask and the ask that wasn't sent isn't lost.
	 */
	send(day: number): { day: number; starts: number } | null {
		if (this.out === day) {
			this.again = true;
			return null;
		}
		this.out = day;
		this.again = false;
		return { day, starts: this.starts };
	}

	/**
	 * The ask came back. `current`: its answer is news about this day's latest
	 * start. `again`: another ask came while it was out.
	 */
	landed(ask: { day: number; starts: number }, day: number): { current: boolean; again: boolean } {
		let again = false;
		if (this.out === ask.day) {
			this.out = null;
			again = this.again;
			this.again = false;
		}
		const sameDay = ask.day === day;
		return { current: sameDay && ask.starts === this.starts, again: sameDay && again };
	}
}

function fresh(finishedAt: string | null | undefined, now: number): boolean {
	if (!finishedAt) return false;
	const t = Date.parse(finishedAt);
	return Number.isFinite(t) && now - t <= FRESH_MS;
}

/** What the page knew about this day's rewrites when an answer came. */
export interface Seen {
	/** This client saw the day's rewrite running, from its own start or an earlier answer. */
	watched: boolean;
	/**
	 * Set once a start came back here: the `started_at` of the last status
	 * the page read before that start, null when that status had none.
	 * Undefined while no start came back here.
	 */
	before?: string | null;
}

/**
 * Whether a finished rewrite is news to this page, rather than one it already
 * knew about. A page that saw the rewrite running gets that run's finish or a
 * later one, so it is news whatever it started from.
 */
function news(s: DayRewriteStatus, seen: Seen, now: number): boolean {
	if (seen.watched) return true;
	if (seen.before !== undefined) return (s.started_at ?? null) !== seen.before;
	return fresh(s.finished_at, now);
}

/** The line for a status the server sent. */
export function readStatus(s: DayRewriteStatus, seen: Seen, now: number): RewriteLine {
	switch (s.state) {
		case "running":
			return { kind: "running" };
		case "done":
			return news(s, seen, now) ? { kind: "done", beforeVersion: s.before_version ?? null } : IDLE;
		case "failed":
			return news(s, seen, now) ? { kind: "failed", code: s.code || "failed" } : IDLE;
		default:
			return seen.watched ? { kind: "stopped" } : IDLE;
	}
}

/**
 * The line once your server answered the ask after a start that got no
 * answer of its own. Only an answer that reads as idle says the start didn't
 * land: nothing runs and no finish is new. Anything else is as read.
 */
export function afterUnansweredStart(read: RewriteLine): RewriteLine {
	return read.kind === "idle" ? { kind: "failed", code: "failed" } : read;
}

/**
 * What a refused or unanswered start means. `status` is the HTTP status, or
 * null when no answer came back. With no answer, or a gateway's 502, 503 or
 * 504 in front of the server, the rewrite may have started anyway, and the
 * page asks before saying anything.
 */
export type StartRefusal =
	| { kind: "consent" }
	| { kind: "running" }
	| { kind: "ask" }
	| { kind: "failed"; code: string };

export function readRefusal(status: number | null, code: string | null | undefined): StartRefusal {
	if (status === null || status === 502 || status === 503 || status === 504) return { kind: "ask" };
	if (status === 409 && code === "needs_consent") return { kind: "consent" };
	if (status === 409 && code === "rewrite_in_progress") return { kind: "running" };
	// Another writer holds the day, such as the nightly one: no rewrite of
	// yours is running to watch.
	if (status === 409 && code === "busy") return { kind: "failed", code: "busy" };
	if (status === 422 && code === "not_over") return { kind: "failed", code: "not_over" };
	if (status === 404 && code === "no_page") return { kind: "failed", code: "no_page" };
	return { kind: "failed", code: "failed" };
}

/** A rewrite that ended without finishing: a restart, or a crash. The page may or may not have changed. */
export const STOPPED = "Your server stopped before it finished. Rewrite the page again if it still needs it.";

/** A start with no answer, and no answer to the ask after it either. */
export const UNKNOWN =
	"Your server didn't answer, so this page can't tell yet whether it started. It keeps asking.";

/** Show it, when the new page didn't come back. */
export const SHOW_FAILED = "Your server couldn't load the new page. Try Show it again.";

/** Put the earlier page back, when the page itself didn't come back afterwards. */
export const PUT_BACK_UNSEEN = "Your server put the earlier page back. Open this day again to see it.";

/**
 * The line after Put the earlier page back, in this page's own words rather
 * than the revert's general ones: the day page names no version. `shown` is
 * whether the page your server has now came back and is on screen.
 */
export function putBackCopy(outcome: { changed: boolean; saved: boolean }, shown: boolean): string {
	if (!outcome.changed) {
		return shown
			? "The earlier page was already back, so nothing changed."
			: "The earlier page was already back, so nothing changed. Open this day again to see it.";
	}
	if (!outcome.saved) {
		return "Your server put the earlier page back but couldn't save it yet. It saves it again on its own, so check back in a minute.";
	}
	return shown ? "Your server put the earlier page back." : PUT_BACK_UNSEEN;
}

/** What each failure code says. Anything not listed gets the last line. */
export function failedCopy(code: string): string {
	switch (code) {
		case "not_enough":
			return "Your server couldn't find enough in this day's record to write a new page, so this one hasn't changed. If more of the day arrives later, rewrite it then.";
		case "edited_while_writing":
			return "This page changed while your server was writing, so your server left it as it is. Rewrite it again if you still want a new page.";
		case "thin_draft":
			return "Your server's new page came out too short to replace this one, so this one hasn't changed. Try again later.";
		case "not_saved":
			return "Your server wrote the new page but couldn't save it yet. It saves it again on its own, so check back in a minute.";
		case "billing":
			return "Billing stopped your server before it could write the page, so this one hasn't changed. Check Billing, then try again.";
		case "not_over":
			return "This day isn't over yet where you spent it. Rewrite it after midnight there.";
		case "busy":
			return "Your server is writing this day's page right now. Try again in a few minutes.";
		case "interrupted":
			return STOPPED;
		default:
			return "Your server couldn't write this day's page again, so this one hasn't changed. Try again in a few minutes.";
	}
}

export const CONFIRM_TITLE = "Rewrite this day's page?";
export const CONFIRM_LABEL = "Rewrite the page";
export const CANCEL_LABEL = "Not now";

/**
 * The confirm's body. `withEdits` says the rewrite replaces changes you made,
 * and only a confirm that said so may send `replace_edits`.
 */
export function confirmBody(withEdits: boolean, anchoredNotes: number): string {
	let body =
		"Your server writes the page again from this day's record, which can take a few minutes. It keeps the current page in History, so you can put it back.";
	if (withEdits) body += " It replaces any changes you made to this page, too.";
	if (anchoredNotes > 0) body += " Your notes stay. A note whose sentence changes moves to the top of the page.";
	return body;
}

type Noted = { anchor?: NoteAnchor | null };

/** Notes that sit beside a sentence of this page, not beside the Abstract. */
export function anchoredNoteCount(blocks: ArticleBlock[], notes: Noted[]): number {
	return placeNotes(blocks, notes).filter((p) => p.at !== null).length;
}

/** Notes that sat beside a sentence before and sit beside the Abstract after. */
export function movedNoteCount(before: ArticleBlock[], after: ArticleBlock[], notes: Noted[]): number {
	const was = placeNotes(before, notes);
	const now = placeNotes(after, notes);
	return was.filter((p, i) => p.at !== null && now[i].at === null).length;
}

/** The line after a swap that moved notes, or null when none moved. */
export function movedNotesCopy(n: number): string | null {
	if (n <= 0) return null;
	if (n === 1) return "One of your notes no longer matches a sentence, so it's at the top of the page.";
	return `${n} of your notes no longer match a sentence, so they're at the top of the page.`;
}
