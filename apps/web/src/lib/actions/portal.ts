/**
 * `use:portal` — move the node to `<body>` while it lives, or into `target`
 * when one is given (`use:portal={host}`), following it if it changes.
 *
 * For anything that floats over the page (a modal, a lightbox, a card beside
 * a word): rendered where it is declared, it would inherit the type of the
 * paragraph around it and be clipped by any scrolling ancestor. A target
 * hangs it from another part of the view, as the page's find bar hangs from
 * the page's toolbar.
 */

import type { Action } from "svelte/action";

export const portal: Action<HTMLElement, HTMLElement | null | undefined> = (node, target) => {
	move(node, target);
	return {
		update: (next) => move(node, next),
		destroy: () => node.remove(),
	};
};

/**
 * Moving a node takes the focus out of it: a field that focused itself as
 * it mounted (the find bar's) would lose the keys typed next. What held
 * the focus inside the node gets it back.
 */
function move(node: HTMLElement, target: HTMLElement | null | undefined): void {
	const focused = document.activeElement;
	(target ?? document.body).appendChild(node);
	if (focused instanceof HTMLElement && focused !== document.activeElement && node.contains(focused)) {
		focused.focus({ preventScroll: true });
	}
}
