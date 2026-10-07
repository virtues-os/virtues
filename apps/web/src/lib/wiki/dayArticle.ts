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
 * A page written before the narrator had footnotes parses the same way, with
 * no notes: its first paragraph is still the lede.
 */

export type NoteKind = "ev" | "cx";

export interface MarginNote {
	kind: NoteKind;
	/** The note's text: "Message · 4:52 PM" on an evidence card, "7:46–8:41 AM" in the margin. */
	label: string;
	/** `table:id` of the record an evidence note opens, when it has one. */
	ref: string | null;
}

export type BlockKind = "heading" | "paragraph" | "table" | "other";

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

function kindOf(block: string): BlockKind {
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

	const raw = body
		.join("\n")
		.split(/\n\s*\n/)
		.map((b) => b.trim())
		.filter(Boolean);

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
