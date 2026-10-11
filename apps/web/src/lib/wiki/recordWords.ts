/**
 * A cited record's own words, for the day page: a message as it was sent,
 * a recording as the turns nearest the sentence that cites it, and which of
 * the record's words the sentence shares with it. One place, so the evidence
 * card and Data's cited card show the same thing.
 */

import { speakerTurns } from "$lib/timeline/transcript";

type Row = Record<string, unknown>;

/** Words that carry no content of their own, so sharing one claims nothing. */
const STOP = new Set(
	(
		"a an the and or but if then so than as of to in on at by for with from into onto about over under up down out off " +
		"been is am are was were be being do does did done have has had having will would can could should shall may might must " +
		"i me my mine you your yours he him his she her hers it its we us our ours they them their theirs " +
		"this that these those there here what which who whom whose when where why how " +
		"not no yes just like said says say told tell asked ask all any some very too also again only even still " +
		"oh um uh yeah okay ok well really " +
		"i'm i'll i've i'd you're you'll you've you'd he's she's it's we're we'll we've they're they'll they've " +
		"that's there's what's who's don't doesn't didn't can't won't isn't aren't wasn't weren't haven't hasn't hadn't " +
		"wouldn't couldn't shouldn't let's gonna wanna"
	).split(" "),
);

/** -s, -es, -ed and -ing come off, then a final e and a doubled consonant, so
 *  "walks", "walked" and "walking" meet at "walk", "loved" meets "love", and
 *  "stopped" meets "stop". Light on purpose: a stem only has to match itself. */
export function stem(word: string): string {
	let w = word.toLowerCase().replace(/[’']s$/, "").replace(/[’']/g, "");
	if (w.length > 4 && /(?:s|x|z|ch|sh)es$/.test(w)) w = w.slice(0, -2);
	else if (w.length > 3 && w.endsWith("s") && !/(?:ss|us|is)$/.test(w)) w = w.slice(0, -1);
	if (w.length > 4 && w.endsWith("ing")) w = w.slice(0, -3);
	else if (w.length > 3 && w.endsWith("ed") && !w.endsWith("eed")) w = w.slice(0, -2);
	if (w.length > 2 && w.endsWith("e")) w = w.slice(0, -1);
	if (w.length > 3 && /([b-df-hj-np-tv-z])\1$/.test(w) && !/(?:ll|ss|zz)$/.test(w)) w = w.slice(0, -1);
	return w;
}

interface Token {
	start: number;
	end: number;
	/** Its stem, or null for a word that carries no content. */
	key: string | null;
}

const WORD = /[\p{L}\p{N}][\p{L}\p{N}'’]*/gu;

function tokens(text: string): Token[] {
	const out: Token[] = [];
	for (const m of text.matchAll(WORD)) {
		const raw = m[0].toLowerCase().replace(/’/g, "'").replace(/'+$/, "");
		const content = !STOP.has(raw) && (raw.length >= 3 || /\d/.test(raw));
		out.push({ start: m.index ?? 0, end: (m.index ?? 0) + m[0].length, key: content ? stem(raw) : null });
	}
	return out;
}

/** A sentence's markdown as the words a reader sees: link text, no veil marks or emphasis. */
function plain(markdown: string): string {
	return markdown
		.replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
		.replace(/[⟦⟧*_`]/g, "");
}

/** The content words of a sentence (or any text), stemmed. */
export function contentWords(markdown: string): Set<string> {
	return new Set(tokens(plain(markdown)).flatMap((t) => (t.key ? [t.key] : [])));
}

/** How far a run of shared words reaches across words that carry nothing. */
const BRIDGE = 2;

/**
 * Where a record's text says what the sentence says: the ranges, as
 * `[start, end)` offsets into `text`, of the words the two share. Neighboring
 * shared words join into one run across up to two stopwords ("cab in
 * Boston"), never across a word the sentence doesn't have.
 */
export function sharedRanges(text: string, sentence: string | Set<string>): [number, number][] {
	const want = typeof sentence === "string" ? contentWords(sentence) : sentence;
	const out: [number, number][] = [];
	let gap = 0;
	for (const t of tokens(text)) {
		if (t.key && want.has(t.key)) {
			const last = out.at(-1);
			if (last && gap <= BRIDGE) last[1] = t.end;
			else out.push([t.start, t.end]);
			gap = 0;
		} else if (t.key) {
			gap = Infinity;
		} else {
			gap += 1;
		}
	}
	return out;
}

/** Text cut at its ranges, each piece marked or not, in order. */
export function markRanges(text: string, ranges: [number, number][]): { text: string; marked: boolean }[] {
	const out: { text: string; marked: boolean }[] = [];
	let at = 0;
	for (const [s, e] of ranges) {
		if (s > at) out.push({ text: text.slice(at, s), marked: false });
		out.push({ text: text.slice(s, e), marked: true });
		at = e;
	}
	if (at < text.length) out.push({ text: text.slice(at), marked: false });
	return out;
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
	const want = contentWords(sentence);
	let best = -1;
	let score = 0;
	turns.forEach((t, i) => {
		let s = 0;
		for (const w of contentWords(t)) if (want.has(w)) s += 1;
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
