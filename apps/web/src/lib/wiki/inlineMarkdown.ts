/**
 * The inline markdown a day article's sentence can carry, as tokens: links
 * (and the editor's `[@Name](/person/id)` and `![@Name](/person/id)` ref
 * forms), emphasis, strong, strike and code. DayInline draws them; nothing
 * here touches the DOM.
 *
 * An article becomes a block page once opened, and its markdown is then the
 * page's export, which escapes what would otherwise read as markup: a `\`
 * before ASCII punctuation is that character (`AT\&T`, `user\_name`,
 * `\~20`), never markup, a character reference (`&amp;`, `&#32;`,
 * `&#124;`, `&#10214;`) is the character it names, and a `\` ending a line
 * is the hard line break the export writes where its writer wrote two
 * spaces. All read here as CommonMark reads them, so a sentence says the
 * same words in either form.
 */

import { parseEntityRoute } from "$lib/utils/refRoutes";

export type Token =
	| { kind: "text"; text: string }
	| { kind: "code"; text: string }
	| { kind: "link"; text: string; url: string }
	| { kind: "em" | "strong" | "del"; children: Token[] };

/**
 * A backslash escape: `\` and one ASCII punctuation character, which it is;
 * or `\` and a line's end, a hard line break, which reads as that line end.
 */
const ESCAPE = /\\([!-\/:-@[-`{-~]|\n)/g;
/** A character reference CommonMark reads in text. */
const REFERENCE = /&(#\d{1,7}|#[xX][\dA-Fa-f]{1,6}|amp|lt|gt|quot|apos|nbsp);/g;
const NAMED: Record<string, string> = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'", nbsp: " " };

/** The character a reference names, or the reference itself when it names none. */
function referenced(whole: string, name: string): string {
	if (!name.startsWith("#")) return NAMED[name] ?? whole;
	const code = /^#[xX]/.test(name) ? parseInt(name.slice(2), 16) : parseInt(name.slice(1), 10);
	if (!Number.isFinite(code) || code > 0x10ffff || (code >= 0xd800 && code <= 0xdfff)) return "�";
	return code === 0 ? "�" : String.fromCodePoint(code);
}

/** Markdown text with its escapes, hard line breaks and character references read: what a reader sees, markup aside. */
export function unescapeMarkdown(md: string): string {
	return md.replace(new RegExp(`${ESCAPE.source}|${REFERENCE.source}`, "g"), (whole, escaped?: string, name?: string) =>
		escaped !== undefined ? escaped : referenced(whole, name ?? ""),
	);
}

// An escape or a reference first, so an escaped delimiter is never read as
// one; then code, so nothing inside it is read as markup. A URL may hold one
// level of balanced parentheses, or sit in `<…>` as the export writes one
// with a space or a parenthesis. A delimiter hugs its text, never closes
// after a backslash, and a single `*` or `_` never sits inside a word.
const INLINE = new RegExp(
	[
		ESCAPE.source,
		REFERENCE.source,
		/`([^`]+)`/.source,
		/(!?)\[((?:\\.|[^\]\\])+)\]\((<[^<>\n]*>|(?:[^()\s]|\([^()\s]*\))+)\)/.source,
		/\*\*(?=\S)(.+?)(?<=[^\s\\])\*\*/.source,
		/~~(?=\S)(.+?)(?<=[^\s\\])~~/.source,
		/(?<![\w*])\*(?=\S)((?:\\.|[^*\\])+?)(?<=[^\s\\])\*(?![\w*])/.source,
		/(?<![\w_])_(?=\S)((?:\\.|[^_\\])+?)(?<=[^\s\\])_(?![\w_])/.source,
	].join("|"),
	"g",
);

/** A link's address as written: angle brackets off, escapes read. */
function address(raw: string): string {
	return unescapeMarkdown(raw.startsWith("<") && raw.endsWith(">") ? raw.slice(1, -1) : raw);
}

export function tokenize(md: string): Token[] {
	const out: Token[] = [];
	const text = (t: string) => {
		const last = out.at(-1);
		if (last?.kind === "text") last.text += t;
		else out.push({ kind: "text", text: t });
	};
	let from = 0;
	for (const m of md.matchAll(INLINE)) {
		const at = m.index ?? 0;
		if (at > from) text(md.slice(from, at));
		if (m[1] !== undefined) text(m[1]);
		else if (m[2] !== undefined) text(referenced(m[0], m[2]));
		else if (m[3] !== undefined) out.push({ kind: "code", text: m[3] });
		else if (m[5] !== undefined) {
			// A ref's `@` is how the editor writes one, `[@Nick](/person/id)`
			// or `![@Nick](/person/id)`, never part of its name: both are the
			// one mention, which a block page exports in the first form, and
			// both draw as the name.
			const label = unescapeMarkdown(m[5]);
			const url = address(m[6]);
			const ref = parseEntityRoute(url) !== null;
			const shown = ref && label.startsWith("@") ? label.slice(1) : label;
			if (m[4] && !ref) text(m[0]);
			else out.push({ kind: "link", text: shown, url });
		} else if (m[7] !== undefined) out.push({ kind: "strong", children: tokenize(m[7]) });
		else if (m[8] !== undefined) out.push({ kind: "del", children: tokenize(m[8]) });
		else out.push({ kind: "em", children: tokenize(m[9] ?? m[10]) });
		from = at + m[0].length;
	}
	if (from < md.length) text(md.slice(from));
	return out;
}

/**
 * Markdown with each hard line break's backslash read, the line's end left:
 * a backslash ending a line, not itself escaped.
 */
export function readHardBreaks(md: string): string {
	return md.replace(/\\(\\|\n)/g, (whole, next: string) => (next === "\n" ? "\n" : whole));
}

/** The words of inline markdown, its markup gone, read by the same rules the page draws with. */
export function plainText(md: string): string {
	const flat = (ts: Token[]): string => ts.map((t) => ("children" in t ? flat(t.children) : t.text)).join("");
	return flat(tokenize(md));
}
