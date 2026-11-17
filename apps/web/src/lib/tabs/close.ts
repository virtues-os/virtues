/**
 * Closing the tab you are on: its page shrinks back up into its tab and
 * fades while the tab folds away (WindowTabBar `foldAway`). The next page
 * is already underneath, so it is uncovered and does not play its arrival
 * ($lib/tabs/arrive reads the root's `tab-closing`).
 *
 * SplitContainer calls `markClosed` with the tabs that were on screen and
 * are gone, before the slots update, and puts `closeIntoTab` on every slot
 * as its outro. A slot that leaves for any other reason, or one that was
 * hidden, goes at once.
 *
 * Home and a new chat stand on glass, which a fading ancestor would cut off
 * from the painting, so they leave at once too.
 */
import type { TransitionConfig } from 'svelte/transition';
import { reducedMotion } from '$lib/stores/motion.svelte';
import { isNewChat } from '$lib/components/chat/state/chatRoute';
import { easeExit } from '$lib/utils/transitions';

/** A page is a big thing to put away: twice a floating surface's exit. */
const CLOSE_MS = 240;

const closing = new Set<string>();

/** The tabs that were on screen and have just closed. */
export function markClosed(ids: string[]): void {
	if (ids.length === 0 || typeof document === 'undefined') return;
	const root = document.documentElement;
	for (const id of ids) closing.add(id);
	root.classList.add('tab-closing');
	// The slots update in this flush; the arrival reads the class as they do.
	setTimeout(() => {
		root.classList.remove('tab-closing');
		for (const id of ids) closing.delete(id);
	}, 100);
}

export function closeIntoTab(node: HTMLElement, tab: { id: string; route: string }): TransitionConfig {
	if (!closing.has(tab.id) || reducedMotion() || node.style.display === 'none' || isNewChat(tab.route)) {
		return { duration: 0 };
	}
	// The tab is still in its strip, folding, as the page leaves.
	const chip = document.querySelector(`.tab[data-tab-id="${CSS.escape(tab.id)}"]`);
	const page = node.getBoundingClientRect();
	const at = chip?.getBoundingClientRect();
	const x = at ? at.left + at.width / 2 - page.left : page.width / 2;
	const y = at ? at.top + at.height / 2 - page.top : -24;
	node.style.transformOrigin = `${x}px ${y}px`;
	node.style.zIndex = '3';
	node.style.pointerEvents = 'none';
	return {
		duration: CLOSE_MS,
		easing: easeExit,
		// An outro runs t from 1 down to 0.
		css: (t) => `opacity: ${t}; transform: translateY(${(1 - t) * -4}px) scale(${0.92 + 0.08 * t})`,
	};
}
