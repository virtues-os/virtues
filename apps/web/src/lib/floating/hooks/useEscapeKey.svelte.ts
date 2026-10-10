/**
 * useEscapeKey Hook
 *
 * Listens for Escape key and calls a callback.
 * Automatically cleans up on unmount or when disabled.
 */

import { inComposition } from '$lib/utils/ime';

export function useEscapeKey(onEscape: () => void, enabled: () => boolean = () => true) {
	$effect(() => {
		if (!enabled()) return;

		function handleKeydown(event: KeyboardEvent) {
			// An Escape mid-composition drops the input method's candidate.
			if (event.key === 'Escape' && !inComposition(event)) {
				event.preventDefault();
				onEscape();
			}
		}

		document.addEventListener('keydown', handleKeydown);
		return () => document.removeEventListener('keydown', handleKeydown);
	});
}
