/**
 * Code context — where markdown syntax is NOT markdown
 *
 * Several decorators find their constructs by scanning line text with regexes
 * (links and media in ref-links.ts / media-widgets.ts, ==highlight== and
 * <u> in inline-marks.ts) because Lezer has no node for them or because the
 * regex predates the tree walk. A regex cannot know it is inside a code
 * region, and the result was verified on screen: a JS string literal
 * containing `[text](url)` rendered as a clickable link INSIDE the code
 * block, and the page's ref counter counted it.
 *
 * This module is the one answer to "is this range code?". Every regex-based
 * decorator collects the ranges once per build and drops matches that
 * overlap them. The Lezer-driven marks (bold/italic/inline code) never
 * needed this — the parser already refuses to see emphasis inside code.
 */

import { syntaxTree } from '@codemirror/language';
import type { EditorState } from '@codemirror/state';

export interface CodeRange {
	from: number;
	to: number;
}

/** Node names whose contents are literal text, never markdown. */
const CODE_NODES = new Set(['FencedCode', 'CodeBlock', 'InlineCode', 'HTMLBlock', 'CommentBlock']);

/**
 * Every code region overlapping [from, to), in document order.
 * Collect once per decoration build, then test matches with `inCode`.
 */
export function collectCodeRanges(state: EditorState, from: number, to: number): CodeRange[] {
	const ranges: CodeRange[] = [];
	syntaxTree(state).iterate({
		from,
		to,
		enter(node) {
			if (!CODE_NODES.has(node.name)) return;
			ranges.push({ from: node.from, to: node.to });
			// Nothing inside a code region needs visiting.
			return false;
		},
	});
	return ranges;
}

/** Does [from, to) overlap any collected code region? */
export function inCode(ranges: CodeRange[], from: number, to: number): boolean {
	for (const r of ranges) {
		if (from < r.to && to > r.from) return true;
	}
	return false;
}
