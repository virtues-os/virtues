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

import { type CodeRange, collectCodeRanges, inCode } from './code-context';
import { flanking } from './highlight-parser';

/** `<u>underline</u>` has no Lezer node; it is scanned. */
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
	/**
	 * The closer has not been typed yet: `closeFrom === closeTo`, both the
	 * caret. There is no closing delimiter to hide or reveal, and a renderer
	 * must not build a mark decoration over that empty range.
	 */
	open?: boolean;
}

/**
 * Delimiter length comes from the NODE TYPE, not from peeking at the text.
 * The text lies for nested emphasis: the Emphasis node of `***both***` spans
 * text that starts with `**`, but its own delimiter is one `*` — guessing
 * from the text mis-measured every mark in that construct, so toggling
 * italic off deleted a `*` of the bold's, and the reveal dimmed the wrong
 * characters. Inline code is the one type with a variable delimiter
 * (` `` ` fences code containing a backtick), measured from the text where
 * it cannot be ambiguous.
 */
function delimiterLength(nodeName: string, text: string): number {
	switch (nodeName) {
		case 'StrongEmphasis':
		case 'Strikethrough':
		case 'Highlight':
			return 2;
		case 'Emphasis':
			return 1;
		case 'InlineCode':
			return /^`+/.exec(text)?.[0].length ?? 0;
		default:
			return 0;
	}
}

const NODE_CLASS: Record<string, string> = {
	StrongEmphasis: 'cm-strong',
	Emphasis: 'cm-emphasis',
	Strikethrough: 'cm-strikethrough',
	InlineCode: 'cm-inline-code',
	Highlight: 'cm-highlight',
};

export interface InlineMarkOptions {
	/**
	 * Pass `view.hasFocus`. With focus, the caret line also carries marks for
	 * constructs still being typed — `**foo|` with no closer yet. Without it
	 * the tree's opinion is the whole answer: an unfocused pane must not hold
	 * a prediction about what a writer who is not there is about to type.
	 */
	focused?: boolean;
}

/**
 * Every inline mark overlapping [from, to), in document order.
 *
 * `from`/`to` should be a viewport range — this walks the syntax tree, so
 * keeping the range tight matters on long documents.
 */
export function inlineMarks(
	state: EditorState,
	from: number,
	to: number,
	options: InlineMarkOptions = {},
): InlineMark[] {
	const marks: InlineMark[] = [];

	syntaxTree(state).iterate({
		from,
		to,
		enter(node) {
			const cls = NODE_CLASS[node.name];
			if (!cls) return;

			const text = state.sliceDoc(node.from, node.to);
			const delim = delimiterLength(node.name, text);
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

	// Underline is the one line-scanned construct left. Lezer has no node for
	// it, so it is matched per line — which also means, unlike the tree-walked
	// marks above, the regex cannot see code context on its own. `inCode`
	// drops matches inside fences and inline code (a `<u>` in a string literal
	// is characters, not an underline).
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
	}

	if (options.focused) {
		const head = state.selection.main.head;
		if (head >= from && head <= to) openConstructMarks(state, head, codeRanges, marks);
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

// ---------------------------------------------------------------------------
// Open constructs — the styling holds while the closer is still to come
//
// With the caret at the end of `**foo |`, Lezer has no StrongEmphasis node:
// there is no closer, and CommonMark's flanking rules would refuse one that
// followed a space anyway. So the walk above sees nothing, and the bold the
// writer is in the middle of typing blinks off on every space until the `**`
// lands. This pass patches the caret line only, only while the editor has
// focus: an unmatched opener before the caret is styled as the construct it
// will become, from the opener to the caret, with the opener revealed and
// dimmed instead of hidden. The moment the caret leaves the line, the tree's
// opinion is the whole answer again — the prediction never outlives the
// keystroke that would confirm it.
//
// What counts as an opener is CommonMark's rule, not a looser one: a run
// that is left-flanking (`*` needs a non-space after it; `_` additionally
// refuses to open inside a word, so `snake_case` never flashes italic).
// Delimiters the parser already owns — the `**` of a closed `**bold**`, or
// anything inside a code span or fence — are never candidates, so a closed
// construct is never marked twice and code is never marked at all.
// ---------------------------------------------------------------------------

interface Run {
	/** Line-relative range of the whole run of one delimiter character. */
	from: number;
	to: number;
	char: string;
}

/** A logical delimiter carved out of a run: `***` is an em `*` around a strong `**`. */
interface Delim {
	from: number;
	to: number;
	char: string;
	cls: string;
	canOpen: boolean;
	canClose: boolean;
	consumed: boolean;
}

const RUN_REGEX = /\*+|_+|~+|=+|`+/g;

function classFor(char: string, length: number): string | null {
	switch (char) {
		case '*':
		case '_':
			return length === 1 ? 'cm-emphasis' : length === 2 ? 'cm-strong' : null;
		case '~':
			return length === 2 ? 'cm-strikethrough' : null;
		case '=':
			return length === 2 ? 'cm-highlight' : null;
		case '`':
			return 'cm-inline-code';
		default:
			return null;
	}
}

/**
 * Split a run into the delimiters it stands for. `*`/`_` runs of three are
 * emphasis around strong — the outer `*` is the Emphasis node's delimiter in
 * `***both***`, matching how the tree measures the closed form. Runs the
 * parser could never use (`====`, `****`) yield nothing.
 */
function delimitersOf(run: Run, text: string): Delim[] {
	const length = run.to - run.from;
	const { char } = run;
	const before = run.from > 0 ? text[run.from - 1] : '';
	const after = run.to < text.length ? text[run.to] : '';

	if (char === '`') {
		// Code spans have no flanking rules; a backtick run of any length opens
		// and closes, matched by length.
		return [{ from: run.from, to: run.to, char, cls: 'cm-inline-code', canOpen: true, canClose: true, consumed: false }];
	}

	const f = flanking(before, after);
	// Underscore emphasis does not open or close intra-word (spec 6.2, rule 2).
	const canOpen = char === '_' ? f.left && (!f.right || f.punctBefore) : f.left;
	const canClose = char === '_' ? f.right && (!f.left || f.punctAfter) : f.right;
	// A run that can do neither is still returned: the pairing walk ignores it,
	// but it is the closer-after-a-space the writer is typing inside of.

	const one = (from: number, to: number): Delim | null => {
		const cls = classFor(char, to - from);
		return cls ? { from, to, char, cls, canOpen, canClose, consumed: false } : null;
	};

	if ((char === '*' || char === '_') && length === 3) {
		// An opener is em-then-strong reading left to right; a pure closer is
		// strong-then-em, so the inner construct closes first as in the tree.
		const closing = canClose && !canOpen;
		const a = one(run.from, closing ? run.from + 2 : run.from + 1);
		const b = one(closing ? run.from + 2 : run.from + 1, run.to);
		return a && b ? [a, b] : [];
	}
	const single = one(run.from, run.to);
	return single ? [single] : [];
}

function openConstructMarks(state: EditorState, head: number, codeRanges: CodeRange[], marks: InlineMark[]): void {
	const line = state.doc.lineAt(head);
	const text = line.text;
	const caret = head - line.from;

	// Delimiter runs on the caret line that neither a parsed construct's
	// delimiters nor a code region own.
	const delims: Delim[] = [];
	RUN_REGEX.lastIndex = 0;
	for (let m = RUN_REGEX.exec(text); m !== null; m = RUN_REGEX.exec(text)) {
		const from = line.from + m.index;
		const to = from + m[0].length;
		if (inCode(codeRanges, from, to)) continue;
		if (marks.some((k) => (from < k.openTo && to > k.openFrom) || (from < k.closeTo && to > k.closeFrom))) continue;
		delims.push(...delimitersOf({ from: m.index, to: m.index + m[0].length, char: m[0][0] }, text));
	}

	// Pair delimiters the way CommonMark would: a closer takes the nearest
	// unmatched opener of its kind. Whatever pairs here and encloses the caret
	// is a construct the parser declined (a `**` closer after a space, say),
	// and gets styled anyway while the caret is inside it — that is the space-
	// bar flicker this pass exists to stop. Whatever is left open is the case
	// the pass is named for.
	const stack: Delim[] = [];
	const paired: Array<[Delim, Delim]> = [];
	for (const d of delims) {
		if (d.canClose) {
			let i = stack.length - 1;
			while (i >= 0 && !(stack[i].char === d.char && stack[i].to - stack[i].from === d.to - d.from)) i--;
			if (i >= 0) {
				const opener = stack[i];
				stack.splice(i);
				opener.consumed = d.consumed = true;
				paired.push([opener, d]);
				continue;
			}
		}
		if (d.canOpen) stack.push(d);
	}

	const push = (opener: Delim, closeFrom: number, closeTo: number, open: boolean) => {
		if (opener.to >= closeFrom) return; // empty content: nothing to style yet
		marks.push({
			cls: opener.cls,
			openFrom: line.from + opener.from,
			openTo: line.from + opener.to,
			closeFrom: line.from + closeFrom,
			closeTo: line.from + closeTo,
			...(open ? { open: true } : {}),
		});
	};

	for (const [opener, closer] of paired) {
		if (opener.from < caret && caret <= closer.to) push(opener, closer.from, closer.to, false);
	}

	for (const opener of stack) {
		if (opener.to > caret) continue;
		// A same-kind run after the caret that nothing claimed is the closer the
		// writer already typed and is now typing inside of (`**foo |bar **`).
		const closer = delims.find(
			(d) => !d.consumed && d.from >= caret && d.char === opener.char && d.to - d.from === opener.to - opener.from,
		);
		if (closer) {
			closer.consumed = true;
			push(opener, closer.from, closer.to, false);
		} else {
			push(opener, caret, caret, true);
		}
	}
}
