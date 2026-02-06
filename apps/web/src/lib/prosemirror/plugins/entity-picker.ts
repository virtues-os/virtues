/**
 * Entity Picker Plugin for ProseMirror
 *
 * Provides @ mention functionality for inserting entity links:
 * - Detects @ character input
 * - Dispatches event to show EntityPicker component
 * - Handles entity selection to insert entity_link node
 */

import { Plugin, PluginKey, EditorState, Transaction, TextSelection } from 'prosemirror-state';
import { EditorView, Decoration, DecorationSet } from 'prosemirror-view';
import { schema } from '../schema';

// =============================================================================
// PLUGIN KEY
// =============================================================================

export const entityPickerKey = new PluginKey<EntityPickerState>('entityPicker');

// =============================================================================
// TYPES
// =============================================================================

export interface EntityPickerState {
	active: boolean;
	from: number; // Position where @ was typed
	query: string; // Text after @
}

export interface EntitySelection {
	href: string;
	label: string;
}

// =============================================================================
// PLUGIN STATE
// =============================================================================

const initialState: EntityPickerState = {
	active: false,
	from: 0,
	query: '',
};

// =============================================================================
// COMMANDS
// =============================================================================

/**
 * Insert an entity link at the current @ mention position
 */
export function insertEntity(
	view: EditorView,
	entity: EntitySelection
): boolean {
	const state = entityPickerKey.getState(view.state);
	if (!state?.active) return false;

	const { from } = state;
	const to = view.state.selection.from;

	// Create entity_link node
	const node = schema.nodes.entity_link.create({
		href: entity.href,
		label: entity.label,
	});

	// Create a space text node for the cursor to land on
	const space = schema.text(' ');

	// Replace @query with entity link + space
	const tr = view.state.tr
		.delete(from, to)
		.insert(from, node)
		.insert(from + node.nodeSize, space)
		.setMeta(entityPickerKey, { type: 'close' });

	// Set cursor after the space (inside the text node)
	const newPos = from + node.nodeSize + 1;
	tr.setSelection(TextSelection.create(tr.doc, newPos));

	view.dispatch(tr);
	view.focus();

	return true;
}

/**
 * Close the entity picker without inserting
 */
export function closeEntityPicker(view: EditorView): boolean {
	const state = entityPickerKey.getState(view.state);
	if (!state?.active) return false;

	view.dispatch(view.state.tr.setMeta(entityPickerKey, { type: 'close' }));
	return true;
}

/**
 * Check if entity picker is currently active
 */
export function isEntityPickerActive(state: EditorState): boolean {
	return entityPickerKey.getState(state)?.active ?? false;
}

/**
 * Get current entity picker state
 */
export function getEntityPickerState(state: EditorState): EntityPickerState | null {
	return entityPickerKey.getState(state) ?? null;
}

// =============================================================================
// UTILITY FUNCTIONS
// =============================================================================

/**
 * Get cursor coordinates for positioning the picker
 */
export function getCursorCoords(view: EditorView): { left: number; top: number; bottom: number } | null {
	const { from } = view.state.selection;
	const coords = view.coordsAtPos(from);
	return coords;
}

/**
 * Extract the query text after @
 */
function extractQuery(state: EditorState, from: number): string {
	const to = state.selection.from;
	if (to <= from) return '';

	const text = state.doc.textBetween(from, to);
	return text;
}

// =============================================================================
// PLUGIN
// =============================================================================

export interface EntityPickerPluginOptions {
	/**
	 * Callback when entity picker should be shown
	 */
	onOpen?: (coords: { left: number; top: number; bottom: number }, query: string) => void;

	/**
	 * Callback when entity picker should be closed
	 */
	onClose?: () => void;

	/**
	 * Callback when query changes
	 */
	onQueryChange?: (query: string) => void;
}

