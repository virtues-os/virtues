/**
 * Slash Commands Plugin for ProseMirror
 *
 * Provides "/" command palette functionality for inserting blocks:
 * - Detects / character at start of line or after whitespace
 * - Shows command menu at cursor position
 * - Filters commands as user types
 * - Inserts selected block type
 *
 * Pattern follows entity-picker.ts - plugin handles logic, UI is a dumb display.
 */

import { Plugin, PluginKey, TextSelection } from 'prosemirror-state';
import type { EditorState, Transaction } from 'prosemirror-state';
import type { EditorView } from 'prosemirror-view';
import { Decoration, DecorationSet } from 'prosemirror-view';
import { setBlockType, wrapIn } from 'prosemirror-commands';
import { wrapInList } from 'prosemirror-schema-list';
import { schema } from '../schema';

// =============================================================================
// PLUGIN KEY
// =============================================================================

export const slashMenuKey = new PluginKey<SlashMenuState>('slashMenu');

// =============================================================================
// TYPES
// =============================================================================

export interface SlashMenuState {
	active: boolean;
	from: number; // Position where / was typed
	query: string; // Text after /
}

export interface SlashCommand {
	id: string;
	label: string;
	description: string;
	icon: string;
	group: string;
	keywords?: string[];
	execute: (view: EditorView) => boolean;
}

// =============================================================================
// PLUGIN STATE
// =============================================================================

const initialState: SlashMenuState = {
	active: false,
	from: 0,
	query: '',
};

// =============================================================================
// COMMANDS
// =============================================================================

/**
 * Get all available slash commands
 */
