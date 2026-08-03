/**
 * Inline marks — one description of what is hidden, used twice
 *
 * Bold, italic, strikethrough, inline code, ==highlight== and <u>underline</u>
 * keep their delimiters hidden at all times, including on the caret's own line.
 * That is what stops a line from reflowing when the caret arrives — the old
 * behavior un-styled the whole line and every character after it moved.
 *
 * Hiding characters permanently has a consequence: the caret must not be able
 * to sit inside something invisible, or Backspace eats a `*` nobody can see.
 * So this module is the single source of truth for WHERE the delimiters are,
 * and it is consumed in two places:
 *
 *   - live-preview.ts builds the replace/mark decorations from it;
 *   - `inlineMarkAtoms` feeds the same ranges to EditorView.atomicRanges, which
 *     makes CodeMirror step the caret over them and delete them whole.
 *
 * Deriving both from one function is the point. If they disagreed, the caret
 * could land in a gap the renderer had already erased.
 */

import { syntaxTree } from '@codemirror/language';
import { type EditorState, type Extension, type Range, RangeSet, RangeValue } from '@codemirror/state';
import { EditorView } from '@codemirror/view';

/** `==highlight==` and `<u>underline</u>` have no Lezer nodes; they are scanned. */
const HIGHLIGHT_REGEX = /==(.+?)==/g;
const UNDERLINE_REGEX = /<u>(.*?)<\/u>/g;

/** Letters and digits in any script — the intraword test below. */
const WORD_CHAR = /[\p{L}\p{N}]/u;

export interface InlineMark {
	/** Class applied to the content between the delimiters. */
	cls: string;
	/** Opening delimiter range. */
	openFrom: number;
	openTo: number;
	/** Closing delimiter range. */
	closeFrom: number;
	closeTo: number;
	/**
	 * False when the delimiters must stay on screen. See `isIntrawordEmphasis`:
	 * the construct is real, but hiding it would look like eaten characters.
	 */
	hide: boolean;
}

/**
 * Single `*` emphasis wedged between two word characters is nearly always
 * arithmetic or a glob, not italics — `2*3 and 4*5` parses as emphasis under
 * CommonMark and would render as "23 and 45" with the asterisks erased, which
 * reads as the editor eating text rather than formatting it.
 *
 * Only single-asterisk emphasis is treated this way. `un**bold**ing` is a
 * deliberate construction and stays hidden; the accidental case is `*`, never
 * `**`, because `**` requires someone to type four asterisks by mistake.
 *
 * Underscores need no such guard — CommonMark already refuses intraword `_`,
 * so `snake_case_name` never parses as emphasis in the first place.
 */
function isIntrawordEmphasis(state: EditorState, from: number, to: number): boolean {
	const before = from > 0 ? state.sliceDoc(from - 1, from) : '';
	const after = to < state.doc.length ? state.sliceDoc(to, to + 1) : '';
	return WORD_CHAR.test(before) && WORD_CHAR.test(after);
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
 * `from`/`to` should be a viewport range for decoration work, or a small window
 * around the caret for atomicity work — this walks the syntax tree, so keeping
 * the range tight matters on long documents.
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

			const hide =
				!(node.name === 'Emphasis' && delim === 1 &&
					isIntrawordEmphasis(state, node.from, node.to));

			marks.push({
				cls,
				openFrom: node.from,
				openTo: innerFrom,
				closeFrom: innerTo,
				closeTo: node.to,
				hide,
			});
		},
	});

	// Line-scanned constructs. Lezer has no node for either, so they are matched
	// per line the way live-preview always has.
	const startLine = state.doc.lineAt(from).number;
	const endLine = state.doc.lineAt(Math.min(to, state.doc.length)).number;

	for (let lineNum = startLine; lineNum <= endLine; lineNum++) {
		const line = state.doc.line(lineNum);

		UNDERLINE_REGEX.lastIndex = 0;
		for (let m = UNDERLINE_REGEX.exec(line.text); m !== null; m = UNDERLINE_REGEX.exec(line.text)) {
			const at = line.from + m.index;
			marks.push({
				cls: 'cm-underline',
				openFrom: at,
				openTo: at + 3,
				closeFrom: at + 3 + m[1].length,
				closeTo: at + m[0].length,
				hide: true,
			});
		}

		HIGHLIGHT_REGEX.lastIndex = 0;
		for (let m = HIGHLIGHT_REGEX.exec(line.text); m !== null; m = HIGHLIGHT_REGEX.exec(line.text)) {
			const at = line.from + m.index;
			marks.push({
				cls: 'cm-highlight',
				openFrom: at,
				openTo: at + 2,
				closeFrom: at + 2 + m[1].length,
				closeTo: at + m[0].length,
				hide: true,
			});
		}
	}

	marks.sort((a, b) => a.openFrom - b.openFrom);
	return marks;
}

/** Marker value; atomicRanges only cares about the range bounds. */
class AtomValue extends RangeValue {}
const ATOM = new AtomValue();

/**
 * How far either side of the viewport to look for marks when computing atoms.
 * The caret is always in view, so the viewport plus a little slack covers every
 * position the user can actually reach.
 */
const ATOM_MARGIN = 2000;

/**
 * Hidden delimiters, handed to CodeMirror as atoms.
 *
 * This is what makes Left/Right step over an invisible `**` in one press
 * instead of two dead ones, and what makes Backspace at the edge of bold text
 * remove the whole delimiter rather than half of it.
 */
export const inlineMarkAtoms: Extension = EditorView.atomicRanges.of((view) => {
	const { state } = view;
	const from = Math.max(0, view.viewport.from - ATOM_MARGIN);
	const to = Math.min(state.doc.length, view.viewport.to + ATOM_MARGIN);

	const ranges: Range<AtomValue>[] = [];
	for (const mark of inlineMarks(state, from, to)) {
		if (!mark.hide) continue;
		ranges.push(ATOM.range(mark.openFrom, mark.openTo));
		ranges.push(ATOM.range(mark.closeFrom, mark.closeTo));
	}
	ranges.sort((a, b) => a.from - b.from);
	return RangeSet.of(ranges, true);
});
