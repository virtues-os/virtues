/**
 * Helpers for the day page readers' tests: how a day page reads to the
 * readers that draw it, so two spellings of one page (its writer's markdown
 * and its block page's export) can be held to reading the same.
 * Not shipped: only test files import this.
 */

import { parseDayArticle, veilMarks, type ArticleBlock, type MarginNote } from "./dayArticle";
import { placeNotes, plainSentence, type NoteAnchor } from "./dayNotes";
import { plainText } from "./inlineMarkdown";

/** What a reader takes from one block, its markup's spelling aside. */
export interface BlockReading {
	kind: ArticleBlock["kind"];
	/** A heading's or a list's words, a table's cells; null for a figure and a paragraph (read by sentence). */
	words: string[] | null;
	sentences: { plain: string; veiled: string[]; evidence: MarginNote[]; space: boolean }[];
	notes: MarginNote[];
	fields: Record<string, string> | null;
}

export interface Reading {
	abstract: string;
	abstractVeiled: string[];
	blocks: BlockReading[];
}

const words = (s: string) => plainText(s).replace(/\s+/g, " ").trim();

/** A table's cells, row by row, its rule row and its cells' padding aside. */
function cells(table: string): string[] {
	return table
		.split("\n")
		.filter((line) => !/^\s*\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)*\|?\s*$/.test(line))
		.map((line) =>
			line
				.trim()
				.replace(/^\||\|$/g, "")
				.split(/(?<!\\)\|/)
				.map((cell) => words(cell))
				.join(" | "),
		);
}

/** A list's or a quote's lines, each without its marker. */
function lines(block: string): string[] {
	return block.split("\n").map((line) => words(line.replace(/^\s*(?:[-*+]|\d+[.)]|>)\s+/, "")));
}

/**
 * The phrases the veil hides in `md` as the page draws it: each marked
 * phrase the drawn words hold, as `use:veiled` looks for it there. A phrase
 * the drawn words do not hold is shown, and reads as missing.
 */
function hidden(md: string): string[] {
	const { markdown, phrases } = veilMarks(md);
	const drawn = plainText(markdown);
	return phrases.filter((p) => drawn.includes(p)).map(words);
}

function blockReading(b: ArticleBlock): BlockReading {
	const veil = hidden;
	return {
		kind: b.kind,
		words:
			b.kind === "table"
				? cells(veilMarks(b.markdown).markdown)
				: b.kind === "heading"
					? [words(veilMarks(b.markdown).markdown)]
					: b.kind === "other"
						? lines(veilMarks(b.markdown).markdown)
						: null,
		sentences: b.sentences.map((s) => ({
			plain: plainSentence(s.markdown),
			veiled: veil(s.markdown),
			evidence: s.evidence,
			space: s.space,
		})),
		notes: b.notes,
		fields: b.fields ?? null,
	};
}

/** How a day page reads to the page that draws it. */
export function reading(markdown: string): Reading {
	const a = parseDayArticle(markdown);
	return {
		abstract: plainSentence(a.abstract),
		abstractVeiled: hidden(a.abstract),
		blocks: a.blocks.map(blockReading),
	};
}

/**
 * Where each note lands on a page: the sentence it sits beside, as its plain
 * words and its place in its paragraph, or the words it lost. Not the
 * block's index, which a block before it shifts without moving the note.
 */
export function landings(markdown: string, anchors: NoteAnchor[]) {
	const blocks = parseDayArticle(markdown).blocks;
	return placeNotes(
		blocks,
		anchors.map((anchor) => ({ anchor })),
	).map((p) => ({
		beside: p.at ? plainSentence(blocks[p.at.block].sentences[p.at.sentence].markdown) : null,
		sentence: p.at?.sentence ?? null,
		lost: p.lost,
	}));
}

/** An anchor on every sentence of a page, as a note written beside each would keep. */
export function anchorsOn(markdown: string): NoteAnchor[] {
	return parseDayArticle(markdown).blocks.flatMap((b) =>
		b.sentences.map((s, sentence) => ({ quote: plainSentence(s.markdown), sentence })),
	);
}

/**
 * The ways two spellings of a page read differently, by kind of reading and
 * never quoting the page: what a check over a copy of a real box may print.
 */
export function readingDifferences(a: string, b: string, anchors: NoteAnchor[]): string[] {
	const [x, y] = [reading(a), reading(b)];
	const out: string[] = [];
	const same = (p: unknown, q: unknown) => JSON.stringify(p) === JSON.stringify(q);
	if (x.abstract !== y.abstract || !same(x.abstractVeiled, y.abstractVeiled)) out.push("abstract");
	if (!same(x.blocks.map((k) => k.kind), y.blocks.map((k) => k.kind))) out.push("blocks");
	else
		x.blocks.forEach((k, i) => {
			const l = y.blocks[i];
			if (!same(k.words, l.words)) out.push(`block ${i}: words`);
			if (!same(k.sentences.map((s) => [s.plain, s.space]), l.sentences.map((s) => [s.plain, s.space])))
				out.push(`block ${i}: sentences`);
			if (!same(k.sentences.map((s) => s.veiled), l.sentences.map((s) => s.veiled))) out.push(`block ${i}: veils`);
			if (!same(k.notes, l.notes) || !same(k.sentences.map((s) => s.evidence), l.sentences.map((s) => s.evidence)))
				out.push(`block ${i}: notes`);
			if (!same(k.fields, l.fields)) out.push(`block ${i}: figure fields`);
		});
	if (!same(landings(a, anchors), landings(b, anchors))) out.push("notes land elsewhere");
	return out;
}
