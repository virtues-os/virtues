/**
 * useClickOutside Hook
 *
 * Detects clicks outside of specified elements and calls a callback.
 * Uses mousedown for faster response.
 */

export function useClickOutside(
	getElements: () => (HTMLElement | null)[],
	onClickOutside: () => void,
	enabled: () => boolean = () => true
) {
	$effect(() => {
		if (!enabled()) return;

		function handleClick(event: MouseEvent) {
			const elements = getElements().filter(Boolean) as HTMLElement[];
			const target = event.target as Node;

			// Check if click is inside any of the elements
			const isInside = elements.some((el) => el.contains(target));
			if (!isInside) {
				onClickOutside();
			}
		}

		// Use mousedown for faster response
		document.addEventListener('mousedown', handleClick);
		return () => document.removeEventListener('mousedown', handleClick);
	});
}
