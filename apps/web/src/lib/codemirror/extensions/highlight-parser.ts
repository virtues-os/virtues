/**
 * `==highlight==` as a Lezer inline parser
 *
 * CommonMark has no highlight construct, and the editor used to find one with
 * a regex, `/==(.+?)==/`, run over every line. A regex cannot see structure:
 * `a == b == c` is a sentence about equality, not a highlighted ` b `, and a
 * `==` inside a code span is characters — so every consumer of the regex had
 * to re-derive code context for itself. Teaching the parser the construct
 * puts a highlight where StrongEmphasis already lives, in the syntax tree,
 * with the same flanking discipline as `**`, and invisible inside code
 * because the parser never looks there.
 *
 * The node shape mirrors emphasis: a `Highlight` node wraps two
 * `HighlightMark` delimiter children around the content, so inline-marks.ts
 * measures it by type like every other construct.
 */

import type { InlineContext, MarkdownConfig } from '@lezer/markdown';

/** CommonMark's "punctuation": Unicode P and S categories. */
const PUNCTUATION = /[\p{P}\p{S}]/u;

export interface Flanking {
	/** Can open: not followed by whitespace, and not by punctuation unless preceded by whitespace/punctuation. */
	left: boolean;
	/** Can close: the mirror image. */
	right: boolean;
	punctBefore: boolean;
	punctAfter: boolean;
}

/**
 * CommonMark's flanking rules (spec 6.2) for a delimiter run, given the one
 * character before it and the one after it. Either may be `''` at the start
 * or end of the text, which counts as whitespace. Shared with the open-
 * construct pass in inline-marks.ts so the editor has one opinion about
 * what an opener is.
 */
export function flanking(before: string, after: string): Flanking {
	const spaceBefore = before === '' || /\s/.test(before);
	const spaceAfter = after === '' || /\s/.test(after);
	const punctBefore = PUNCTUATION.test(before);
	const punctAfter = PUNCTUATION.test(after);
	return {
		left: !spaceAfter && (!punctAfter || spaceBefore || punctBefore),
		right: !spaceBefore && (!punctBefore || spaceAfter || punctAfter),
		punctBefore,
		punctAfter,
	};
}

const EQUALS = 61; /* '=' */
const HighlightDelim = { resolve: 'Highlight', mark: 'HighlightMark' };

function parseHighlight(cx: InlineContext, next: number, pos: number): number {
	// Exactly two `=`. A longer run is never a delimiter (`===` is a literal,
	// as it is in every editor that has this construct), and checking both
	// neighbors means the second `=` of a run does not get a second look.
	if (next !== EQUALS || cx.char(pos + 1) !== EQUALS) return -1;
	if (cx.char(pos - 1) === EQUALS || cx.char(pos + 2) === EQUALS) return -1;

	const { left, right } = flanking(cx.slice(pos - 1, pos), cx.slice(pos + 2, pos + 3));
	if (!left && !right) return -1;
	return cx.addDelimiter(HighlightDelim, pos, pos + 2, left, right);
}

/** Markdown extension for `==highlight==`. Install through `virtuesMarkdown()`. */
export const highlightExtension: MarkdownConfig = {
	defineNodes: [{ name: 'Highlight' }, { name: 'HighlightMark' }],
	parseInline: [{ name: 'Highlight', parse: parseHighlight, after: 'Emphasis' }],
};
