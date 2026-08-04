/**
 * Markdown Keybindings
 *
 * Keyboard shortcuts for markdown formatting:
 * Mod-b: bold, Mod-i: italic, Mod-e: code, etc.
 */

import { Prec, type Extension } from '@codemirror/state';
import { keymap, type EditorView } from '@codemirror/view';

import { emptyMarkTidy } from './empty-mark-tidy';
import { inlineMarks } from './inline-marks';

/** Wrapper string → the mark class inline-marks.ts reports for it. */
const WRAPPER_CLS: Record<string, string> = {
	'**': 'cm-strong',
	'*': 'cm-emphasis',
	'~~': 'cm-strikethrough',
	'`': 'cm-inline-code',
	'==': 'cm-highlight',
};

/** A list item line: bullet, ordered, or task. */
const LIST_ITEM = /^(\s*)([-*+]|\d+[.)])\s/;

/** One level of list nesting, matching the depth math in live-preview.ts. */
const LIST_INDENT = '  ';

/**
 * Tab / Shift-Tab indent the LIST ITEMS the selection touches, and only list
 * items. On any other line both commands return false, so Tab keeps its
 * accessibility default (moving focus out of the editor) — the binding claims
 * exactly the context where a writer means "nest this bullet" and nothing more.
 *
 * The edit carries userEvent `input.indent` so ordered lists renumber per
 * level in the same transaction (list-renumber gates on input/delete).
 */
function changeListIndent(view: EditorView, direction: 1 | -1): boolean {
	const { state } = view;
	const changes: { from: number; to?: number; insert?: string }[] = [];
	const seen = new Set<number>();

	for (const range of state.selection.ranges) {
		const first = state.doc.lineAt(range.from).number;
		const last = state.doc.lineAt(range.to).number;
		for (let n = first; n <= last; n++) {
			if (seen.has(n)) continue;
			seen.add(n);
			const line = state.doc.line(n);
			if (!LIST_ITEM.test(line.text)) continue;
			if (direction === 1) {
				changes.push({ from: line.from, insert: LIST_INDENT });
			} else {
				const spaces = /^ {1,2}/.exec(line.text)?.[0].length ?? 0;
				if (spaces > 0) changes.push({ from: line.from, to: line.from + spaces });
			}
		}
	}

	if (changes.length === 0) return false;
	view.dispatch({ changes, userEvent: 'input.indent' });
	return true;
}

/**
 * Narrow a range to the text actually worth marking up.
 *
 * A line selected by triple-click, Shift-Down, or Home-then-Shift-Down carries
 * its trailing newline, and wrapping that literally produces `*text\n*` — which
 * is not emphasis in any markdown dialect. It renders as two stray asterisks,
 * one at the end of one line and one at the start of the next, with nothing
 * italic between them, and since delimiters are normally hidden the two that
 * DO show up look like the editor typing punctuation at random.
 *
 * Trimming first is what makes "select the line, press Cmd+I" mean what it
 * looks like it means. It also fixes the smaller everyday version: selecting a
 * word by dragging usually catches the space after it.
 */
function trimRange(view: EditorView, from: number, to: number): { from: number; to: number } {
	let start = from;
	let end = to;
	while (start < end && /\s/.test(view.state.sliceDoc(start, start + 1))) start++;
	while (end > start && /\s/.test(view.state.sliceDoc(end - 1, end))) end--;
	return { from: start, to: end };
}

/**
 * Lines that are structure rather than prose.
 *
 * Emphasis around one of these does not emphasize it, it BREAKS it:
 * `*| Board | Status |*` stops being a table row, `*---*` stops being a rule,
 * and a wrapped fence stops opening a code block. A selection dragged across a
 * table is asking for the prose in it to be marked, not for the table to be
 * dismantled, so these lines are passed over.
 */
