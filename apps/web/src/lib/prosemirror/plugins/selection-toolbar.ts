/**
 * Selection Toolbar Plugin for ProseMirror
 *
 * Detects text selection and provides coordinates for a floating toolbar.
 * Pattern follows entity-picker.ts - plugin handles logic, UI is dumb display.
 */

import { Plugin, PluginKey } from 'prosemirror-state';
import type { EditorState } from 'prosemirror-state';
import type { EditorView } from 'prosemirror-view';
import type { MarkType } from 'prosemirror-model';
import { toggleMark } from 'prosemirror-commands';
import { schema } from '../schema';

// =============================================================================
// PLUGIN KEY
// =============================================================================

export const selectionToolbarKey = new PluginKey<SelectionToolbarState>('selectionToolbar');

// =============================================================================
// TYPES
// =============================================================================

export interface SelectionToolbarState {
	active: boolean;
	from: number;
	to: number;
}

export interface SelectionToolbarPosition {
	x: number;
	y: number;
}

export interface SelectionToolbarPluginOptions {
	/**
	 * Callback when toolbar should be shown
	 */
	onShow?: (position: SelectionToolbarPosition) => void;

	/**
	 * Callback when toolbar should be hidden
	 */
	onHide?: () => void;

	/**
	 * Debounce time in ms before showing toolbar (prevents flicker)
	 */
	debounceMs?: number;
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/**
 * Check if a mark is active in the current selection.
 * Works for both collapsed cursors (uses stored marks) and ranges.
 */
export function isMarkActive(state: EditorState, markType: MarkType): boolean {
	const { from, $from, to, empty } = state.selection;

	if (empty) {
		// Check if mark is in stored marks or at cursor position
		return !!markType.isInSet(state.storedMarks || $from.marks());
	}

	// Check if mark exists anywhere in selection
	return state.doc.rangeHasMark(from, to, markType);
}

/**
 * Get all active marks in the current selection.
 */
export function getActiveMarks(state: EditorState): {
	strong: boolean;
	em: boolean;
	underline: boolean;
	code: boolean;
	strikethrough: boolean;
	link: boolean;
} {
	return {
		strong: isMarkActive(state, schema.marks.strong),
		em: isMarkActive(state, schema.marks.em),
		underline: isMarkActive(state, schema.marks.underline),
		code: isMarkActive(state, schema.marks.code),
		strikethrough: isMarkActive(state, schema.marks.strikethrough),
		link: isMarkActive(state, schema.marks.link),
	};
}

/**
 * Toggle a mark on the current selection.
 */
export function toggleFormat(view: EditorView, markType: MarkType): boolean {
	return toggleMark(markType)(view.state, view.dispatch);
}

/**
 * Get toolbar position centered above the selection.
 */
export function getSelectionToolbarPosition(view: EditorView): SelectionToolbarPosition | null {
	const { from, to, empty } = view.state.selection;

	if (empty) return null; // No selection

	try {
		// Get coordinates for start and end of selection
		const start = view.coordsAtPos(from);
		const end = view.coordsAtPos(to, -1); // Use -1 to get end of last character

		// Validate coordinates
		if (!start || !end || start.left === 0 && start.top === 0) {
			return null;
		}

		// Center horizontally between start and end
		// Position above the selection (use the top of the first line)
		const x = (start.left + end.left) / 2;
		const y = Math.min(start.top, end.top); // Use topmost point

		return { x, y };
	} catch {
		return null;
	}
}

// =============================================================================
// INITIAL STATE
// =============================================================================

const initialState: SelectionToolbarState = {
	active: false,
	from: 0,
	to: 0,
};

// =============================================================================
// PLUGIN
// =============================================================================

export function createSelectionToolbarPlugin(options: SelectionToolbarPluginOptions = {}) {
	const { debounceMs = 200 } = options;
	let showTimeout: ReturnType<typeof setTimeout> | null = null;
	let lastView: EditorView | null = null;

	return new Plugin<SelectionToolbarState>({
		key: selectionToolbarKey,

		state: {
			init() {
				return initialState;
			},

			apply(tr, state, oldState, newState) {
				// Check for explicit meta actions
				const meta = tr.getMeta(selectionToolbarKey);
				if (meta?.type === 'hide') {
					return initialState;
				}

				const { from, to, empty } = newState.selection;

				// Hide toolbar if selection is empty/collapsed
				if (empty) {
					return initialState;
				}

				// Check if selection is within a code block (don't show toolbar)
				const $from = newState.selection.$from;
				if ($from.parent.type.name === 'code_block') {
					return initialState;
				}

				// Selection exists - return new state
				return {
					active: true,
					from,
					to,
				};
			},
		},

		view(editorView) {
			lastView = editorView;

			return {
				update(view, prevState) {
					lastView = view;
					const pluginState = selectionToolbarKey.getState(view.state);
					const prevPluginState = selectionToolbarKey.getState(prevState);

					// If becoming inactive (selection collapsed), hide immediately and clear timeout
					if (!pluginState?.active) {
						if (showTimeout) {
							clearTimeout(showTimeout);
							showTimeout = null;
						}
						if (prevPluginState?.active) {
							options.onHide?.();
						}
						return;
					}

					// Selection is active - check if we need to show toolbar
					const selectionChanged =
						pluginState.from !== prevPluginState?.from ||
						pluginState.to !== prevPluginState?.to;

					const wasInactive = !prevPluginState?.active;

					if (selectionChanged || wasInactive) {
						// Clear existing timeout and set a new one
						if (showTimeout) {
							clearTimeout(showTimeout);
						}
						showTimeout = setTimeout(() => {
							showTimeout = null;
							// Re-check that selection is still active
							const currentState = selectionToolbarKey.getState(view.state);
							if (currentState?.active) {
								const position = getSelectionToolbarPosition(view);
								if (position) {
									options.onShow?.(position);
								}
							}
						}, debounceMs);
					}
				},

				destroy() {
					if (showTimeout) {
						clearTimeout(showTimeout);
						showTimeout = null;
					}
					lastView = null;
				},
			};
		},

		props: {
			// Handle blur to hide toolbar when focus leaves editor
			handleDOMEvents: {
				blur(view) {
					// Small delay to allow clicking toolbar buttons
					setTimeout(() => {
						if (showTimeout) {
							clearTimeout(showTimeout);
							showTimeout = null;
						}
						// Don't hide immediately - let the component handle it
						// based on whether the click was on the toolbar
					}, 100);
					return false;
				},
			},
		},
	});
}

/**
 * Hide the selection toolbar programmatically.
 */
export function hideSelectionToolbar(view: EditorView): void {
	view.dispatch(view.state.tr.setMeta(selectionToolbarKey, { type: 'hide' }));
}

/**
 * Check if the selection toolbar is active.
 */
export function isSelectionToolbarActive(state: EditorState): boolean {
	return selectionToolbarKey.getState(state)?.active ?? false;
}
