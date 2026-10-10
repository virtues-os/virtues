/**
 * The page outline for a block page: its h1 to h3 headings by block id, and
 * the `OutlineNav` that scrolls the editor's scroller to one.
 */

import type { Editor } from '@tiptap/core';
import type { Node as PmNode } from '@tiptap/pm/model';
import type { OutlineNav, PageHeading } from '$lib/components/pages/outline';
import { contract } from './schema';

/** Room left above a heading scrolled to, so it does not sit under the bar. */
const SCROLL_MARGIN = 24;

/** The h1 to h3 headings of a page, in order, at the top level and inside containers. */
export function treeHeadings(doc: PmNode): PageHeading[] {
	const out: PageHeading[] = [];
	doc.descendants((node, pos) => {
		if (node.type.name !== 'heading') return !node.isTextblock;
		const level = Number(node.attrs.level);
		if (level >= 1 && level <= 3) {
			const id = node.attrs[contract.id.attr];
			out.push({
				level: level as 1 | 2 | 3,
				text: node.textContent.trim(),
				from: pos,
				...(typeof id === 'string' ? { id } : {}),
			});
		}
		return false;
	});
	return out.filter((h) => h.text.length > 0);
}

/**
 * The nearest ancestor of `el` set to scroll vertically, or the document's
 * scroller. Read from its style alone, not from whether it overflows now:
 * a page that opens short is still scrolled by that element once it grows,
 * and what binds to the scroller binds once, when the editor mounts.
 */
export function scrollerOf(el: HTMLElement): HTMLElement {
	for (let node = el.parentElement; node; node = node.parentElement) {
		const overflow = getComputedStyle(node).overflowY;
		if (overflow === 'auto' || overflow === 'scroll' || overflow === 'overlay') return node;
	}
	return (document.scrollingElement as HTMLElement | null) ?? document.documentElement;
}

/** The heading's element: by its block id, else by its position. */
function headingElement(editor: Editor, h: PageHeading): HTMLElement | null {
	if (editor.isDestroyed) return null;
	const root = editor.view.dom;
	if (h.id) {
		const el = root.querySelector<HTMLElement>(`[${contract.id.html}="${CSS.escape(h.id)}"]`);
		if (el) return el;
	}
	try {
		const node = editor.view.nodeDOM(h.from);
		return node instanceof HTMLElement ? node : null;
	} catch {
		return null;
	}
}

export function treeOutlineNav(editor: Editor): OutlineNav {
	const scroller = scrollerOf(editor.view.dom);
	const topOf = (h: PageHeading): number | null => {
		const el = headingElement(editor, h);
		if (!el) return null;
		const base = scroller === document.scrollingElement ? 0 : scroller.getBoundingClientRect().top;
		return el.getBoundingClientRect().top - base + scroller.scrollTop;
	};
	return {
		scroller,
		topOf,
		scrollTo(h) {
			const top = topOf(h);
			if (top === null) return;
			scroller.scrollTo({ top: Math.max(0, top - SCROLL_MARGIN), behavior: 'smooth' });
		},
	};
}
