/**
 * Table Toolbar Plugin for ProseMirror
 *
 * Detects when cursor is inside a table and provides coordinates for a floating toolbar.
 * Uses prosemirror-tables commands for table manipulation.
 */

import { Plugin, PluginKey } from 'prosemirror-state';
import type { EditorState } from 'prosemirror-state';
import type { EditorView } from 'prosemirror-view';
import {
	addRowBefore,
	addRowAfter,
	addColumnBefore,
	addColumnAfter,
	deleteRow,
	deleteColumn,
	deleteTable,
	CellSelection,
	isInTable,
} from 'prosemirror-tables';

// =============================================================================
// PLUGIN KEY
// =============================================================================

export const tableToolbarKey = new PluginKey<TableToolbarState>('tableToolbar');

// =============================================================================
// TYPES
// =============================================================================

export interface TableToolbarState {
	active: boolean;
	tablePos: number; // Position of the table node
}

export interface TableToolbarPosition {
	x: number;
	y: number;
}

export interface TableToolbarPluginOptions {
	/**
	 * Callback when toolbar should be shown
	 */
	onShow?: (position: TableToolbarPosition) => void;

	/**
	 * Callback when toolbar should be hidden
	 */
	onHide?: () => void;

	/**
	 * Debounce time in ms before showing toolbar
	 */
	debounceMs?: number;
}

// =============================================================================
// TABLE COMMANDS
// =============================================================================

export type TableCommand =
	| 'addRowBefore'
	| 'addRowAfter'
	| 'addColumnBefore'
	| 'addColumnAfter'
	| 'deleteRow'
	| 'deleteColumn'
	| 'deleteTable';

/**
 * Execute a table command on the editor.
 */
export function executeTableCommand(view: EditorView, command: TableCommand): boolean {
	const commands: Record<TableCommand, (state: EditorState, dispatch?: (tr: any) => void) => boolean> = {
		addRowBefore,
		addRowAfter,
		addColumnBefore,
		addColumnAfter,
		deleteRow,
		deleteColumn,
		deleteTable,
	};

	const cmd = commands[command];
	if (cmd) {
		return cmd(view.state, view.dispatch);
	}
	return false;
}

/**
 * Check if a table command can be executed.
 */
export function canExecuteTableCommand(state: EditorState, command: TableCommand): boolean {
	const commands: Record<TableCommand, (state: EditorState) => boolean> = {
		addRowBefore,
		addRowAfter,
		addColumnBefore,
		addColumnAfter,
		deleteRow,
		deleteColumn,
		deleteTable,
	};

	const cmd = commands[command];
	if (cmd) {
		return cmd(state);
	}
	return false;
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/**
 * Find the table node containing the current selection.
 * Returns the position of the table node, or null if not in a table.
 */
function findTablePos(state: EditorState): number | null {
	const { $from } = state.selection;

	// Walk up the tree to find a table
	for (let depth = $from.depth; depth > 0; depth--) {
		const node = $from.node(depth);
		if (node.type.name === 'table') {
			return $from.before(depth);
		}
	}

	return null;
}

/**
 * Get the position for the table toolbar (above the table).
 */
export function getTableToolbarPosition(view: EditorView): TableToolbarPosition | null {
	const tablePos = findTablePos(view.state);
	if (tablePos === null) return null;

	try {
		// Get the DOM element for the table
		const tableNode = view.nodeDOM(tablePos) as HTMLElement | null;
		if (!tableNode) return null;

		const rect = tableNode.getBoundingClientRect();

		// Position centered above the table
		return {
			x: rect.left + rect.width / 2,
			y: rect.top,
		};
	} catch {
		return null;
	}
}

// =============================================================================
// INITIAL STATE
// =============================================================================

const initialState: TableToolbarState = {
	active: false,
	tablePos: -1,
};

// =============================================================================
// PLUGIN
// =============================================================================

export function createTableToolbarPlugin(options: TableToolbarPluginOptions = {}) {
	const { debounceMs = 100 } = options;
	let showTimeout: ReturnType<typeof setTimeout> | null = null;

	return new Plugin<TableToolbarState>({
		key: tableToolbarKey,

		state: {
			init() {
				return initialState;
			},

			apply(tr, state, oldState, newState) {
				// Check for explicit meta actions
				const meta = tr.getMeta(tableToolbarKey);
				if (meta?.type === 'hide') {
					return initialState;
				}

				// Check if cursor is in a table
				const tablePos = findTablePos(newState);

				if (tablePos === null) {
					return initialState;
				}

				return {
					active: true,
					tablePos,
				};
			},
		},

		view() {
			return {
				update(view, prevState) {
					const pluginState = tableToolbarKey.getState(view.state);
					const prevPluginState = tableToolbarKey.getState(prevState);

					// If becoming inactive, hide immediately
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

					// Check if we moved to a different table or just entered one
					const tableChanged = pluginState.tablePos !== prevPluginState?.tablePos;
					const wasInactive = !prevPluginState?.active;

					if (tableChanged || wasInactive) {
						// Clear existing timeout and set a new one
						if (showTimeout) {
							clearTimeout(showTimeout);
						}
						showTimeout = setTimeout(() => {
							showTimeout = null;
							// Re-check that we're still in a table
							const currentState = tableToolbarKey.getState(view.state);
							if (currentState?.active) {
								const position = getTableToolbarPosition(view);
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
				},
			};
		},
	});
}

/**
 * Hide the table toolbar programmatically.
 */
export function hideTableToolbar(view: EditorView): void {
	view.dispatch(view.state.tr.setMeta(tableToolbarKey, { type: 'hide' }));
}

/**
 * Check if the table toolbar is active.
 */
export function isTableToolbarActive(state: EditorState): boolean {
	return tableToolbarKey.getState(state)?.active ?? false;
}

/**
 * Check if cursor is currently in a table.
 */
export function isCursorInTable(state: EditorState): boolean {
	return isInTable(state);
}
