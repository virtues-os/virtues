/**
 * useClickOutside Hook
 *
 * Detects clicks outside of specified elements and calls a callback.
 * Uses mousedown for faster response. A press in a dialog opened over them
 * is not outside (`pressedOutside`).
 */

import { pressedOutside } from '../core/outside';

export function useClickOutside(
	getElements: () => (HTMLElement | null)[],
	onClickOutside: () => void,
	enabled: () => boolean = () => true
) {
	$effect(() => {
		if (!enabled()) return;

		function handleClick(event: MouseEvent) {
			if (pressedOutside(event.target, getElements())) onClickOutside();
		}

		// Use mousedown for faster response
		document.addEventListener('mousedown', handleClick);
		return () => document.removeEventListener('mousedown', handleClick);
	});
}