const STRUCTURAL_LINE = /^\s*(\||```|~~~|-{3,}\s*$|\*{3,}\s*$|_{3,}\s*$)/;

/**
 * Leading block markers, which must stay OUTSIDE the emphasis.
 *
 * `*# Heading*` is not an emphasized heading — it is a paragraph beginning with
 * an asterisk. Same for `*- item*`. The marker is matched here so the wrapper
 * can be placed after it.
 */
const BLOCK_PREFIX = /^\s*(?:>\s*)*(?:#{1,6}\s+|[-*+]\s+(?:\[[ xX]\]\s+)?|\d+[.)]\s+)?/;

/**
 * Wrap each line's own prose, for a selection spanning several.
 *
 * Emphasis cannot straddle a line break, so a multi-line selection has to
 * become one mark per line rather than one mark around the lot. Every change is
 * an insertion at a distinct position, so offsets stay valid without manual
 * bookkeeping.
 */
function wrapEachLine(view: EditorView, wrapper: string, from: number, to: number): boolean {
	const { doc } = view.state;

	// Pass 1: collect each line's markable segment.
	const segs: { from: number; to: number; wrapped: boolean }[] = [];
	const firstLine = doc.lineAt(from).number;
	const lastLine = doc.lineAt(to).number;

	for (let n = firstLine; n <= lastLine; n++) {
		const line = doc.line(n);
		if (STRUCTURAL_LINE.test(line.text)) continue;

		// Start after any list bullet / heading hashes / quote markers.
		const prefix = BLOCK_PREFIX.exec(line.text)?.[0].length ?? 0;
		const seg = trimRange(
			view,
			Math.max(line.from + prefix, from),
			Math.min(line.to, to),
		);
		if (seg.from >= seg.to) continue; // blank line, or whitespace only

		const text = view.state.sliceDoc(seg.from, seg.to);
		const wrapped =
			text.length > wrapper.length * 2 &&
			text.startsWith(wrapper) &&
			text.endsWith(wrapper);
		segs.push({ ...seg, wrapped });
	}

	if (segs.length === 0) return true;

	// Pass 2: a toggle, like the single-line path. Every segment already
	// wrapped → this press means "take it off"; otherwise wrap the ones that
	// are not (skipping the already-wrapped so a mixed selection cannot end up
	// double-marked — the second press then finds everything wrapped and
	// unwraps uniformly).
	const changes: { from: number; to?: number; insert?: string }[] = [];
	if (segs.every((s) => s.wrapped)) {
		for (const s of segs) {
			changes.push({ from: s.from, to: s.from + wrapper.length });
			changes.push({ from: s.to - wrapper.length, to: s.to });
		}
	} else {
		for (const s of segs) {
			if (s.wrapped) continue;
			changes.push({ from: s.from, insert: wrapper });
			changes.push({ from: s.to, insert: wrapper });
		}
	}

	view.dispatch({ changes });
	return true;
}

/**
 * Toggle a markdown wrapper around the selection (e.g., ** for bold)
 */
function toggleWrapper(view: EditorView, wrapper: string): boolean {
	const selection = view.state.selection.main;

	if (selection.empty) {
		// Nothing selected — insert the pair with the caret inside. Under
		// reveal-on-touch the caret is touching the construct it just created,
		// so all four delimiters are VISIBLE while you type into them; the
		// "invisible caret inside an invisible construct" problem this branch
		// once worked around (via an armed-format mode) no longer exists.
		view.dispatch({
			changes: { from: selection.from, insert: `${wrapper}${wrapper}` },
			selection: { anchor: selection.from + wrapper.length },
		});
		return true;
	}

	let { from, to } = trimRange(view, selection.from, selection.to);
	// Selecting only whitespace is not a request to format anything.
	if (from >= to) return true;

	if (view.state.doc.lineAt(from).number !== view.state.doc.lineAt(to).number) {
		return wrapEachLine(view, wrapper, from, to);
	}

	// Keep block markers OUTSIDE the mark on the single-line path too — a
	// whole-line selection of `- item` must wrap the item, not the bullet.
	// (`**- item**` stops being a list item at all; verified on screen when
	// this guard only existed in the multi-line path.)
	const markLine = view.state.doc.lineAt(from);
	if (STRUCTURAL_LINE.test(markLine.text)) return true;
	const linePrefix = BLOCK_PREFIX.exec(markLine.text)?.[0].length ?? 0;
	if (from < markLine.from + linePrefix) {
		from = Math.min(markLine.from + linePrefix, to);
		({ from, to } = trimRange(view, from, to));
		if (from >= to) return true;
	}

	// Already formatted? Asked of inline-marks.ts rather than answered by
	// peeking at adjacent characters — the peek broke on nesting (`***both***`:
	// unwrapping the italic deleted one of the bold's asterisks) and on any
	// selection that didn't hug the delimiters exactly. A construct counts when
	// the selection sits inside its content or spans it whole; unwrapping
	// deletes that construct's own delimiters, wherever they are.
	const cls = WRAPPER_CLS[wrapper];
	if (cls) {
		const lineEnd = view.state.doc.lineAt(to).to;
		for (const mark of inlineMarks(view.state, markLine.from, lineEnd)) {
			if (mark.cls !== cls) continue;
			const insideContent = from >= mark.openTo && to <= mark.closeFrom;
			const spansConstruct = from <= mark.openFrom && to >= mark.closeTo;
			if (!insideContent && !spansConstruct) continue;

			const openLen = mark.openTo - mark.openFrom;
			view.dispatch({
				changes: [
					{ from: mark.openFrom, to: mark.openTo },
					{ from: mark.closeFrom, to: mark.closeTo },
				],
				selection: {
					anchor: Math.max(mark.openFrom, from - openLen),
					head: Math.min(mark.closeFrom - openLen, to - openLen),
				},
			});
			return true;
		}
	}

	// Not formatted — wrap.
	const selectedText = view.state.sliceDoc(from, to);
	view.dispatch({
		changes: { from, to, insert: `${wrapper}${selectedText}${wrapper}` },
		selection: { anchor: from + wrapper.length, head: to + wrapper.length },
	});

	return true;
}

/**
 * Toggle an HTML tag wrapper (for underline: <u>text</u>)
 */
function toggleHtmlTag(view: EditorView, tag: string): boolean {
	const selection = view.state.selection.main;
	const openTag = `<${tag}>`;
	const closeTag = `</${tag}>`;

	if (selection.empty) {
		view.dispatch({
			changes: { from: selection.from, insert: `${openTag}${closeTag}` },
			selection: { anchor: selection.from + openTag.length },
		});
		return true;
	}

	// Same trailing-newline and block-marker traps as toggleWrapper.
	let { from, to } = trimRange(view, selection.from, selection.to);
	if (from >= to) return true;

	const tagLine = view.state.doc.lineAt(from);
	if (STRUCTURAL_LINE.test(tagLine.text)) return true;
	const tagPrefix = BLOCK_PREFIX.exec(tagLine.text)?.[0].length ?? 0;
	if (from < tagLine.from + tagPrefix) {
		from = Math.min(tagLine.from + tagPrefix, to);
		({ from, to } = trimRange(view, from, to));
		if (from >= to) return true;
	}

	// Already underlined? Same containment rule as toggleWrapper.
	if (tag === 'u') {
		const lineEnd = view.state.doc.lineAt(to).to;
		for (const mark of inlineMarks(view.state, tagLine.from, lineEnd)) {
			if (mark.cls !== 'cm-underline') continue;
			const insideContent = from >= mark.openTo && to <= mark.closeFrom;
			const spansConstruct = from <= mark.openFrom && to >= mark.closeTo;
			if (!insideContent && !spansConstruct) continue;

			const openLen = mark.openTo - mark.openFrom;
			view.dispatch({
				changes: [
					{ from: mark.openFrom, to: mark.openTo },
					{ from: mark.closeFrom, to: mark.closeTo },
				],
				selection: {
					anchor: Math.max(mark.openFrom, from - openLen),
					head: Math.min(mark.closeFrom - openLen, to - openLen),
				},
			});
			return true;
		}
	}

	const selectedText = view.state.sliceDoc(from, to);
	view.dispatch({
		changes: { from, to, insert: `${openTag}${selectedText}${closeTag}` },
		selection: { anchor: from + openTag.length, head: to + openTag.length },
	});

	return true;
}

/**
 * Backspace at the start of a heading unwraps the heading.
 *
 * The `# ` prefix is hidden (see live-preview.ts), so the default
 * deleteCharBackward would swallow one invisible `#` and leave `## Foo` as
 * `# Foo` — or `# Foo` as `Foo` with a stray space — with nothing on screen
 * explaining why the type changed. Deleting the whole marker is what someone
 * pressing Backspace at the front of a heading actually means.
 *
 * This claims the entire hidden prefix, including position 0, because with the
 * marker hidden the line start and the text start render at the same x and the
 * user cannot aim between them. Joining with the previous line is still one
 * more Backspace away, once the line is no longer a heading.
 */
function unwrapHeadingOnBackspace(view: EditorView): boolean {
	const { state } = view;
	const sel = state.selection.main;
	if (!sel.empty) return false;

	const line = state.doc.lineAt(sel.head);
	const marker = /^(#{1,6})([ \t]+)/.exec(line.text);
	if (!marker) return false;

	const markerEnd = line.from + marker[0].length;
	if (sel.head > markerEnd) return false;

	view.dispatch({
		changes: { from: line.from, to: markerEnd },
		selection: { anchor: line.from },
		userEvent: 'delete.heading',
	});
	return true;
}

/**
 * Bindings that must beat the defaults. `defaultKeymap` claims Backspace, and
 * it is installed ahead of this module in editor.ts, so this rule only gets a
 * look in at raised precedence.
 *
 * Only the HEADING gets a special Backspace: its `# ` stays hidden even with
 * the caret on the line (the marker hangs in the margin), so deleting it
 * character-by-character would be deleting the invisible. Inline marks need no
 * such rule — reveal-on-touch means a caret against a `**` can see it, and
 * ordinary Backspace does what it looks like it does.
 */
const highPrecedenceKeybindings: Extension = Prec.high(
	keymap.of([{ key: 'Backspace', run: unwrapHeadingOnBackspace }]),
);

/**
 * Formatting shortcuts.
 *
 * Raised precedence for one specific reason: `defaultKeymap` binds Mod-i to
 * selectParentSyntax with preventDefault, and it is registered ahead of this
 * module, so plain precedence meant Cmd+I selected the enclosing paragraph
 * instead of italicizing — the whole line lighting up rather than four
 * characters going slanted. Mod-i is the only collision (checked against the
 * default list), but the whole set is raised so the next one added here does
 * not silently lose the same way.
 */
const formattingKeybindings: Extension = Prec.high(keymap.of([
	{
		key: 'Mod-b',
		run: (view) => toggleWrapper(view, '**'),
	},
	{
		key: 'Mod-i',
		run: (view) => toggleWrapper(view, '*'),
	},
	{
		key: 'Mod-e',
		run: (view) => toggleWrapper(view, '`'),
	},
	{
		key: 'Mod-`',
		run: (view) => toggleWrapper(view, '`'),
	},
	{
		key: 'Mod-u',
		run: (view) => toggleHtmlTag(view, 'u'),
	},
	{
		key: 'Mod-Shift-s',
		run: (view) => toggleWrapper(view, '~~'),
	},
	{
		key: 'Mod-Shift-x',
		run: (view) => toggleWrapper(view, '~~'),
	},
	{
		// Highlight (==text==)
		key: 'Mod-Shift-h',
		run: (view) => toggleWrapper(view, '=='),
	},
	{
		key: 'Tab',
		run: (view) => changeListIndent(view, 1),
		shift: (view) => changeListIndent(view, -1),
	},
]));

export const markdownKeybindings: Extension = [
	highPrecedenceKeybindings,
	formattingKeybindings,
	emptyMarkTidy,
];
