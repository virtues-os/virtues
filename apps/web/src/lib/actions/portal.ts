/**
 * `use:portal` — move the node to `<body>` while it lives.
 *
 * For anything that floats over the page (a modal, a lightbox, a card beside
 * a word): rendered where it is declared, it would inherit the type of the
 * paragraph around it and be clipped by any scrolling ancestor.
 */

import type { Action } from "svelte/action";

export const portal: Action<HTMLElement> = (node) => {
	document.body.appendChild(node);
	return { destroy: () => node.remove() };
};
