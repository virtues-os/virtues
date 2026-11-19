/**
 * Search's hand-off to the page you chose: one surface goes down and the
 * next comes up. The search card drops and shrinks as it fades (its outro in
 * SearchModal), and the page rises into place on a soft spring (the weighted
 * arrival in $lib/tabs/arrive), a beat behind.
 *
 * The root carries `search-handoff` for the moment the page opens, which is
 * how the arrival knows to play its weighted version.
 */
import { search } from '$lib/stores/search.svelte';

/** Open a result with the hand-off. `open` opens the page. */
export function handOff(open: () => void): void {
	const root = document.documentElement;
	root.classList.add('search-handoff');
	open();
	search.hide({ sink: true });
	// The arrival reads the class when the slot updates, after this flush.
	setTimeout(() => root.classList.remove('search-handoff'), 100);
}