export function createEntityPickerPlugin(options: EntityPickerPluginOptions = {}) {
	return new Plugin<EntityPickerState>({
		key: entityPickerKey,

		state: {
			init() {
				return initialState;
			},

			apply(tr, state, oldEditorState, newEditorState) {
				// Check for explicit meta actions
				const meta = tr.getMeta(entityPickerKey);
				if (meta?.type === 'close') {
					if (state.active) {
						options.onClose?.();
					}
					return initialState;
				}
				if (meta?.type === 'open') {
					return {
						active: true,
						from: meta.from,
						query: '',
					};
				}

				// If not active, check if @ was just typed
				if (!state.active) {
					// Only check on text input
					if (!tr.docChanged) return state;

					const { $from } = newEditorState.selection;

					// Check if we just typed @
					if ($from.pos > 0) {
						const charBefore = newEditorState.doc.textBetween($from.pos - 1, $from.pos);
						if (charBefore === '@') {
							// Check character before @ (should be start or whitespace)
							const charBeforeAt = $from.pos > 1
								? newEditorState.doc.textBetween($from.pos - 2, $from.pos - 1)
								: ' ';

							if (charBeforeAt === ' ' || charBeforeAt === '\n' || $from.pos === 1 || $from.parentOffset === 1) {
								const newState = {
									active: true,
									from: $from.pos - 1, // Include the @
									query: '',
								};

								// Notify that picker should open
								const coords = getCursorCoordsFromState(newEditorState, tr);
								if (coords) {
									setTimeout(() => options.onOpen?.(coords, ''), 0);
								}

								return newState;
							}
						}
					}

					return state;
				}

				// If active, update query or close
				if (state.active) {
					// Check if selection moved away
					const { from: selFrom } = newEditorState.selection;
					if (selFrom < state.from) {
						options.onClose?.();
						return initialState;
					}

					// Check if @ was deleted
					if (state.from >= newEditorState.doc.content.size) {
						options.onClose?.();
						return initialState;
					}

					const atPos = state.from;
					if (atPos >= 0 && atPos < newEditorState.doc.content.size) {
						const charAtFrom = newEditorState.doc.textBetween(atPos, atPos + 1);
						if (charAtFrom !== '@') {
							options.onClose?.();
							return initialState;
						}
					}

					// Check for space or invalid chars in query (close picker)
					const query = extractQuery(newEditorState, state.from + 1);
					if (query.includes(' ') || query.includes('\n')) {
						options.onClose?.();
						return initialState;
					}

					// Update query
					if (query !== state.query) {
						options.onQueryChange?.(query);
					}

					return {
						...state,
						query,
					};
				}

				return state;
			},
		},

		props: {
			handleKeyDown(view, event) {
				const state = entityPickerKey.getState(view.state);
				if (!state?.active) return false;

				// Escape closes picker
				if (event.key === 'Escape') {
					closeEntityPicker(view);
					return true;
				}

				// Let parent handle navigation (arrow keys, enter, etc.)
				// These events bubble up to the EntityPicker component
				return false;
			},

			// Decorations to highlight the @mention
			decorations(state) {
				const pluginState = entityPickerKey.getState(state);
				if (!pluginState?.active) return DecorationSet.empty;

				const { from } = pluginState;
				const to = state.selection.from;

				if (from >= to) return DecorationSet.empty;

				const decoration = Decoration.inline(from, to, {
					class: 'pm-entity-mention-active',
				});

				return DecorationSet.create(state.doc, [decoration]);
			},
		},
	});
}

// Helper to get coords without view (approximate)
function getCursorCoordsFromState(
	_state: EditorState,
	_tr: Transaction
): { left: number; top: number; bottom: number } | null {
	// This is a placeholder - actual coords need the view
	// The real coords will be fetched via getCursorCoords() when opening
	return { left: 0, top: 0, bottom: 0 };
}

// =============================================================================
// CSS (add to theme.css)
// =============================================================================

/*
.pm-entity-mention-active {
  background-color: color-mix(in srgb, var(--color-primary) 15%, transparent);
  border-radius: 2px;
}
*/
