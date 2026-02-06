/**
 * useEscapeKey Hook
 *
 * Listens for Escape key and calls a callback.
 * Automatically cleans up on unmount or when disabled.
 */

export function useEscapeKey(onEscape: () => void, enabled: () => boolean = () => true) {
	$effect(() => {
		if (!enabled()) return;

		function handleKeydown(event: KeyboardEvent) {
			if (event.key === 'Escape') {
				event.preventDefault();
				onEscape();
			}
		}

		document.addEventListener('keydown', handleKeydown);
		return () => document.removeEventListener('keydown', handleKeydown);
	});
}
