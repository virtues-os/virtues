/**
 * The day article's markdown, split into what the page draws.
 *
 * The narrator (`virtues-core/src/api/day_article.rs`) writes one ordinary
 * markdown page: the Abstract as the first paragraph, sections under `## `,
 * and GFM footnotes the page draws in its margin instead of at the bottom.
 * Footnote labels carry their kind:
 *
 * - `[^ev-N]` evidence: `Kind · time · table:id` — opens Record on that item.
 * - `[^cx-N]` context: a plain fact, e.g. a section's time span.
 *
 * A page written before the narrator had footnotes parses the same way, with
 * no notes: its first paragraph is still the lede.
 */

export type NoteKind = "ev" | "cx";

export interface MarginNote {
	kind: NoteKind;
	/** What the margin shows: "Message · 4:52 PM", "7:46–8:41 AM". */
	label: string;
	/** `table:id` of the record an evidence note opens, when it has one. */
	ref: string | null;
}

export type BlockKind = "heading" | "paragraph" | "table" | "other";

export interface ArticleBlock {
	kind: BlockKind;
	/** The block's markdown with its footnote markers removed. */
	markdown: string;
	notes: MarginNote[];
}

export interface DayArticle {
	/** The first paragraph, markers removed. Empty when the page has none. */
	abstract: string;
	blocks: ArticleBlock[];
}

const DEF = /^\[\^(ev|cx)-(\d+)\]:\s*(.+)$/;
const MARK = /\[\^(ev|cx)-(\d+)\]/g;
const REF = /^([a-z_]+:[A-Za-z0-9_\-]+)$/;

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
		blocks.push({ kind: kindOf(markdownText), markdown: markdownText, notes });
	}

	const first = blocks.findIndex((b) => b.kind === "paragraph");
	const abstract = first === 0 ? blocks[0].markdown : "";
	return { abstract, blocks: first === 0 ? blocks.slice(1) : blocks };
}

/** The Abstract of another day's page, for a previous / next card. */
export function abstractOf(markdown: string | null | undefined): string {
	if (!markdown) return "";
	return parseDayArticle(markdown).abstract.replace(/\[([^\]]+)\]\([^)]*\)/g, "$1");
}
