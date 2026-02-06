/**
 * Input Rules Plugin for ProseMirror
 *
 * Provides markdown-style auto-conversion:
 * - **text** → bold
 * - *text* → italic
 * - `text` → inline code
 * - ~~text~~ → strikethrough
 */

import { inputRules, InputRule } from 'prosemirror-inputrules';
import type { MarkType } from 'prosemirror-model';
import type { EditorState } from 'prosemirror-state';
import { schema } from '../schema';

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/**
 * Create an input rule that wraps text between delimiters with a mark.
 *
 * When the user types the closing delimiter, the text between the delimiters
 * is wrapped with the specified mark, and the delimiters are removed.
 */
function markWrapRule(pattern: RegExp, markType: MarkType): InputRule {
	return new InputRule(pattern, (state: EditorState, match, start, end) => {
		const [, innerText] = match;
		if (!innerText) return null;

		// Create a transaction that replaces the matched text
		// (including delimiters) with just the inner text marked up
		const tr = state.tr;

		// Create the marked text node
		const markedText = schema.text(innerText, [markType.create()]);

		// Replace the full match with the marked text
		tr.replaceWith(start, end, markedText);

		// Clear stored marks so subsequent typing isn't formatted
		// This lets the user "escape" from the mark after conversion
		tr.removeStoredMark(markType);

		return tr;
	});
}

// =============================================================================
// PLUGIN
// =============================================================================

/**
 * Creates the input rules plugin for markdown-style inline formatting.
 *
 * Supports:
 * - **bold** → strong mark
 * - *italic* → em mark
 * - `code` → code mark
 * - ~~strikethrough~~ → strikethrough mark
 */
export function createFormattingInputRules() {
	return inputRules({
		rules: [
			// Bold: **text** - two asterisks on each side
			// Pattern: matches **anything** at the end of input
			markWrapRule(/\*\*([^*]+)\*\*$/, schema.marks.strong),

			// Italic: *text* - single asterisk on each side
			// Use negative lookbehind to avoid matching ** patterns
			// Pattern: not preceded by *, then *text*, not followed by *
			markWrapRule(/(?<!\*)\*([^*]+)\*(?!\*)$/, schema.marks.em),

			// Inline code: `text` - backticks
			markWrapRule(/`([^`]+)`$/, schema.marks.code),

			// Strikethrough: ~~text~~ - two tildes on each side
			markWrapRule(/~~([^~]+)~~$/, schema.marks.strikethrough),
		],
	});
}
