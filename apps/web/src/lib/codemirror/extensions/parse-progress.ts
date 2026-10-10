/**
 * Whether the syntax tree moved on between two states: an edit, or the
 * language's background parse catching up with the document.
 *
 * A state's first parse stops when its time slice runs out (20ms of the
 * clock, in @codemirror/language), so a long page, or any page on a busy
 * machine, starts with a tree that ends part way down the document. The rest
 * arrives in later transactions that change nothing else. Decorations built
 * from the tree rebuild on this, or what the first slice did not reach stays
 * undrawn until the next edit or caret move.
 */

import { syntaxTree } from '@codemirror/language';
import type { EditorState } from '@codemirror/state';

/** For a `ViewUpdate` or a `Transaction`: both carry the two states. */
export function treeAdvanced(update: { state: EditorState; startState: EditorState }): boolean {
	return syntaxTree(update.state) !== syntaxTree(update.startState);
}
