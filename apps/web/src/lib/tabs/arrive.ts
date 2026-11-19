/**
 * A page you open comes in rather than cutting: it rises from a few pixels
 * below as it fades up, quickly enough that opening still feels instant.
 *
 * It plays on a pane's tab slot whenever the page in it changes: when the
 * slot becomes the active one (switching tabs), and when its tab navigates
 * in place (a link, the sidebar, Back, search). A CSS animation could do
 * only the first, since an in-place navigation keeps the same element on
 * screen.
 *
 * A page opened from search arrives with more weight: it comes up from
 * further below on a soft spring, a beat after the search card drops away
 * ($lib/search/handoff).
 *
 * Closing the tab you are on is a switch to its neighbour: the closed page
 * goes at once and the page that takes its place arrives as above.
 *
 * Two cases keep still. Home and a new chat stand on glass cards over the
 * window's painting, and a fading ancestor cuts glass off from what it
 * frosts. A tab moving between panes has its own motion (`data-motion`).
 */
import { reducedMotion } from '$lib/stores/motion.svelte';
import { isNewChat } from '$lib/components/chat/state/chatRoute';
import { springEasing } from '$lib/utils/springEasing';

export interface Arrival {
	route: string;
	active: boolean;
}

/** Search's arrival: soft, settling with a hint of overshoot. */
const WEIGHTED = springEasing(260, 26);
/** How long the page waits for the search card to start dropping. */
const WEIGHTED_DELAY_MS = 50;

export function arrive(node: HTMLElement, params: Arrival) {
	let last = params;

	function play(route: string) {
		// By route, not by looking for Home in the slot: when a tab navigates
		// away from Home in place, Home is still in the slot as this runs.
		if (reducedMotion() || node.dataset.motion || isNewChat(route)) return;
		const root = document.documentElement;
		if (root.classList.contains('search-handoff')) {
			node.animate([{ transform: 'translateY(24px) scale(0.99)' }, { transform: 'none' }], {
				duration: WEIGHTED.duration,
				delay: WEIGHTED_DELAY_MS,
				easing: WEIGHTED.easing,
				fill: 'backwards',
			});
			node.animate([{ opacity: 0 }, { opacity: 1 }], {
				duration: 180,
				delay: WEIGHTED_DELAY_MS,
				easing: 'linear',
				fill: 'backwards',
			});
			return;
		}
		node.animate([{ opacity: 0.4, translate: '0 4px' }, { opacity: 1, translate: '0 0' }], {
			duration: 200,
			easing: 'cubic-bezier(0.2, 0, 0, 1)',
		});
	}

	if (params.active) play(params.route);

	return {
		update(next: Arrival) {
			if (next.active && (!last.active || next.route !== last.route)) play(next.route);
			last = next;
		},
	};
}
