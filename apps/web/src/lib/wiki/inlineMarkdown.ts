/**
 * The inline markdown a day article's sentence can carry, as tokens: links
 * (and the editor's `![@Name](/person/id)` ref form), emphasis, strong,
 * strike and code. DayInline draws them; nothing here touches the DOM.
 */

import { parseEntityRoute } from "$lib/utils/refRoutes";

export type Token =
	| { kind: "text"; text: string }
	| { kind: "code"; text: string }
	| { kind: "link"; text: string; url: string }
	| { kind: "em" | "strong" | "del"; children: Token[] };

// Code first, so nothing inside it is read as markup. A URL may hold one
// level of balanced parentheses. A delimiter hugs its text, and a single
// `*` or `_` never sits inside a word.
const INLINE =
	/`([^`]+)`|(!?)\[([^\]]+)\]\(((?:[^()\s]|\([^()\s]*\))+)\)|\*\*(?=\S)(.+?)(?<=\S)\*\*|~~(?=\S)(.+?)(?<=\S)~~|(?<![\w*])\*(?=\S)([^*]+?)(?<=\S)\*(?![\w*])|(?<![\w_])_(?=\S)([^_]+?)(?<=\S)_(?![\w_])/g;

export function tokenize(md: string): Token[] {
	const out: Token[] = [];
	let from = 0;
	for (const m of md.matchAll(INLINE)) {
		const at = m.index ?? 0;
		if (at > from) out.push({ kind: "text", text: md.slice(from, at) });
		if (m[1] !== undefined) out.push({ kind: "code", text: m[1] });
		else if (m[3] !== undefined) {
			// The editor writes an entity as `![@Nick](/person/id)`; Markdown
			// drew that as the ref, without the `@`.
			const text = m[2] && m[3].startsWith("@") ? m[3].slice(1) : m[3];
			if (m[2] && parseEntityRoute(m[4]) === null) out.push({ kind: "text", text: m[0] });
			else out.push({ kind: "link", text, url: m[4] });
		} else if (m[5] !== undefined) out.push({ kind: "strong", children: tokenize(m[5]) });
		else if (m[6] !== undefined) out.push({ kind: "del", children: tokenize(m[6]) });
		else out.push({ kind: "em", children: tokenize(m[7] ?? m[8]) });
		from = at + m[0].length;
	}
	if (from < md.length) out.push({ kind: "text", text: md.slice(from) });
	return out;
}