export function getSlashCommands(): SlashCommand[] {
	return [
		// Text
		{
			id: 'paragraph',
			label: 'Text',
			description: 'Plain text paragraph',
			icon: 'ri:text',
			group: 'Basic',
			keywords: ['paragraph', 'text', 'plain'],
			execute: (view) => {
				return setBlockType(schema.nodes.paragraph)(view.state, view.dispatch);
			},
		},
		{
			id: 'heading1',
			label: 'Heading 1',
			description: 'Large section heading',
			icon: 'ri:h-1',
			group: 'Basic',
			keywords: ['h1', 'title', 'header'],
			execute: (view) => {
				return setBlockType(schema.nodes.heading, { level: 1 })(view.state, view.dispatch);
			},
		},
		{
			id: 'heading2',
			label: 'Heading 2',
			description: 'Medium section heading',
			icon: 'ri:h-2',
			group: 'Basic',
			keywords: ['h2', 'subtitle', 'header'],
			execute: (view) => {
				return setBlockType(schema.nodes.heading, { level: 2 })(view.state, view.dispatch);
			},
		},
		{
			id: 'heading3',
			label: 'Heading 3',
			description: 'Small section heading',
			icon: 'ri:h-3',
			group: 'Basic',
			keywords: ['h3', 'subheading', 'header'],
			execute: (view) => {
				return setBlockType(schema.nodes.heading, { level: 3 })(view.state, view.dispatch);
			},
		},

		// Lists
		{
			id: 'bullet_list',
			label: 'Bullet List',
			description: 'Unordered list with bullets',
			icon: 'ri:list-unordered',
			group: 'Lists',
			keywords: ['ul', 'unordered', 'bullets'],
			execute: (view) => {
				return wrapInList(schema.nodes.bullet_list)(view.state, view.dispatch);
			},
		},
		{
			id: 'ordered_list',
			label: 'Numbered List',
			description: 'Ordered list with numbers',
			icon: 'ri:list-ordered',
			group: 'Lists',
			keywords: ['ol', 'ordered', 'numbers'],
			execute: (view) => {
				return wrapInList(schema.nodes.ordered_list)(view.state, view.dispatch);
			},
		},
		{
			id: 'roman_list',
			label: 'Roman List',
			description: 'Ordered list with Roman numerals',
			icon: 'ri:list-ordered-2',
			group: 'Lists',
			keywords: ['roman', 'numerals', 'i', 'ii', 'iii'],
			execute: (view) => {
				return wrapInList(schema.nodes.ordered_list, { listStyleType: 'upper-roman' })(view.state, view.dispatch);
			},
		},

		// Blocks
		{
			id: 'blockquote',
			label: 'Quote',
			description: 'Blockquote for citations',
			icon: 'ri:double-quotes-l',
			group: 'Blocks',
			keywords: ['quote', 'citation', 'blockquote'],
			execute: (view) => {
				return wrapIn(schema.nodes.blockquote)(view.state, view.dispatch);
			},
		},
		{
			id: 'code_block',
			label: 'Code Block',
			description: 'Preformatted code block',
			icon: 'ri:code-box-line',
			group: 'Blocks',
			keywords: ['code', 'pre', 'programming'],
			execute: (view) => {
				return setBlockType(schema.nodes.code_block)(view.state, view.dispatch);
			},
		},
		{
			id: 'horizontal_rule',
			label: 'Divider',
			description: 'Horizontal line separator',
			icon: 'ri:separator',
			group: 'Blocks',
			keywords: ['hr', 'divider', 'separator', 'line'],
			execute: (view) => {
				const { state, dispatch } = view;
				const hr = schema.nodes.horizontal_rule.create();
				const tr = state.tr.replaceSelectionWith(hr);
				dispatch(tr);
				return true;
			},
		},

		// Media
		{
			id: 'image',
			label: 'Image',
			description: 'Upload an image',
			icon: 'ri:image-line',
			group: 'Media',
			keywords: ['image', 'picture', 'photo', 'upload'],
			execute: (view) => {
				// Trigger file picker via custom event - handled by PageEditor
				const event = new CustomEvent('slash-command-image', {
					bubbles: true,
					detail: { pos: view.state.selection.from },
				});
				view.dom.dispatchEvent(event);
				return true;
			},
		},
		{
			id: 'video',
			label: 'Video',
			description: 'Upload a video',
			icon: 'ri:video-line',
			group: 'Media',
			keywords: ['video', 'movie', 'upload'],
			execute: (view) => {
				// Trigger file picker via custom event - handled by PageEditor
				const event = new CustomEvent('slash-command-video', {
					bubbles: true,
					detail: { pos: view.state.selection.from },
				});
				view.dom.dispatchEvent(event);
				return true;
			},
		},
		{
			id: 'audio',
			label: 'Audio',
			description: 'Upload an audio file',
			icon: 'ri:music-line',
			group: 'Media',
			keywords: ['audio', 'music', 'sound', 'upload'],
			execute: (view) => {
				// Trigger file picker via custom event - handled by PageEditor
				const event = new CustomEvent('slash-command-audio', {
					bubbles: true,
					detail: { pos: view.state.selection.from },
				});
				view.dom.dispatchEvent(event);
				return true;
			},
		},

		// Tables
		{
			id: 'table',
			label: 'Table',
			description: 'Insert a 3x3 table',
			icon: 'ri:table-line',
			group: 'Advanced',
			keywords: ['table', 'grid', 'spreadsheet'],
			execute: (view) => {
				const { state, dispatch } = view;

				// Create a 3x3 table
				const cellContent = schema.nodes.paragraph.create();
				const cells = [
					schema.nodes.table_cell.create(null, cellContent),
					schema.nodes.table_cell.create(null, cellContent),
					schema.nodes.table_cell.create(null, cellContent),
				];
				const headerCells = [
					schema.nodes.table_header.create(null, cellContent),
					schema.nodes.table_header.create(null, cellContent),
					schema.nodes.table_header.create(null, cellContent),
				];
				const headerRow = schema.nodes.table_row.create(null, headerCells);
				const row1 = schema.nodes.table_row.create(null, cells);
				const row2 = schema.nodes.table_row.create(null, [
					schema.nodes.table_cell.create(null, schema.nodes.paragraph.create()),
					schema.nodes.table_cell.create(null, schema.nodes.paragraph.create()),
					schema.nodes.table_cell.create(null, schema.nodes.paragraph.create()),
				]);
				const table = schema.nodes.table.create(null, [headerRow, row1, row2]);

				const tr = state.tr.replaceSelectionWith(table);
				dispatch(tr);
				return true;
			},
		},
	];
}

/**
 * Filter commands by query
 */
export function filterSlashCommands(commands: SlashCommand[], query: string): SlashCommand[] {
	if (!query) return commands;

	const lowerQuery = query.toLowerCase();
	return commands.filter(cmd => {
		const matchLabel = cmd.label.toLowerCase().includes(lowerQuery);
		const matchKeywords = cmd.keywords?.some(kw => kw.includes(lowerQuery));
		return matchLabel || matchKeywords;
	});
}

/**
 * Execute a slash command
 */
export function executeSlashCommand(view: EditorView, command: SlashCommand): boolean {
	const state = slashMenuKey.getState(view.state);
	if (!state?.active) return false;

	const { from } = state;
	const to = view.state.selection.from;

	// Delete the /query text first
	let tr = view.state.tr.delete(from, to);
	view.dispatch(tr);

	// Close the menu
	view.dispatch(view.state.tr.setMeta(slashMenuKey, { type: 'close' }));

	// Execute the command
	command.execute(view);
	view.focus();

	return true;
}

/**
 * Close the slash menu without executing
 */
export function closeSlashMenu(view: EditorView): boolean {
	const state = slashMenuKey.getState(view.state);
	if (!state?.active) return false;

	view.dispatch(view.state.tr.setMeta(slashMenuKey, { type: 'close' }));
	return true;
}

