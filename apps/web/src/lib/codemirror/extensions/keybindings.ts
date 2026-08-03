/**
 * Markdown Keybindings
 *
 * Keyboard shortcuts for markdown formatting:
 * Mod-b: bold, Mod-i: italic, Mod-e: code, etc.
 */

import { Prec, type Extension } from '@codemirror/state';
import { keymap, type EditorView } from '@codemirror/view';

import { armedFormatting, armFormat } from './armed-format';
import { inlineMarks } from './inline-marks';

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
	const changes: { from: number; insert: string }[] = [];

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
		changes.push({ from: seg.from, insert: wrapper });
		changes.push({ from: seg.to, insert: wrapper });
	}

	if (changes.length === 0) return true;
	view.dispatch({ changes });
	return true;
}

/**
 * Toggle a markdown wrapper around the selection (e.g., ** for bold)
 */
function toggleWrapper(view: EditorView, wrapper: string): boolean {
	const selection = view.state.selection.main;

	if (selection.empty) {
		// Nothing selected — arm the format for the next character rather than
		// writing an empty `****`, which the never-reveal rule would render as
		// nothing at all. See armed-format.ts.
		view.dispatch({ effects: armFormat.of({ open: wrapper, close: wrapper }) });
		return true;
	}

	const { from, to } = trimRange(view, selection.from, selection.to);
	// Selecting only whitespace is not a request to format anything.
	if (from >= to) return true;

	if (view.state.doc.lineAt(from).number !== view.state.doc.lineAt(to).number) {
		return wrapEachLine(view, wrapper, from, to);
	}

	const selectedText = view.state.sliceDoc(from, to);
	const beforeText = view.state.sliceDoc(Math.max(0, from - wrapper.length), from);
	const afterText = view.state.sliceDoc(to, Math.min(view.state.doc.length, to + wrapper.length));

	// Check if already wrapped
	if (beforeText === wrapper && afterText === wrapper) {
		// Remove wrapper
		view.dispatch({
			changes: [
				{ from: from - wrapper.length, to: from },
				{ from: to, to: to + wrapper.length },
			],
			selection: { anchor: from - wrapper.length, head: to - wrapper.length },
		});
	} else if (selectedText.startsWith(wrapper) && selectedText.endsWith(wrapper)) {
		// Selection includes wrappers — remove them
		view.dispatch({
			changes: { from, to, insert: selectedText.slice(wrapper.length, -wrapper.length) },
			selection: { anchor: from, head: to - wrapper.length * 2 },
		});
	} else {
		// Add wrapper
		view.dispatch({
			changes: { from, to, insert: `${wrapper}${selectedText}${wrapper}` },
			selection: { anchor: from + wrapper.length, head: to + wrapper.length },
		});
	}

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
		view.dispatch({ effects: armFormat.of({ open: openTag, close: closeTag }) });
		return true;
	}

	// Same trailing-newline trap as toggleWrapper — see trimRange.
	const { from, to } = trimRange(view, selection.from, selection.to);
	if (from >= to) return true;

	const selectedText = view.state.sliceDoc(from, to);
	if (selectedText.startsWith(openTag) && selectedText.endsWith(closeTag)) {
		const inner = selectedText.slice(openTag.length, -closeTag.length);
		view.dispatch({
			changes: { from, to, insert: inner },
			selection: { anchor: from, head: from + inner.length },
		});
	} else {
		view.dispatch({
			changes: { from, to, insert: `${openTag}${selectedText}${closeTag}` },
			selection: { anchor: from + openTag.length, head: to + openTag.length },
		});
	}

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
 * Backspace at the front of bold/italic/code content unwraps the whole mark.
 *
 * Atomic ranges already stop Backspace from splitting a `**` in half, but
 * deleting just the opening delimiter leaves the closing one orphaned — it
 * stops parsing as emphasis and two asterisks appear in the prose out of
 * nowhere. Since neither delimiter is visible, the only coherent reading of
 * "delete the formatting" is to take both.
 */
function unwrapInlineMarkOnBackspace(view: EditorView): boolean {
	const { state } = view;
	const sel = state.selection.main;
	if (!sel.empty) return false;

	const line = state.doc.lineAt(sel.head);
	for (const mark of inlineMarks(state, line.from, line.to)) {
		if (!mark.hide) continue;
		// Only at the very front of the content, where the caret is sitting
		// against the hidden opening delimiter.
		if (sel.head !== mark.openTo) continue;

		view.dispatch({
			changes: [
				{ from: mark.openFrom, to: mark.openTo },
				{ from: mark.closeFrom, to: mark.closeTo },
			],
			selection: { anchor: mark.openFrom },
			userEvent: 'delete.inlinemark',
		});
		return true;
	}
	return false;
}

/**
 * Bindings that must beat the defaults. `defaultKeymap` claims Backspace, and
 * it is installed ahead of this module in editor.ts, so these rules only get a
 * look in at raised precedence.
 */
const highPrecedenceKeybindings: Extension = Prec.high(
	keymap.of([
		{ key: 'Backspace', run: unwrapHeadingOnBackspace },
		{ key: 'Backspace', run: unwrapInlineMarkOnBackspace },
	]),
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
]));

export const markdownKeybindings: Extension = [
	highPrecedenceKeybindings,
	formattingKeybindings,
	armedFormatting,
];
