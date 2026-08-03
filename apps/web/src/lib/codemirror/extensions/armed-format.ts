/**
 * Armed formatting — Cmd+B with nothing selected
 *
 * With delimiters permanently hidden, the old behavior of this shortcut broke
 * down: it inserted `****` and parked the caret in the middle, which under
 * never-reveal renders as nothing at all — an invisible caret inside an
 * invisible construct, with no way to tell bold was on.
 *
 * So an empty-selection Cmd+B no longer writes anything. It ARMS the format,
 * and the next character typed is written already wrapped, with the caret left
 * inside so that typing continues in bold. Nothing exists in the document until
 * there is something for it to mark up, which means there is never a moment
 * where the document contains formatting the screen cannot show.
 *
 * Arming is deliberately fragile: it survives exactly one keystroke, and any
 * cursor move or edit discards it. A mode you can forget you are in is the
 * thing this file exists to avoid.
 */

import { StateEffect, StateField, type Extension } from '@codemirror/state';
import { EditorView } from '@codemirror/view';

export interface ArmedFormat {
	open: string;
	close: string;
}

/** Toggle a format on the pending list. */
export const armFormat = StateEffect.define<ArmedFormat>();

/**
 * Formats waiting for their first character, outermost first.
 *
 * Cleared by any doc change or selection move — including the very insertion
 * that consumes it, which is how the arming ends after one character.
 */
export const armedFormats = StateField.define<ArmedFormat[]>({
	create: () => [],

	update(value, tr) {
		for (const effect of tr.effects) {
			if (effect.is(armFormat)) {
				const already = value.some((f) => f.open === effect.value.open);
				return already
					? value.filter((f) => f.open !== effect.value.open)
					: [...value, effect.value];
			}
		}
		if (tr.docChanged || tr.selection) return [];
		return value;
	},
});

/**
 * Writes the wrapped character.
 *
 * Only single characters are intercepted. A paste or an IME composition is not
 * "the next character typed" in any sense the user means, and wrapping only the
 * first glyph of a pasted run would be worse than doing nothing.
 */
const armedInput: Extension = EditorView.inputHandler.of((view, from, to, text) => {
	const armed = view.state.field(armedFormats, false);
	if (!armed || armed.length === 0) return false;
	if ([...text].length !== 1) return false;

	const open = armed.map((f) => f.open).join('');
	const close = armed
		.slice()
		.reverse()
		.map((f) => f.close)
		.join('');

	view.dispatch({
		changes: { from, to, insert: `${open}${text}${close}` },
		// Inside the closing delimiter, so continued typing stays formatted.
		selection: { anchor: from + open.length + text.length },
		userEvent: 'input.type',
	});
	return true;
});

/**
 * The only signal that a format is armed.
 *
 * `caret-color` is used because this editor draws the native caret — there is
 * no drawSelection extension here, so there is no CodeMirror cursor element to
 * style. Recoloring the real caret is both the cheapest and the most direct way
 * to say "what you type next will be bold".
 */
const armedIndicator: Extension = EditorView.contentAttributes.compute(
	[armedFormats],
	(state): Record<string, string> => {
		const armed = state.field(armedFormats, false);
		return armed && armed.length > 0 ? { class: 'cm-format-armed' } : {};
	},
);

export const armedFormatting: Extension = [armedFormats, armedInput, armedIndicator];