/**
 * Check if slash menu is currently active
 */
export function isSlashMenuActive(state: EditorState): boolean {
	return slashMenuKey.getState(state)?.active ?? false;
}

/**
 * Get current slash menu state
 */
export function getSlashMenuState(state: EditorState): SlashMenuState | null {
	return slashMenuKey.getState(state) ?? null;
}

// =============================================================================
// UTILITY FUNCTIONS
// =============================================================================

/**
 * Get cursor coordinates for positioning the menu
 */
export function getSlashMenuCoords(view: EditorView): { left: number; top: number; bottom: number } | null {
	const { from } = view.state.selection;
	return view.coordsAtPos(from);
}

/**
 * Extract the query text after /
 */
function extractQuery(state: EditorState, from: number): string {
	const to = state.selection.from;
	if (to <= from) return '';
	return state.doc.textBetween(from, to);
}

/**
 * Check if position is valid for slash command (start of line or after whitespace)
 */
function isValidSlashPosition($pos: ReturnType<EditorState['doc']['resolve']>): boolean {
	// Check if at start of text block
	if ($pos.parentOffset === 1) return true;

	// Check character before /
	if ($pos.pos > 1) {
		const charBefore = $pos.doc.textBetween($pos.pos - 2, $pos.pos - 1);
		return charBefore === ' ' || charBefore === '\n';
	}

	return true;
}

/**
 * Check if current node allows slash commands (not code blocks)
 */
function isValidSlashContext($pos: ReturnType<EditorState['doc']['resolve']>): boolean {
	const parent = $pos.parent;
	// Don't allow in code blocks
	if (parent.type.name === 'code_block') return false;
	// Don't allow in table cells (can be confusing)
	// Actually, let's allow it for now
	return true;
}

// =============================================================================
// PLUGIN
// =============================================================================

export interface SlashMenuPluginOptions {
	/**
	 * Callback when slash menu should be shown
	 */
	onOpen?: (coords: { left: number; top: number; bottom: number }, query: string) => void;

	/**
	 * Callback when slash menu should be closed
	 */
	onClose?: () => void;

	/**
	 * Callback when query changes
	 */
	onQueryChange?: (query: string) => void;
}

export function createSlashMenuPlugin(options: SlashMenuPluginOptions = {}) {
	return new Plugin<SlashMenuState>({
		key: slashMenuKey,

		state: {
			init() {
				return initialState;
			},

			apply(tr, state, oldEditorState, newEditorState) {
				// Check for explicit meta actions
				const meta = tr.getMeta(slashMenuKey);
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

				// If not active, check if / was just typed
				if (!state.active) {
					// Only check on text input
					if (!tr.docChanged) return state;

					const { $from } = newEditorState.selection;

					// Check if we just typed /
					if ($from.pos > 0) {
						const charBefore = newEditorState.doc.textBetween($from.pos - 1, $from.pos);
						if (charBefore === '/') {
							// Check if in valid position and context
							if (isValidSlashPosition($from) && isValidSlashContext($from)) {
								const newState = {
									active: true,
									from: $from.pos - 1, // Include the /
									query: '',
								};

								// Notify that menu should open
								setTimeout(() => {
									// We need to get coords from the view, but we don't have access here
									// The parent component will call getSlashMenuCoords
									options.onOpen?.({ left: 0, top: 0, bottom: 0 }, '');
								}, 0);

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

					// Check if / was deleted
					if (state.from >= newEditorState.doc.content.size) {
						options.onClose?.();
						return initialState;
					}

					const slashPos = state.from;
					if (slashPos >= 0 && slashPos < newEditorState.doc.content.size) {
						const charAtFrom = newEditorState.doc.textBetween(slashPos, slashPos + 1);
						if (charAtFrom !== '/') {
							options.onClose?.();
							return initialState;
						}
					}

					// Check for space in query (close menu - user is typing normally)
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
				const state = slashMenuKey.getState(view.state);
				if (!state?.active) return false;

				// Escape closes menu
				if (event.key === 'Escape') {
					closeSlashMenu(view);
					return true;
				}

				// Arrow keys and Enter are handled by the SlashMenu component
				// We don't consume them here so they can bubble up
				return false;
			},

			// Decorations to highlight the /query
			decorations(state) {
				const pluginState = slashMenuKey.getState(state);
				if (!pluginState?.active) return DecorationSet.empty;

				const { from } = pluginState;
				const to = state.selection.from;

				if (from >= to) return DecorationSet.empty;

				const decoration = Decoration.inline(from, to, {
					class: 'pm-slash-command-active',
				});

				return DecorationSet.create(state.doc, [decoration]);
			},
		},
	});
}
