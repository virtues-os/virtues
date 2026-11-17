/**
 * Glass: how much of the painting shows through the shell's glass (the
 * sidebar and rail, the selected tab, the corner buttons, the home cards),
 * set in Settings · Appearance.
 *
 * Kept per browser, like Reduce motion, since it is about the screen in
 * front of you. The root carries `data-glass` for every level but Medium,
 * the default, and app.css turns Low and High into `--lq-see`, which the
 * glass tokens in themes.css scale their fills by, and None into the plain
 * surface. app.html sets the attribute before the first paint so the
 * glass does not change after the page loads.
 */

export type GlassLevel = 'none' | 'low' | 'medium' | 'high';

const STORAGE_KEY = 'virtues-glass';
const LEVELS: readonly GlassLevel[] = ['none', 'low', 'medium', 'high'];

function readSaved(): GlassLevel {
	try {
		const saved = localStorage.getItem(STORAGE_KEY);
		return LEVELS.includes(saved as GlassLevel) ? (saved as GlassLevel) : 'medium';
	} catch {
		return 'medium';
	}
}

class Glass {
	level = $state<GlassLevel>('medium');

	constructor() {
		if (typeof window === 'undefined') return;
		this.level = readSaved();
		this.#apply();
	}

	set(next: GlassLevel) {
		this.level = next;
		try {
			if (next === 'medium') localStorage.removeItem(STORAGE_KEY);
			else localStorage.setItem(STORAGE_KEY, next);
		} catch {
			// No storage: the choice lasts until the page reloads.
		}
		this.#apply();
	}

	#apply() {
		const root = document.documentElement;
		if (this.level === 'medium') root.removeAttribute('data-glass');
		else root.setAttribute('data-glass', this.level);
	}
}

export const glass = new Glass();
