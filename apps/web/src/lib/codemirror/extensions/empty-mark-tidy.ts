/**
 * Empty-mark tidy
 *
 * Cmd+B with nothing selected inserts `****` with the caret inside; if the
 * writer then navigates away without typing, the pair stays in the document —
 * invisible on the rendered surface (an empty construct produces no mark to
 * reveal), visible in raw mode as litter, and a confusing obstacle for the
 * caret either way. This listener removes a delimiter pair the moment the
 * selection leaves it still empty.
 *
 * Deliberately narrow: it fires only on pure selection moves (no doc change,
 * so old positions need no mapping), only for a caret that was EXACTLY
 * between a known open/close pair, and never when that range belongs to a
 * real construct (a caret sitting between the two `*` of a genuine closing
 * `**` must not get them deleted — the inlineMarks check is what rules that
 * out, since only non-empty constructs produce marks).
 */

import type { Extension } from '@codemirror/state';
import { EditorView } from '@codemirror/view';

import { inlineMarks } from './inline-marks';

/** Longest-open first, so an empty bold matches before an empty italic. */
const PAIRS: [open: string, close: string][] = [
	['<u>', '</u>'],
	['**', '**'],
	['~~', '~~'],
	['==', '=='],
	['*', '*'],
	['`', '`'],
];

export const emptyMarkTidy: Extension = EditorView.updateListener.of((update) => {
	if (update.docChanged || !update.selectionSet) return;

	const { state, startState } = update;
	const oldHead = startState.selection.main.head;
	const newSel = state.selection.main;

	for (const [open, close] of PAIRS) {
		const from = oldHead - open.length;
		const to = oldHead + close.length;
		if (from < 0 || to > startState.doc.length) continue;
		if (startState.sliceDoc(from, oldHead) !== open) continue;
		if (startState.sliceDoc(oldHead, to) !== close) continue;

		// Still touching the pair (edges inclusive, matching the reveal rule)
		// → the writer may yet type into it.
		if (newSel.from <= to && newSel.to >= from) return;

		// Part of a real construct? Then these characters are somebody's
		// delimiters, not an abandoned pair.
		const line = startState.doc.lineAt(oldHead);
		const owned = inlineMarks(startState, line.from, line.to).some(
			(m) => m.openFrom < to && m.closeTo > from,
		);
		if (owned) return;

		update.view.dispatch({
			changes: { from, to },
			userEvent: 'delete.emptymark',
		});
		return;
	}
});
