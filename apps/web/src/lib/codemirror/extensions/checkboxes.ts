/**
 * Checkbox Decorations
 *
 * Detects `- [ ]` and `- [x]` patterns and renders interactive checkboxes.
 * Clicking a checkbox toggles the character in the document.
 *
 * On non-active lines: hides `- [ ] ` entirely and shows just the checkbox (GH-like).
 * On the active line: shows the raw `- ` but replaces `[ ]` with a checkbox.
 */

import type { Extension, Range } from '@codemirror/state';
import { Decoration, type DecorationSet, type EditorView, ViewPlugin, type ViewUpdate, WidgetType } from '@codemirror/view';

const CHECKBOX_REGEX = /^(\s*)-\s+\[([ xX])\]/;

class CheckboxWidget extends WidgetType {
	constructor(private checked: boolean) {
		super();
	}

	toDOM(view: EditorView) {
		const checkbox = document.createElement('input');
		checkbox.type = 'checkbox';
		checkbox.className = 'cm-checkbox';
		checkbox.checked = this.checked;

		checkbox.addEventListener('mousedown', (e) => {
			e.preventDefault();
			// Re-derive position from DOM at click time (handles remote edits shifting positions)
			const pos = view.posAtDOM(checkbox);
			const line = view.state.doc.lineAt(pos);
			const match = line.text.match(CHECKBOX_REGEX);
			if (!match) return;
			const charPos = line.from + match[0].length - 2; // the space/x inside [ ]
			const newChar = this.checked ? ' ' : 'x';
			view.dispatch({
				changes: { from: charPos, to: charPos + 1, insert: newChar },
			});
		});

		return checkbox;
	}

	eq(other: CheckboxWidget) {
		return other.checked === this.checked;
	}

	ignoreEvent() {
		return false;
	}
}

function buildCheckboxDecorations(view: EditorView): DecorationSet {
	const builder: Range<Decoration>[] = [];
	const doc = view.state.doc;
	const { from: vpFrom, to: vpTo } = view.viewport;

	// Active-line detection for GH-like checkbox display
	const cursorLine = doc.lineAt(view.state.selection.main.head).number;

	const startLine = doc.lineAt(vpFrom).number;
	const endLine = doc.lineAt(Math.min(vpTo, doc.length)).number;

	for (let lineNum = startLine; lineNum <= endLine; lineNum++) {
		const line = doc.line(lineNum);
		const match = line.text.match(CHECKBOX_REGEX);
		if (!match) continue;

		const indent = match[1].length;
		const checked = match[2].toLowerCase() === 'x';
		const isActiveLine = lineNum === cursorLine;

		if (isActiveLine) {
			// Active line: keep `- ` visible, replace only `[ ]` with checkbox
			const bracketFrom = line.from + indent + 2; // position of [
			const bracketTo = bracketFrom + 3; // position after ]

			builder.push(
				Decoration.replace({
					widget: new CheckboxWidget(checked),
					inclusive: false,
				}).range(bracketFrom, bracketTo)
			);
		} else {
			// Non-active line: hide `- [ ] ` entirely, show just checkbox
			const dashFrom = line.from + indent; // position of -
			let hideEnd = line.from + match[0].length; // position after ]
			// Also hide trailing space after ]
			if (hideEnd < doc.length && view.state.sliceDoc(hideEnd, hideEnd + 1) === ' ') {
				hideEnd += 1;
			}

			builder.push(
				Decoration.replace({
					widget: new CheckboxWidget(checked),
					inclusive: false,
				}).range(dashFrom, hideEnd)
			);
		}
	}

	builder.sort((a, b) => a.from - b.from);
	return Decoration.set(builder);
}

const checkboxPlugin = ViewPlugin.fromClass(
	class {
		decorations: DecorationSet;

		constructor(view: EditorView) {
			this.decorations = buildCheckboxDecorations(view);
		}

		update(update: ViewUpdate) {
			if (update.docChanged || update.viewportChanged || update.selectionSet) {
				this.decorations = buildCheckboxDecorations(update.view);
			}
		}
	},
	{
		decorations: (v) => v.decorations,
	}
);

export const checkboxes: Extension = checkboxPlugin;
