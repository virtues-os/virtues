/**
 * The day article's markdown, split into what the page draws.
 *
 * The narrator (`virtues-core/src/api/day_article.rs`) writes one ordinary
 * markdown page: the Abstract as the first paragraph, sections under `## `,
 * and GFM footnotes the page lifts out of the text instead of drawing them at
 * the bottom.
 * Footnote labels carry their kind:
 *
 * - `[^ev-N]` evidence: `Kind · time · table:id`, closing the sentence it
 *   supports. Clicking the sentence shows that record's words.
 * - `[^cx-N]` context: a plain fact, e.g. a section's time span.
 *
 * A fenced block whose info string is `figure` is a figure (a quote, an
 * exchange, the day's route): `key: value` lines the page draws as one.
 *
 * A page written before the narrator had footnotes parses the same way, with
 * no notes: its first paragraph is still the lede.
 */

import { zoneOffset } from "$lib/timeline/scale";

export type NoteKind = "ev" | "cx";

export interface MarginNote {
	kind: NoteKind;
	/** The note's text: "Message · 4:52 PM" on an evidence card, "7:46–8:41 AM" in the margin. */
	label: string;
	/** `table:id` of the record an evidence note opens, when it has one. */
	ref: string | null;
}

export type BlockKind = "heading" | "paragraph" | "table" | "figure" | "other";

/**
 * One sentence of a paragraph and the evidence that closes it. The writer ends
 * each sentence it could source with a run of markers. A sentence without one
 * (kept unsourced, or added by hand in the editor) is split off at its full
 * stop and carries no evidence, so it never borrows the next sentence's.
 */
export interface Sentence {
	/** Inline markdown, markers removed, veil marks kept. */
	markdown: string;
	evidence: MarginNote[];
	/** Whether whitespace came before it in the paragraph. */
	space: boolean;
}

export interface ArticleBlock {
	kind: BlockKind;
	/** The block's markdown with its footnote markers removed. */
	markdown: string;
	notes: MarginNote[];
	/** A paragraph's sentences, in order. Empty for every other kind. */
	sentences: Sentence[];
	/** A figure's `key: value` lines, keys lowercase. Only on a figure. */
	fields?: Record<string, string>;
}

export interface DayArticle {
	/** The first paragraph, markers removed. Empty when the page has none. */
	abstract: string;
	blocks: ArticleBlock[];
}

const DEF = /^\[\^(ev|cx)-(\d+)\]:\s*(.+)$/;
const MARK = /\[\^(ev|cx)-(\d+)\]/g;
const MARK_RUN = /(?:\[\^(?:ev|cx)-\d+\])+/g;
const REF = /^([a-z_]+:[A-Za-z0-9_\-]+)$/;

/** A full stop, then the start of a new sentence. */
const END = /(?<=[.!?]["”’)]?)\s+(?=[A-Z⟦[*"“])/;
/** Stops that don't end a sentence: "St. John", "Mt. Bonnell", an initial ("David O. Okafor"). */
const ABBR = /\b(?:Dr|Mr|Mrs|Ms|St|Mt|Ft|Jr|Sr|vs|e\.g|i\.e|[A-Z])\.$/;

const count = (s: string, c: string) => s.split(c).length - 1;
/** A link, veil mark or bold span still open, so a stop here is inside it. */
const unclosed = (s: string) =>
	count(s, "[") > count(s, "]") || count(s, "⟦") > count(s, "⟧") || count(s, "(") > count(s, ")") || count(s, "**") % 2 === 1;

function splitSentences(chunk: string): string[] {
	const parts: string[] = [];
	for (const p of chunk.split(END)) {
		const last = parts[parts.length - 1];
		if (last !== undefined && (ABBR.test(last) || unclosed(last))) parts[parts.length - 1] += ` ${p}`;
		else parts.push(p);
	}
	return parts;
}

