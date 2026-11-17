/**
 * How home shows its tiles: as cards, as rows, or as a sentence. The choice
 * is kept in this browser, like the page display settings, and cards are the
 * default.
 */

export type HomeLayout = 'cards' | 'rows' | 'prose';

const STORAGE_KEY = 'virtues-home-layout';
const LAYOUTS: HomeLayout[] = ['cards', 'rows', 'prose'];

function load(): HomeLayout {
	try {
		const saved = localStorage.getItem(STORAGE_KEY);
		if (LAYOUTS.includes(saved as HomeLayout)) return saved as HomeLayout;
	} catch {
		// No storage (private window, SSR): cards.
	}
	return 'cards';
}

class HomeLayoutPref {
	value = $state<HomeLayout>(load());

	set(layout: HomeLayout) {
		this.value = layout;
		try {
			localStorage.setItem(STORAGE_KEY, layout);
		} catch {
			// The choice still holds until the app reloads.
		}
	}
}

export const homeLayout = new HomeLayoutPref();
