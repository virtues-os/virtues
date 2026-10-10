/**
 * The status bar's counts for a block page, read off the tree.
 */

import type { Mark, Node as PmNode } from '@tiptap/pm/model';
import type { DocStats } from '$lib/components/pages/stats';

/** Block widgets the media count counts. */
export const MEDIA_TYPES = new Set(['image', 'audio', 'video', 'file', 'applet']);

/** What an inline widget reads as: a mention as its name, anything else as nothing. */
function leafText(node: PmNode): string {
	return node.type.name === 'mention' ? String(node.attrs.label ?? '') : '';
}

/**
 * Words, characters, links and media in a page. A link is a link mark's run
 * (text in one link, however many marks split it, counts once) or a mention.
 * Words and characters count what a reader sees: a mention reads as its
 * name, and the breaks between blocks are not characters.
 */
export function treeStats(doc: PmNode): DocStats {
	const text = doc.textBetween(0, doc.content.size, '\n', leafText);
	const words = text.split(/\s+/).filter(Boolean);
	let linkCount = 0;
	let mediaCount = 0;
	let lastLink: Mark | null = null;
	let lastEnd = -1;

	doc.descendants((node, pos) => {
		if (MEDIA_TYPES.has(node.type.name)) mediaCount++;
		if (node.type.name === 'mention') {
			linkCount++;
			lastLink = null;
			return false;
		}
		if (node.isText) {
			const link = node.marks.find((m) => m.type.name === 'link') ?? null;
			if (link && !(lastLink && pos === lastEnd && lastLink.eq(link))) linkCount++;
			lastLink = link;
			lastEnd = pos + node.nodeSize;
			return false;
		}
		if (!node.isInline) lastLink = null;
		return true;
	});

	return {
		wordCount: words.length,
		charCount: text.replace(/\n/g, '').length,
		linkCount,
		mediaCount,
	};
}
