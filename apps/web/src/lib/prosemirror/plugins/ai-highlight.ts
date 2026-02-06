/**
 * AI Edit Highlight Plugin for ProseMirror
 *
 * Provides visual highlighting for AI-proposed edits.
 * Tracks pending edits by editId and allows accept/reject via decorations.
 */

import { Plugin, PluginKey } from 'prosemirror-state';
import type { EditorState, Transaction } from 'prosemirror-state';
import { Decoration, DecorationSet } from 'prosemirror-view';

// =============================================================================
// PLUGIN KEY
// =============================================================================

export const aiHighlightKey = new PluginKey<AIHighlightState>('aiHighlight');

// =============================================================================
// TYPES
// =============================================================================

export interface AIHighlightRange {
	editId: string;
	from: number;
	to: number;
}

interface AIHighlightState {
	ranges: AIHighlightRange[];
}

// =============================================================================
// COMMANDS
// =============================================================================

/**
 * Add a highlight range for an AI edit
 */
export function addAIHighlight(
	editId: string,
	from: number,
	to: number
): (state: EditorState, dispatch?: (tr: Transaction) => void) => boolean {
	return (state, dispatch) => {
		if (dispatch) {
			const tr = state.tr.setMeta(aiHighlightKey, {
				type: 'add',
				range: { editId, from, to }
			});
			dispatch(tr);
		}
		return true;
	};
}

/**
 * Remove a highlight by editId
 */
export function removeAIHighlight(
	editId: string
): (state: EditorState, dispatch?: (tr: Transaction) => void) => boolean {
	return (state, dispatch) => {
		if (dispatch) {
			const tr = state.tr.setMeta(aiHighlightKey, {
				type: 'remove',
				editId
			});
			dispatch(tr);
		}
		return true;
	};
}

/**
 * Clear all AI highlights
 */
export function clearAllAIHighlights(
	state: EditorState,
	dispatch?: (tr: Transaction) => void
): boolean {
	if (dispatch) {
		const tr = state.tr.setMeta(aiHighlightKey, { type: 'clear' });
		dispatch(tr);
	}
	return true;
}

/**
 * Get the current highlight ranges
 */
export function getAIHighlights(state: EditorState): AIHighlightRange[] {
	const pluginState = aiHighlightKey.getState(state);
	return pluginState?.ranges ?? [];
}

/**
 * Check if there are any pending AI highlights
 */
export function hasAIHighlights(state: EditorState): boolean {
	return getAIHighlights(state).length > 0;
}

/**
 * Count pending AI highlights
 */
export function countAIHighlights(state: EditorState): number {
	return getAIHighlights(state).length;
}

// =============================================================================
// PLUGIN
// =============================================================================

export const aiHighlightPlugin = new Plugin<AIHighlightState>({
	key: aiHighlightKey,

	state: {
		init(): AIHighlightState {
			return { ranges: [] };
		},

		apply(tr, state): AIHighlightState {
			const meta = tr.getMeta(aiHighlightKey);

			if (meta) {
				if (meta.type === 'add') {
					return {
						ranges: [...state.ranges, meta.range]
					};
				}
				if (meta.type === 'remove') {
					return {
						ranges: state.ranges.filter(r => r.editId !== meta.editId)
					};
				}
				if (meta.type === 'clear') {
					return { ranges: [] };
				}
			}

			// Map ranges through document changes
			if (tr.docChanged) {
				const newRanges = state.ranges
					.map(range => ({
						editId: range.editId,
						from: tr.mapping.map(range.from),
						to: tr.mapping.map(range.to)
					}))
					.filter(range => range.from < range.to); // Remove collapsed ranges

				return { ranges: newRanges };
			}

			return state;
		}
	},

	props: {
		decorations(state) {
			const pluginState = aiHighlightKey.getState(state);
			if (!pluginState || pluginState.ranges.length === 0) {
				return DecorationSet.empty;
			}

			const decorations: Decoration[] = pluginState.ranges.map(range =>
				Decoration.inline(range.from, range.to, {
					class: 'pm-ai-highlight',
					'data-edit-id': range.editId
				})
			);

			return DecorationSet.create(state.doc, decorations);
		}
	}
});

// =============================================================================
// EXPORTS
// =============================================================================

export type { AIHighlightState };
