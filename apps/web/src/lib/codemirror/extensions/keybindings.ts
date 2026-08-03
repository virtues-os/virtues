/**
 * Markdown Keybindings
 *
 * Keyboard shortcuts for markdown formatting:
 * Mod-b: bold, Mod-i: italic, Mod-e: code, etc.
 */

import { Prec, type Extension } from '@codemirror/state';
import { keymap, type EditorView } from '@codemirror/view';

/**
 * Toggle a markdown wrapper around the selection (e.g., ** for bold)
 */
function toggleWrapper(view: EditorView, wrapper: string): boolean {
	const { from, to } = view.state.selection.main;

	if (from === to) {
		// No selection — insert wrapper pair and place cursor inside
		view.dispatch({
			changes: { from, insert: `${wrapper}${wrapper}` },
			selection: { anchor: from + wrapper.length },
		});
		return true;
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
	const { from, to } = view.state.selection.main;
	const openTag = `<${tag}>`;
	const closeTag = `</${tag}>`;

	if (from === to) {
		view.dispatch({
			changes: { from, insert: `${openTag}${closeTag}` },
			selection: { anchor: from + openTag.length },
		});
		return true;
	}

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
 * Bindings that must beat the defaults. `defaultKeymap` claims Backspace, and
 * it is installed ahead of this module in editor.ts, so the heading rule only
 * gets a look in at raised precedence.
 */
const highPrecedenceKeybindings: Extension = Prec.high(
	keymap.of([{ key: 'Backspace', run: unwrapHeadingOnBackspace }]),
);

const formattingKeybindings: Extension = keymap.of([
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
]);

export const markdownKeybindings: Extension = [
	highPrecedenceKeybindings,
	formattingKeybindings,
];
