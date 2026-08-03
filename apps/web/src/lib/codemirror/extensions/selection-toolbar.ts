/**
 * Selection Toolbar Extension
 *
 * Shows a floating formatting toolbar when the user selects text.
 * Communicates position and active marks via callbacks.
 */

import type { Extension } from '@codemirror/state';
import { type EditorView, ViewPlugin, type ViewUpdate } from '@codemirror/view';

import { inlineMarks } from './inline-marks';

export interface SelectionToolbarCallbacks {
	onShow: (coords: { x: number; y: number }, activeFormats: Set<string>) => void;
	onHide: () => void;
}

const CLS_TO_FORMAT: Record<string, string> = {
	'cm-strong': 'bold',
	'cm-emphasis': 'italic',
	'cm-inline-code': 'code',
	'cm-strikethrough': 'strikethrough',
	'cm-underline': 'underline',
	'cm-highlight': 'highlight',
};

/**
 * Which formats apply to the selection — answered by inline-marks.ts, the
 * same source the renderer uses.
 *
 * The previous version sniffed two raw characters either side of the
 * selection, so the toolbar's B/I state was only right when the selection
 * happened to hug the delimiters exactly: selecting one word inside a longer
 * bold run showed bold as OFF. A format is active when the selection sits
 * inside the construct's content or spans the whole construct.
 */
function getActiveFormats(view: EditorView): Set<string> {
	const { from, to } = view.state.selection.main;
	const formats = new Set<string>();
	if (from === to) return formats;

	const lineFrom = view.state.doc.lineAt(from).from;
	const lineTo = view.state.doc.lineAt(to).to;

	for (const mark of inlineMarks(view.state, lineFrom, lineTo)) {
		const format = CLS_TO_FORMAT[mark.cls];
		if (!format) continue;
		const insideContent = from >= mark.openTo && to <= mark.closeFrom;
		const spansConstruct = from <= mark.openFrom && to >= mark.closeTo;
		if (insideContent || spansConstruct) formats.add(format);
	}

	return formats;
}

/**
 * Create the selection toolbar extension with callbacks.
 */
export function createSelectionToolbar(callbacks: SelectionToolbarCallbacks): Extension {
	let debounceTimer: ReturnType<typeof setTimeout> | null = null;

	return ViewPlugin.fromClass(
		class {
			active = false;

			constructor(_view: EditorView) {}

			update(update: ViewUpdate) {
				if (!update.selectionSet && !update.docChanged) return;

				const { view } = update;
				const { from, to } = view.state.selection.main;

				if (from === to) {
					// Selection collapsed
					this.hide();
					return;
				}

				// Don't show in code blocks
				const line = view.state.doc.lineAt(from);
				const lineText = line.text.trimStart();
				if (lineText.startsWith('```')) {
					this.hide();
					return;
				}

				// Debounce to prevent flicker during rapid selection changes
				if (debounceTimer) clearTimeout(debounceTimer);
				debounceTimer = setTimeout(() => {
					const coordsFrom = view.coordsAtPos(from);
					const coordsTo = view.coordsAtPos(to, -1);
					if (!coordsFrom || !coordsTo) return;

					// Position centered above the selection
					const x = (coordsFrom.left + coordsTo.right) / 2;
					const y = Math.min(coordsFrom.top, coordsTo.top);

					const activeFormats = getActiveFormats(view);
					this.active = true;
					callbacks.onShow({ x, y }, activeFormats);
				}, 200);
			}

			hide() {
				if (debounceTimer) {
					clearTimeout(debounceTimer);
					debounceTimer = null;
				}
				if (this.active) {
					this.active = false;
					callbacks.onHide();
				}
			}

			destroy() {
				this.hide();
			}
		},
	);
}
