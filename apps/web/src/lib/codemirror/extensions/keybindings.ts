/**
 * Markdown Keybindings
 *
 * Keyboard shortcuts for markdown formatting:
 * Mod-b: bold, Mod-i: italic, Mod-e: code, etc.
 */

import { type Extension } from '@codemirror/state';
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

export const markdownKeybindings: Extension = keymap.of([
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
]);
