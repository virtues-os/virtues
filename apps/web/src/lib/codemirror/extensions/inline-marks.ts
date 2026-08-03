/**
 * Inline marks — one description of WHERE inline formatting lives
 *
 * Bold, italic, strikethrough, inline code, ==highlight== and <u>underline</u>
 * follow the reveal-on-touch rule (the one Obsidian and every shipping
 * markdown live-preview converged on): a construct's delimiters are hidden
 * until the selection touches that construct, then they appear IN PLACE,
 * dimmed, while the styling stays applied. Only the construct you are inside
 * changes; nothing else on the line moves, and nothing moves vertically.
 *
 * This file deliberately owns no decorations. It answers two questions —
 * where are the marks (`inlineMarks`), and is the selection touching this one
 * (`selectionTouches`) — and live-preview.ts renders from those answers, so
 * there is exactly one definition of a construct's extent.
 *
 * History note: an earlier revision hid delimiters permanently and declared
 * them atomic to cursor motion. That is rich-text-editor UX imposed on a plain
 * text buffer, and it made the caret dishonest — every hidden `*` was a caret
 * position that existed but could not be seen or aimed at. Reveal-on-touch is
 * the fix, not a compromise: the document's characters are visible exactly
 * when the caret could interact with them.
 */

import { syntaxTree } from '@codemirror/language';
import type { EditorState } from '@codemirror/state';

import { collectCodeRanges, inCode } from './code-context';

/** `==highlight==` and `<u>underline</u>` have no Lezer nodes; they are scanned. */
const HIGHLIGHT_REGEX = /==(.+?)==/g;
const UNDERLINE_REGEX = /<u>(.*?)<\/u>/g;

export interface InlineMark {
	/** Class applied to the content between the delimiters. */
	cls: string;
	/** Opening delimiter range. */
	openFrom: number;
	openTo: number;
	/** Closing delimiter range. */
	closeFrom: number;
	closeTo: number;
}

/** Length of the delimiter that opens this construct, or 0 if unrecognized. */
function delimiterLength(text: string): number {
	if (text.startsWith('**') || text.startsWith('~~')) return 2;
	if (text.startsWith('*') || text.startsWith('_') || text.startsWith('`')) return 1;
	return 0;
}

const NODE_CLASS: Record<string, string> = {
	StrongEmphasis: 'cm-strong',
	Emphasis: 'cm-emphasis',
	Strikethrough: 'cm-strikethrough',
	InlineCode: 'cm-inline-code',
};

/**
 * Every inline mark overlapping [from, to), in document order.
 *
 * `from`/`to` should be a viewport range — this walks the syntax tree, so
 * keeping the range tight matters on long documents.
 */
export function inlineMarks(state: EditorState, from: number, to: number): InlineMark[] {
	const marks: InlineMark[] = [];

	syntaxTree(state).iterate({
		from,
		to,
		enter(node) {
			const cls = NODE_CLASS[node.name];
			if (!cls) return;

			const text = state.sliceDoc(node.from, node.to);
			const delim = delimiterLength(text);
			if (!delim) return;

			const innerFrom = node.from + delim;
			const innerTo = node.to - delim;
			if (innerFrom >= innerTo) return;

			marks.push({
				cls,
				openFrom: node.from,
				openTo: innerFrom,
				closeFrom: innerTo,
				closeTo: node.to,
			});
		},
	});

	// Line-scanned constructs. Lezer has no node for either, so they are matched
	// per line the way live-preview always has — which also means, unlike the
	// tree-walked marks above, the regex cannot see code context on its own.
	// `inCode` drops matches inside fences and inline code (a `==x==` in a
	// string literal is characters, not a highlight).
	const codeRanges = collectCodeRanges(state, from, to);
	const startLine = state.doc.lineAt(from).number;
	const endLine = state.doc.lineAt(Math.min(to, state.doc.length)).number;

	for (let lineNum = startLine; lineNum <= endLine; lineNum++) {
		const line = state.doc.line(lineNum);

		UNDERLINE_REGEX.lastIndex = 0;
		for (let m = UNDERLINE_REGEX.exec(line.text); m !== null; m = UNDERLINE_REGEX.exec(line.text)) {
			const at = line.from + m.index;
			if (inCode(codeRanges, at, at + m[0].length)) continue;
			marks.push({
				cls: 'cm-underline',
				openFrom: at,
				openTo: at + 3,
				closeFrom: at + 3 + m[1].length,
				closeTo: at + m[0].length,
			});
		}

		HIGHLIGHT_REGEX.lastIndex = 0;
		for (let m = HIGHLIGHT_REGEX.exec(line.text); m !== null; m = HIGHLIGHT_REGEX.exec(line.text)) {
			const at = line.from + m.index;
			if (inCode(codeRanges, at, at + m[0].length)) continue;
			marks.push({
				cls: 'cm-highlight',
				openFrom: at,
				openTo: at + 2,
				closeFrom: at + 2 + m[1].length,
				closeTo: at + m[0].length,
			});
		}
	}

	marks.sort((a, b) => a.openFrom - b.openFrom);
	return marks;
}

/**
 * Does any selection range touch this construct, edges inclusive?
 *
 * Inclusive on both ends so a caret sitting immediately before or after the
 * construct also reveals it — that caret is one keystroke from editing it, and
 * revealing on adjacency is what makes the boundary position aimable at all.
 */
export function selectionTouches(
	state: EditorState,
	range: { openFrom: number; closeTo: number },
): boolean {
	for (const r of state.selection.ranges) {
		if (r.from <= range.closeTo && r.to >= range.openFrom) return true;
	}
	return false;
}
