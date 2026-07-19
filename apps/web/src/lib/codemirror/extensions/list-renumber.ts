/**
 * Ordered-list auto-renumbering
 *
 * Keeps numbered markdown lists sequential ("always normalize"): after any
 * local edit, each contiguous run of ordered items is rewritten to run
 * 1,2,3… from the run's own starting number (so a list that begins at `5.`
 * stays 5,6,7…). Nested lists renumber independently per indent level.
 *
 * Implemented as a transactionFilter so the renumbering rides along in the
 * SAME transaction as the user's edit — one undo step, one Yjs update, no
 * follow-up dispatch loop. Gated to local input/delete events so it never
 * fires on remote Yjs sync (which would echo divergent edits between peers;
 * normalization is deterministic, so an already-normalized remote edit is a
 * no-op anyway, but skipping avoids the round-trip entirely).
 */

import { type ChangeSpec, EditorState, type Extension, type Text } from '@codemirror/state';

// Ordered item: leading indent, digits, `.` or `)`, then at least one space.
const ORDERED_RE = /^(\s*)(\d+)([.)])(\s)/;
// Bullet item: leading indent, one of -*+, then a space.
const BULLET_RE = /^(\s*)[-*+](\s)/;

interface Change {
	from: number;
	to: number;
	insert: string;
}

/**
 * Scan the document and return the changes needed to make every ordered-list
 * run sequential. Positions are in `doc` coordinates.
 */
function computeRenumbering(doc: Text): Change[] {
	const changes: Change[] = [];
	// indent width → next expected ordinal for that level
	const counters = new Map<number, number>();
	let blankRun = 0;

	for (let i = 1; i <= doc.lines; i++) {
		const line = doc.line(i);
		const text = line.text;

		if (text.trim() === '') {
			// A single blank line can sit inside a loose list; two breaks it.
			blankRun++;
			if (blankRun >= 2) counters.clear();
			continue;
		}
		blankRun = 0;

		const ordered = ORDERED_RE.exec(text);
		if (ordered) {
			const indent = ordered[1].length;
			const parsed = parseInt(ordered[2], 10);

			// Dedent resets any deeper (more-indented) counters.
			for (const key of counters.keys()) {
				if (key > indent) counters.delete(key);
			}

			let expected = counters.get(indent);
			if (expected === undefined) {
				// First item of a fresh run at this level — honor its start value.
				expected = parsed;
			}

			if (parsed !== expected) {
				const digitFrom = line.from + ordered[1].length;
				const digitTo = digitFrom + ordered[2].length;
				changes.push({ from: digitFrom, to: digitTo, insert: String(expected) });
			}

			counters.set(indent, expected + 1);
			continue;
		}

		const bullet = BULLET_RE.exec(text);
		if (bullet) {
			// A bullet interrupts ordered runs at its indent and deeper.
			const indent = bullet[1].length;
			for (const key of counters.keys()) {
				if (key >= indent) counters.delete(key);
			}
			continue;
		}

		// Continuation line (indented paragraph under a list item) keeps the
		// run alive; any other non-empty line ends all runs.
		const leading = text.length - text.trimStart().length;
		if (leading === 0) counters.clear();
	}

	return changes;
}

export const listRenumber: Extension = EditorState.transactionFilter.of((tr) => {
	if (!tr.docChanged) return tr;
	// Only local text editing — never remote Yjs sync or programmatic dispatch.
	if (!tr.isUserEvent('input') && !tr.isUserEvent('delete')) return tr;

	const changes = computeRenumbering(tr.newDoc);
	if (changes.length === 0) return tr;

	// Appended-spec change positions are in tr.newDoc coordinates; CodeMirror
	// composes them into the same transaction. The filter is not re-run on its
	// own output, so there is no recursion.
	return [tr, { changes: changes as ChangeSpec, sequential: false }];
});
