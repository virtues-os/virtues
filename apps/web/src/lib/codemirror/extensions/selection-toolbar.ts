/**
 * Selection Toolbar Extension
 *
 * Shows a floating formatting toolbar when the user selects text.
 * Communicates position and active marks via callbacks.
 */

import type { Extension } from '@codemirror/state';
import { type EditorView, ViewPlugin, type ViewUpdate } from '@codemirror/view';

export interface SelectionToolbarCallbacks {
	onShow: (coords: { x: number; y: number }, activeFormats: Set<string>) => void;
	onHide: () => void;
}

/**
 * Detect which markdown formatting is active across the selection.
 */
function getActiveFormats(view: EditorView): Set<string> {
	const { from, to } = view.state.selection.main;
	const formats = new Set<string>();
	if (from === to) return formats;

	const text = view.state.sliceDoc(from, to);
	const before2 = view.state.sliceDoc(Math.max(0, from - 2), from);
	const after2 = view.state.sliceDoc(to, Math.min(view.state.doc.length, to + 2));
	const before1 = before2.slice(-1);
	const after1 = after2.slice(0, 1);

	// Check if selection is wrapped with formatting
	if (before2 === '**' && after2.startsWith('**')) formats.add('bold');
	if (before1 === '*' && after1 === '*' && !before2.endsWith('**')) formats.add('italic');
	if (before1 === '`' && after1 === '`') formats.add('code');
	if (before2 === '~~' && after2.startsWith('~~')) formats.add('strikethrough');

	// Also check if selection includes the markers
	if (text.startsWith('**') && text.endsWith('**')) formats.add('bold');
	if (text.startsWith('*') && text.endsWith('*') && !text.startsWith('**')) formats.add('italic');
	if (text.startsWith('`') && text.endsWith('`')) formats.add('code');
	if (text.startsWith('~~') && text.endsWith('~~')) formats.add('strikethrough');
	if (text.startsWith('<u>') && text.endsWith('</u>')) formats.add('underline');

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