function sentencesOf(raw: string, defs: Map<string, MarginNote>): Sentence[] {
	const text = raw.replace(/\s*\n\s*/g, " ");
	const out: Sentence[] = [];
	// Everything up to a run of markers is one or more sentences; only the
	// last of them is the one the markers close.
	const push = (piece: string, evidence: MarginNote[]) => {
		const parts = splitSentences(piece.trim());
		parts.forEach((p, i) =>
			out.push({ markdown: p, evidence: i === parts.length - 1 ? evidence : [], space: i > 0 || /^\s/.test(piece) }),
		);
	};
	let from = 0;
	for (const run of text.matchAll(MARK_RUN)) {
		const evidence: MarginNote[] = [];
		for (const m of run[0].matchAll(MARK)) {
			const note = defs.get(`${m[1]}-${m[2]}`);
			if (note?.kind === "ev") evidence.push(note);
		}
		const piece = text.slice(from, run.index);
		if (piece.trim()) push(piece, evidence);
		else out.at(-1)?.evidence.push(...evidence);
		from = (run.index ?? 0) + run[0].length;
	}
	const tail = text.slice(from);
	if (tail.trim()) push(tail, []);
	return out;
}

function parseDefinition(kind: NoteKind, body: string): MarginNote {
	const parts = body.split(" · ").map((p) => p.trim());
	const last = parts[parts.length - 1];
	if (kind === "ev" && parts.length > 1 && REF.test(last)) {
		return { kind, label: parts.slice(0, -1).join(" · "), ref: last };
	}
	return { kind, label: body.trim(), ref: null };
}

const FIGURE = /^```figure[ \t]*\n([\s\S]*?)\n?```$/;

/** A figure's lines as fields: the first `:` splits key from value. */
function figureFields(block: string): Record<string, string> | null {
	const m = block.match(FIGURE);
	if (!m) return null;
	const fields: Record<string, string> = {};
	for (const line of m[1].split("\n")) {
		const at = line.indexOf(":");
		const key = line.slice(0, at).trim().toLowerCase();
		if (at > 0 && key) fields[key] = line.slice(at + 1).trim();
	}
	return fields;
}

/** The records a figure stands on, as `table:id`: a thread's `refs`, any other's `ref`. */
export function figureRefs(fields: Record<string, string>): string[] {
	return ((fields.kind === "thread" ? fields.refs : fields.ref) ?? "")
		.split(",")
		.map((r) => r.trim())
		.filter((r) => r.includes(":"));
}

/**
 * A paragraph that only says what wasn't recorded ("Nothing was recorded
 * between 7:41 and 12:39.") is the margin's to say: its gap mark is computed
 * from the day's coverage, so the paragraph goes.
 */
const PROSE_TIME = String.raw`\d{1,2}(?::\d{2})?(?:\s*[ap]\.?\s?m\.?)?`;
const GAP_PARAGRAPH = new RegExp(
	String.raw`^Nothing was recorded (?:between ${PROSE_TIME} and ${PROSE_TIME}|before ${PROSE_TIME}|after ${PROSE_TIME})\.?$`,
	"i",
);

