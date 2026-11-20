/**
 * Background on pages: how strongly the window's painting shows around a
 * page or a chat, set in Settings · Appearance. Home always shows it in
 * full; when a page opens the painting fades to this strength, and back
 * when home returns (the `.window-scene` rule in routes/(app)/+layout).
 *
 * Kept per browser, like Glass, since it is about the screen in front of
 * you. The root carries `--page-scene-opacity` for every value but the
 * default, and app.html sets it before the first paint.
 */

const STORAGE_KEY = 'virtues-page-background';
/** Percent of full strength. */
export const PAGE_BACKGROUND_DEFAULT = 50;

function readSaved(): number {
	try {
		const saved = Number(localStorage.getItem(STORAGE_KEY));
		return localStorage.getItem(STORAGE_KEY) !== null && Number.isFinite(saved) && saved >= 0 && saved <= 100
			? saved
			: PAGE_BACKGROUND_DEFAULT;
	} catch {
		return PAGE_BACKGROUND_DEFAULT;
	}
}

class PageBackground {
	percent = $state(PAGE_BACKGROUND_DEFAULT);

	constructor() {
		if (typeof window === 'undefined') return;
		this.percent = readSaved();
		this.#apply();
	}

	set(next: number) {
		this.percent = Math.round(Math.min(100, Math.max(0, next)));
		try {
			if (this.percent === PAGE_BACKGROUND_DEFAULT) localStorage.removeItem(STORAGE_KEY);
			else localStorage.setItem(STORAGE_KEY, String(this.percent));
		} catch {
			// No storage: the choice lasts until the page reloads.
		}
		this.#apply();
	}

	#apply() {
		const root = document.documentElement;
		if (this.percent === PAGE_BACKGROUND_DEFAULT) root.style.removeProperty('--page-scene-opacity');
		else root.style.setProperty('--page-scene-opacity', String(this.percent / 100));
	}
}

export const pageBackground = new PageBackground();
