/**
 * A cited record's own words, for the day page: a message as it was sent,
 * a recording as the turns nearest the sentence that cites it. One place, so
 * the evidence card and Data's cited card show the same thing.
 */

import { speakerTurns } from "$lib/timeline/transcript";

type Row = Record<string, unknown>;

const STOP = new Set(
	"the and you your was were that this with for from had have her his she they them then there what when who into out about just like said told asked".split(
		" ",
	),
);

function words(s: string): Set<string> {
	const found = s
		.toLowerCase()
		.replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
		.replace(/[⟦⟧]/g, "")
		.match(/[a-z0-9']{3,}/g);
	return new Set((found ?? []).filter((w) => !STOP.has(w)));
}

export interface NearTurns {
	turns: { text: string; near: boolean }[];
	/** False when no turn shares a word with the sentence: these are the chunk's opening turns. */
	matched: boolean;
}

/**
 * The turns of a recording chunk nearest a sentence: the one sharing the most
 * of its words, the turn before it, and the turns after it, `span` in all.
 * Word overlap places the window; it claims nothing about which words support
 * the sentence. With no overlap at all, the chunk's opening turns, unmarked.
 */
export function nearTurns(text: string, sentence: string, span = 4): NearTurns {
	const turns = speakerTurns(text);
	const want = words(sentence);
	let best = -1;
	let score = 0;
	turns.forEach((t, i) => {
		let s = 0;
		for (const w of words(t)) if (want.has(w)) s += 1;
		if (s > score) [best, score] = [i, s];
	});
	const from = Math.max(0, best - 1);
	return {
		turns: turns.slice(from, from + span).map((t, i) => ({ text: t, near: from + i === best })),
		matched: best >= 0,
	};
}

/** A message's text, as sent, trimmed to a card's length. */
export function messageBody(row: Row, max = 420): string {
	const body = String(row.body ?? "").replace(/￼/g, "").trim();
	return body.length > max ? `${body.slice(0, max)}…` : body;
}

/** Who sent a message: you, or their name, and the group it went to. */
export function messageSender(row: Row): string {
	const meta = (row.metadata ?? {}) as Row;
	const fromMe = meta.is_from_me === true || meta.is_from_me === "true";
	const group = row.is_group_message === true ? (meta.group_title as string | undefined) : undefined;
	const sender = fromMe ? "You" : ((row.from_name as string | undefined) ?? "Someone");
	return group ? `${sender}, in ${group}` : sender;
}
