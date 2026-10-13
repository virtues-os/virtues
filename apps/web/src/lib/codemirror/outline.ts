/**
 * Page outline for the CodeMirror editor: h1–h3 headings extracted from the
 * markdown, with their document positions, and the `OutlineNav` that lets
 * the table of contents scroll the editor to them.
 */

import { EditorView } from '@codemirror/view';
import type { OutlineNav, PageHeading } from '$lib/components/pages/outline';

export type { PageHeading } from '$lib/components/pages/outline';

const HEADING_RE = /^(#{1,3})\s+(.+?)\s*$/;

/**
 * Extract h1–h3 headings with their document positions. Fenced code blocks are
 * skipped so a `# comment` inside code isn't mistaken for a heading.
 */
export function extractHeadings(content: string): PageHeading[] {
	const out: PageHeading[] = [];
	let pos = 0;
	let inFence = false;
	for (const line of content.split('\n')) {
		const trimmed = line.trimStart();
		if (trimmed.startsWith('```') || trimmed.startsWith('~~~')) {
			inFence = !inFence;
		} else if (!inFence) {
			const m = HEADING_RE.exec(line);
			if (m) out.push({ level: m[1].length as 1 | 2 | 3, text: m[2], from: pos });
		}
		pos += line.length + 1; // +1 for the newline
	}
	return out;
}

/**
 * The outline's view of a CodeMirror editor. Headings live as lines, not DOM
 * anchors: scroll-to goes through `EditorView.scrollIntoView`, and a
 * heading's top is its line block's, which shares the editor scroller's
 * coordinates.
 */
export function cmOutlineNav(view: EditorView): OutlineNav {
	return {
		scroller: view.scrollDOM,
		topOf(h) {
			const pos = Math.min(h.from, view.state.doc.length);
			try {
				return view.lineBlockAt(pos).top;
			} catch {
				return null;
			}
		},
		scrollTo(h) {
			const pos = Math.min(h.from, view.state.doc.length);
			view.dispatch({ effects: EditorView.scrollIntoView(pos, { y: 'start', yMargin: 24 }) });
		},
	};
}