/** Blank lines part blocks, except inside a fence, which stays one block. */
function splitBlocks(text: string): string[] {
	const out: string[] = [];
	let lines: string[] = [];
	let fenced = false;
	for (const line of text.split("\n")) {
		if (/^\s*```/.test(line)) fenced = !fenced;
		if (!fenced && !line.trim()) {
			if (lines.length) out.push(lines.join("\n").trim());
			lines = [];
		} else {
			lines.push(line);
		}
	}
	if (lines.length) out.push(lines.join("\n").trim());
	return out.filter(Boolean);
}

function kindOf(block: string): BlockKind {
	if (FIGURE.test(block)) return "figure";
	if (/^#{1,6}\s/.test(block)) return "heading";
	if (block.split("\n").every((l) => l.trim().startsWith("|"))) return "table";
	if (/^(```|>|-\s|\*\s|\d+\.\s|!\[)/.test(block)) return "other";
	return "paragraph";
}

export function parseDayArticle(markdown: string): DayArticle {
	const defs = new Map<string, MarginNote>();
	const body: string[] = [];
	for (const line of markdown.split("\n")) {
		const m = line.trim().match(DEF);
		if (m) defs.set(`${m[1]}-${m[2]}`, parseDefinition(m[1] as NoteKind, m[3]));
		else body.push(line);
	}

	const raw = splitBlocks(body.join("\n"));

	const blocks: ArticleBlock[] = [];
	for (const b of raw) {
		const notes: MarginNote[] = [];
		for (const m of b.matchAll(MARK)) {
			const note = defs.get(`${m[1]}-${m[2]}`);
			if (note) notes.push(note);
		}
		// A marker alone on its line (a table's evidence) belongs to the block before it.
		const text = b.replace(MARK, "").replace(/[ \t]+$/gm, "").trim();
		if (!text && notes.length && blocks.length) {
			blocks[blocks.length - 1].notes.push(...notes);
			continue;
		}
		const markdownText = text.replace(/\n\s*$/g, "");
		const kind = kindOf(markdownText);
		if (kind === "paragraph" && GAP_PARAGRAPH.test(markdownText)) continue;
		if (kind === "figure") {
			blocks.push({ kind, markdown: markdownText, notes, sentences: [], fields: figureFields(markdownText) ?? {} });
			continue;
		}
		blocks.push({ kind, markdown: markdownText, notes, sentences: kind === "paragraph" ? sentencesOf(b, defs) : [] });
	}

	const first = blocks.findIndex((b) => b.kind === "paragraph");
	const abstract = first === 0 ? blocks[0].markdown : "";
	return { abstract, blocks: first === 0 ? blocks.slice(1) : blocks };
}

/**
 * The writer wraps what the veil may hide — names, and phrases about hard
 * or intimate things — in ⟦ ⟧. Reading normally, the marks are simply gone.
 */
export function veilMarks(markdown: string): { markdown: string; phrases: string[] } {
	const phrases: string[] = [];
	const out = markdown.replace(/⟦([^⟧]*)⟧/g, (_, p: string) => {
		phrases.push(p);
		return p;
	});
	return { markdown: out, phrases };
}

/** The Abstract of another day's page, for a previous / next card. */
export function abstractOf(markdown: string | null | undefined): string {
	if (!markdown) return "";
	return veilMarks(parseDayArticle(markdown).abstract).markdown.replace(/\[([^\]]+)\]\([^)]*\)/g, "$1");
}

// ── The margin's computed marks ──────────────────────────────────────────

/** A silence shorter than this gets no mark. */
export const GAP_MIN_MINUTES = 30;
/** No more than this many "new" marks on a page. */
export const FIRSTS_PER_PAGE = 3;

const MINUTE = 60_000;

/** The instant a YYYY-MM-DD day starts in `zone`, exact on a daylight-saving change. */
export function dayStartIn(slug: string, zone: string): number {
	const [y, m, d] = slug.split("-").map(Number);
	const wall = Date.UTC(y, m - 1, d);
	return wall - zoneOffset(wall - zoneOffset(wall, zone), zone);
}

/** Minutes on the day's clock at an instant: 0 at its midnight, 1440 at the next. */
export function clockMinutes(t: number, zone: string, dayEnd: number): number {
	if (t >= dayEnd) return 24 * 60;
	const local = Math.floor((t + zoneOffset(t, zone)) / MINUTE);
	return ((local % 1440) + 1440) % 1440;
}

/** "12:25–10:22 AM", or "11:20 AM–1:05 PM" across noon, on the day's clock. */
export function timeRange(start: number, end: number, zone: string): string {
	const fmt = (t: number) =>
		new Date(t).toLocaleTimeString("en-US", { timeZone: zone, hour: "numeric", minute: "2-digit" }).replace(/\s+/g, " ");
	const [a, b] = [fmt(start), fmt(end)];
	const [time, half] = a.split(" ");
	return half === b.split(" ")[1] ? `${time}–${b}` : `${a}–${b}`;
}

/**
 * The silences in a day's recording: every stretch of at least
 * `GAP_MIN_MINUTES` between `start` and `end` (or now, on a day still going)
 * that no recorded span covers. No coverage at all is not a mark: the day had
 * no recording to have gaps in.
 */
export function recordingGaps(
	coverage: [string, string][] | null,
	start: number,
	end: number,
	now = Date.now(),
): { start: number; end: number }[] {
	if (!coverage?.length) return [];
	const until = Math.min(end, now);
	const spans = coverage.map(([s, e]) => [Date.parse(s), Date.parse(e)] as const).sort((x, y) => x[0] - y[0]);
	const out: { start: number; end: number }[] = [];
	let at = start;
	for (const [s, e] of spans) {
		const to = Math.min(s, until);
		if (to - at >= GAP_MIN_MINUTES * MINUTE) out.push({ start: at, end: to });
		at = Math.max(at, e);
	}
	if (until - at >= GAP_MIN_MINUTES * MINUTE) out.push({ start: at, end: until });
	return out;
}

