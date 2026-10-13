/**
 * Putting a block widget (an image, a file, an applet) into text: what an
 * upload, a picked file, the insert menu and a typed `![name](url)` share.
 * No Svelte here, so the schema reaches it.
 */

import type { Node as PmNode } from '@tiptap/pm/model';
import type { Transaction } from '@tiptap/pm/state';

/**
 * Put a block at `pos`: in place of an empty paragraph, before or after a
 * paragraph the position starts or ends, or splitting it. Returns the
 * position just after the block.
 */
export function insertBlockAt(tr: Transaction, pos: number, node: PmNode): number {
	const $pos = tr.doc.resolve(Math.min(pos, tr.doc.content.size));
	if ($pos.parent.isTextblock && $pos.depth > 0) {
		if ($pos.parent.content.size === 0) tr.replaceWith($pos.before(), $pos.after(), node);
		else if ($pos.parentOffset === 0) tr.insert($pos.before(), node);
		else if ($pos.parentOffset === $pos.parent.content.size) tr.insert($pos.after(), node);
		else tr.replaceWith($pos.pos, $pos.pos, node);
	} else {
		tr.replaceWith($pos.pos, $pos.pos, node);
	}
	// Where it landed: a block is placed whole, so it is the node itself,
	// at most a few levels of splitting past where it was asked for.
	const from = tr.mapping.map($pos.pos, -1);
	const limit = Math.min(tr.doc.content.size, from + 2 * $pos.depth + 2);
	for (let p = Math.max(0, from - 2 * $pos.depth - 2); p <= limit; p++) {
		if (tr.doc.nodeAt(p) === node) return p + node.nodeSize;
	}
	return tr.mapping.map($pos.pos, 1);
}
