/**
 * Reduce motion: the device's setting, or the one in Settings · Appearance.
 *
 * The Settings choice is "Match your device" or "On", kept per browser like
 * the page display settings, since motion is about the screen in front of
 * you. When it is on, the root carries `data-reduce-motion` and app.css
 * stills every CSS animation and transition, the way the `@media
 * (prefers-reduced-motion)` blocks do for the device setting. Code that
 * animates in JS (Svelte transitions, the Web Animations API, canvases,
 * smooth scrolling) asks `reducedMotion()` instead of the media query, so it
 * follows both. Svelte transitions come from $lib/utils/transitions, which
 * asks it for them.
 */

export type MotionSetting = 'device' | 'reduce';

const STORAGE_KEY = 'virtues-reduce-motion';
const QUERY = '(prefers-reduced-motion: reduce)';

function readSaved(): MotionSetting {
	try {
		return localStorage.getItem(STORAGE_KEY) === 'reduce' ? 'reduce' : 'device';
	} catch {
		return 'device';
	}
}

class Motion {
	setting = $state<MotionSetting>('device');
	#device = $state(false);
	#listeners = new Set<(reduced: boolean) => void>();

	constructor() {
		if (typeof window === 'undefined') return;
		this.setting = readSaved();
		const query = window.matchMedia?.(QUERY);
		this.#device = query?.matches ?? false;
		query?.addEventListener('change', (e) => {
			const before = this.reduced;
			this.#device = e.matches;
			this.#notify(before);
		});
		this.#apply();
	}

	/** True when motion should be stilled, by the device or by Settings. */
	get reduced(): boolean {
		return this.setting === 'reduce' || this.#device;
	}

	/** The device's own setting, for the row's help text. */
	get device(): boolean {
		return this.#device;
	}

	set(next: MotionSetting) {
		const before = this.reduced;
		this.setting = next;
		try {
			if (next === 'reduce') localStorage.setItem(STORAGE_KEY, 'reduce');
			else localStorage.removeItem(STORAGE_KEY);
		} catch {
			// No storage: the choice lasts until the page reloads.
		}
		this.#apply();
		this.#notify(before);
	}

	/** Calls `fn` whenever `reduced` changes, for loops that run outside Svelte's effects. Returns the unsubscribe. */
	watch(fn: (reduced: boolean) => void): () => void {
		this.#listeners.add(fn);
		return () => this.#listeners.delete(fn);
	}

	#apply() {
		document.documentElement.toggleAttribute('data-reduce-motion', this.setting === 'reduce');
	}

	#notify(before: boolean) {
		const now = this.reduced;
		if (now !== before) for (const fn of this.#listeners) fn(now);
	}
}

export const motion = new Motion();

/** Whether to still motion now. Safe to call during SSR, where it is false. */
export function reducedMotion(): boolean {
	return motion.reduced;
}
