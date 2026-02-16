/**
 * Entity Picker Extension
 *
 * Detects `@` after whitespace/line start by comparing cursor position
 * before and after document changes. This naturally filters out remote
 * Yjs sync (which doesn't move the local cursor to right after the insertion).
 */

import type { Extension } from '@codemirror/state';
import { type EditorView, ViewPlugin, type ViewUpdate } from '@codemirror/view';

export interface EntityPickerCallbacks {
	onOpen: (coords: { x: number; y: number }, from: number) => void;
	onClose: () => void;
	onQueryChange: (query: string) => void;
}

interface PickerState {
	active: boolean;
	from: number; // position of @
	query: string;
}

/**
 * Insert an entity link at the current picker position and close the picker.
 * Inserts markdown: [@Label](/entity-type/id)
 */
export function insertEntity(
	view: EditorView,
	from: number,
	label: string,
	href: string,
): void {
	const to = view.state.selection.main.head;
	const insert = `[@${label}](${href}) `;
	view.dispatch({
		changes: { from, to, insert },
		selection: { anchor: from + insert.length },
	});
	view.focus();
}

/**
 * Create the entity picker extension.
 *
 * Detection strategy: compare cursor position before/after each update.
 * If head moved forward by exactly 1 and that character is `@`, the user
 * just typed it locally. Remote Yjs changes don't move the local cursor
 * to right after the insertion, so this naturally filters them out.
 */
export function createEntityPicker(callbacks: EntityPickerCallbacks): Extension {
	let state: PickerState = { active: false, from: 0, query: '' };

	function close() {
		if (state.active) {
			state = { active: false, from: 0, query: '' };
			callbacks.onClose();
		}
	}

	return ViewPlugin.fromClass(
		class {
			update(update: ViewUpdate) {
				const { head } = update.state.selection.main;

				if (!state.active) {
					if (!update.docChanged) return;

					// Detect: cursor moved forward by exactly 1 char, and that char is @
					const oldHead = update.startState.selection.main.head;
					if (head !== oldHead + 1) return;

					const typed = update.state.sliceDoc(head - 1, head);
					if (typed !== '@') return;

					// Must be at line start or after whitespace
					const atPos = head - 1;
					if (atPos > 0) {
						const charBefore = update.state.sliceDoc(atPos - 1, atPos);
						if (charBefore && !/\s/.test(charBefore)) return;
					}

					state = { active: true, from: atPos, query: '' };
					// coordsAtPos can't be called during update — defer to after layout
					const v = update.view;
					requestAnimationFrame(() => {
						const coords = v.coordsAtPos(atPos);
						if (coords) {
							callbacks.onOpen({ x: coords.left, y: coords.bottom }, atPos);
						}
					});
				} else {
					if (!update.docChanged && !update.selectionSet) return;

					const { from } = state;

					if (head <= from) {
						close();
						return;
					}

					if (from >= update.state.doc.length || update.state.sliceDoc(from, from + 1) !== '@') {
						close();
						return;
					}

					const query = update.state.sliceDoc(from + 1, head);

					if (/[\s\n]/.test(query)) {
						close();
						return;
					}

					if (query !== state.query) {
						state.query = query;
						callbacks.onQueryChange(query);
					}
				}
			}

			destroy() {
				close();
			}
		},
	);
}
