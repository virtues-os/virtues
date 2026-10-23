/**
 * The open RecordCard over a run of text that links to your data: a chat
 * answer, or a page. Each link to a record carries `data-cite-route`, and
 * the card steps through them in the order they appear, one stop per
 * record, however many times the text links it.
 *
 * The day page keeps its own: its stops are sentences, and its card can sit
 * in the margin (DayArticleBody).
 */

import { cardMode } from '$lib/components/RecordCard.svelte';

/** The event a `Ref` to a record sends up when clicked, for the text it is in to open its card. */
export const RECORD_CITE = 'recordcite';
export type RecordCiteDetail = { route: string; focus: boolean };

export interface OpenCard {
	route: string;
	anchor: HTMLElement;
	/** The sentence the link is in, for the words the card marks. */
	sentence: string;
	mode: 'popover' | 'sheet';
	position: { n: number; total: number };
	/** Opened, or stepped to, from the keyboard. */
	focus: boolean;
}

/**
 * The sentence a link sits in, as its reader sees it. The card marks the
 * words the record shares with it, as the day page does with its sentences.
 */
export function sentenceAround(anchor: HTMLElement): string {
	const block = anchor.closest('p, li, td, th, blockquote, h1, h2, h3, h4, h5, h6') ?? anchor.parentElement;
	const text = block?.textContent ?? anchor.textContent ?? '';
	if (!block || typeof Intl.Segmenter !== 'function') return text;
	const range = document.createRange();
	range.selectNodeContents(block);
	range.setEndBefore(anchor);
	const at = range.toString().length;
	for (const s of new Intl.Segmenter(undefined, { granularity: 'sentence' }).segment(text)) {
		if (at >= s.index && at < s.index + s.segment.length) return s.segment.trim();
	}
	return text;
}

/**
 * `mark: false` leaves the links' own elements alone. Inside a page's
 * editor, a class set on one is a change to the page's DOM, which the
 * editor would read back and redraw.
 */
export function recordCards(getRoot: () => HTMLElement | null, { mark: marks = true } = {}) {
	let open = $state<OpenCard | null>(null);

	const anchors = () => [...(getRoot()?.querySelectorAll<HTMLElement>('[data-cite-route]') ?? [])];
	const routes = () => [...new Set(anchors().map((a) => a.dataset.citeRoute ?? ''))].filter(Boolean);

	function mark(anchor: HTMLElement, on: boolean) {
		if (!marks) return;
		anchor.classList.toggle('is-open', on);
		anchor.setAttribute('aria-expanded', String(on));
	}

	function place(route: string, anchor: HTMLElement, focus: boolean) {
		if (open) mark(open.anchor, false);
		mark(anchor, true);
		const list = routes();
		open = {
			route,
			anchor,
			sentence: sentenceAround(anchor),
			mode: cardMode(),
			position: { n: list.indexOf(route) + 1, total: list.length },
			focus,
		};
	}

	return {
		get open() {
			return open;
		},

		/** Open the card on a link; a second click on the same link closes it. */
		show(route: string, anchor: HTMLElement, focus = false) {
			if (open?.anchor === anchor) return this.close();
			place(route, anchor, focus);
		},

		close(returnFocus = false) {
			if (!open) return;
			const anchor = open.anchor;
			mark(anchor, false);
			open = null;
			if (returnFocus) anchor.focus({ preventScroll: true });
		},

		/** The previous or next record the text links, scrolled into view. */
		step(by: -1 | 1) {
			if (!open) return;
			const list = routes();
			const next = list[list.indexOf(open.route) + by];
			const anchor = next ? anchors().find((a) => a.dataset.citeRoute === next) : undefined;
			if (!next || !anchor) return;
			const still = matchMedia('(prefers-reduced-motion: reduce)').matches;
			anchor.scrollIntoView({ block: 'nearest', behavior: still ? 'auto' : 'smooth' });
			place(next, anchor, !!document.activeElement?.closest('.record-card'));
		},

		/**
		 * After the text re-renders (a reply still arriving, a page edited
		 * elsewhere): keep the card on the same record's new link, or close it
		 * when the link is gone.
		 */
		reattach() {
			if (!open) return;
			if (open.anchor.isConnected) {
				const list = routes();
				const n = list.indexOf(open.route) + 1;
				if (n !== open.position.n || list.length !== open.position.total) open = { ...open, position: { n, total: list.length } };
				return;
			}
			const anchor = anchors().find((a) => a.dataset.citeRoute === open!.route);
			if (anchor) place(open.route, anchor, false);
			else open = null;
		},
	};
}