/** What the margin says beside a paragraph, besides your notes. */
export type MarginMark =
	| { kind: "gap"; spans: string[] }
	/** `title` is what's new when it has a name; `veil` is what the veil hides. */
	| { kind: "first"; title: string | null; line: string; veil: string[] };

/** A first, ready to place: its moment on the day's clock, and how much it matters. */
export interface FirstMark {
	at: number;
	title: string | null;
	line: string;
	veil: string[];
	/** Lower places first when the page has more than it can show. */
	rank: number;
}

const CLOCK = /\b(\d{1,2}):(\d{2})\s*([AP])\.?M\b/i;

/** The minutes on the day's clock of a paragraph's evidence, in reading order. */
function evidenceMinutes(block: ArticleBlock): number[] {
	return block.sentences.flatMap((s) =>
		s.evidence.flatMap((n) => {
			const m = n.label.match(CLOCK);
			if (!m) return [];
			const h = (Number(m[1]) % 12) + (m[3].toUpperCase() === "P" ? 12 : 0);
			return [h * 60 + Number(m[2])];
		}),
	);
}

/**
 * Where each computed mark goes, by block index: only beside paragraphs.
 *
 * A gap goes beside the first paragraph whose first evidence is at or after
 * the gap's end (before all the evidence, the first paragraph; after it, the
 * last), and gaps that land on one paragraph share one mark. A first goes
 * beside the paragraph whose evidence spans its moment, else the next one
 * after it; one to a paragraph, `FIRSTS_PER_PAGE` to a page, lowest rank
 * first. A first with no name (a place nobody named) shows once a section:
 * beside the paragraph that tells the visit it adds something, and a second
 * "First visit in your record" under one heading only repeats the first.
 * Times are minutes on the day's clock.
 */
export function placeMarks(
	blocks: ArticleBlock[],
	gaps: { end: number; label: string }[],
	firsts: FirstMark[],
): Map<number, MarginMark[]> {
	const out = new Map<number, MarginMark[]>();
	const paragraphs = blocks.flatMap((b, i) => (b.kind === "paragraph" ? [{ i, times: evidenceMinutes(b) }] : []));
	if (!paragraphs.length) return out;
	const timed = paragraphs
		.filter((p) => p.times.length)
		.map((p) => ({ i: p.i, first: p.times[0], lo: Math.min(...p.times), hi: Math.max(...p.times) }));
	const earliest = Math.min(...timed.map((p) => p.lo));
	const firstParagraph = paragraphs[0].i;
	const lastParagraph = paragraphs[paragraphs.length - 1].i;
	const add = (i: number, mark: MarginMark) => out.set(i, [...(out.get(i) ?? []), mark]);

	for (const g of gaps) {
		const home = g.end <= earliest ? firstParagraph : (timed.find((p) => p.first >= g.end)?.i ?? lastParagraph);
		const there = out.get(home)?.find((m) => m.kind === "gap");
		if (there?.kind === "gap") there.spans.push(g.label);
		else add(home, { kind: "gap", spans: [g.label] });
	}

	const sectionOf = (i: number) => blocks.slice(0, i).filter((b) => b.kind === "heading").length;
	let shown = 0;
	const taken = new Set<number>();
	const unnamedIn = new Set<number>();
	for (const f of [...firsts].sort((a, b) => a.rank - b.rank || a.at - b.at)) {
		if (shown >= FIRSTS_PER_PAGE) break;
		const home =
			timed.find((p) => p.lo <= f.at && f.at <= p.hi)?.i ?? timed.find((p) => p.lo >= f.at)?.i ?? lastParagraph;
		if (taken.has(home)) continue;
		if (f.title === null) {
			if (unnamedIn.has(sectionOf(home))) continue;
			unnamedIn.add(sectionOf(home));
		}
		taken.add(home);
		shown += 1;
		add(home, { kind: "first", title: f.title, line: f.line, veil: f.veil });
	}
	return out;
}
