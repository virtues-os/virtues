/**
 * AI Edit Decorations for CodeMirror
 *
 * Provides visual highlighting for AI-proposed edits in the editor.
 * Uses theme CSS variables for consistent appearance across themes.
 */

import { EditorView, Decoration, type DecorationSet } from '@codemirror/view';
import { StateField, StateEffect, RangeSetBuilder } from '@codemirror/state';

/**
 * AI edit range with type indicator
 */
export interface AIEditRange {
	from: number;
	to: number;
	type: 'insert' | 'delete';
}

/**
 * Effect to set AI decoration ranges
 */
export const setAIRanges = StateEffect.define<AIEditRange[]>();

/**
 * Effect to clear all AI decorations
 */
export const clearAIDecorations = StateEffect.define<void>();

// Decoration marks for inserts and deletes
const aiInsertMark = Decoration.mark({ class: 'cm-ai-insert' });
const aiDeleteMark = Decoration.mark({ class: 'cm-ai-delete' });

/**
 * StateField that tracks AI edit decorations
 */
export const aiDecorationField = StateField.define<DecorationSet>({
	create() {
		return Decoration.none;
	},
	update(decorations, tr) {
		// Map decorations through document changes
		decorations = decorations.map(tr.changes);

		for (const effect of tr.effects) {
			if (effect.is(setAIRanges)) {
				const builder = new RangeSetBuilder<Decoration>();
				// Sort ranges by position for proper building
				const sortedRanges = [...effect.value].sort((a, b) => a.from - b.from);
				for (const { from, to, type } of sortedRanges) {
					builder.add(from, to, type === 'insert' ? aiInsertMark : aiDeleteMark);
				}
				decorations = builder.finish();
			}
			if (effect.is(clearAIDecorations)) {
				decorations = Decoration.none;
			}
		}

		return decorations;
	},
	provide: (f) => EditorView.decorations.from(f)
});

/**
 * Theme-aware styles for AI edit decorations
 * Uses CSS variables for consistent appearance across light/dark themes
 */
export const aiDecorationTheme = EditorView.baseTheme({
	'.cm-ai-insert': {
		backgroundColor: 'var(--color-success-subtle)',
		borderBottom: '2px solid var(--color-success)',
		borderRadius: '2px'
	},
	'.cm-ai-delete': {
		backgroundColor: 'var(--color-error-subtle)',
		textDecoration: 'line-through',
		opacity: '0.7'
	}
});

/**
 * CodeMirror extension bundle for AI decorations
 * Include this in your editor extensions to enable AI highlighting
 */
export const aiDecorationExtension = [aiDecorationField, aiDecorationTheme];

/**
 * Helper function to highlight an AI edit in the editor
 */
export function highlightAIEdit(view: EditorView, ranges: AIEditRange[]) {
	view.dispatch({
		effects: setAIRanges.of(ranges)
	});
}

/**
 * Helper function to clear AI highlights from the editor
 */
export function clearAIHighlights(view: EditorView) {
	view.dispatch({
		effects: clearAIDecorations.of()
	});
}
