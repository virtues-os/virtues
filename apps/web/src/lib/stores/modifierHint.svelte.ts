/**
 * "Hold ⌘ to see what the number keys do."
 *
 * Reveals the ⌘1/⌘2 pane labels while the accelerator is held down, so the
 * shortcut is discoverable without a permanent badge cluttering the toolbar.
 *
 * Gated on a hold, not on keydown. ⌘ is the first half of ⌘S, ⌘K, ⌘N and every
 * other chord in the app, so firing immediately would flash the labels dozens
 * of times an hour at people who were never asking. 400ms is long enough that
 * only a deliberate hold reaches it.
 */

import { isAppleKeyboard } from '$lib/utils/platform';

const HOLD_MS = 400;

class ModifierHintStore {
	visible = $state(false);
	#timer: ReturnType<typeof setTimeout> | null = null;
	#listening = false;

	start(): () => void {
		if (this.#listening || typeof window === 'undefined') return () => {};
		this.#listening = true;

		window.addEventListener('keydown', this.#onKeydown);
		window.addEventListener('keyup', this.#onKeyup);
		// Losing focus mid-hold would otherwise strand the labels on screen.
		window.addEventListener('blur', this.#cancel);

		return () => {
			window.removeEventListener('keydown', this.#onKeydown);
			window.removeEventListener('keyup', this.#onKeyup);
			window.removeEventListener('blur', this.#cancel);
			this.#cancel();
			this.#listening = false;
		};
	}

	#isAccel(e: KeyboardEvent): boolean {
		return isAppleKeyboard ? e.key === 'Meta' : e.key === 'Control';
	}

	#onKeydown = (e: KeyboardEvent) => {
		// Only a bare accelerator arms the hint. Once another key joins it the
		// user is mid-chord and has already decided what they want.
		if (!this.#isAccel(e)) {
			this.#cancel();
			return;
		}
		if (this.#timer || this.visible) return;
		this.#timer = setTimeout(() => {
			this.visible = true;
			this.#timer = null;
		}, HOLD_MS);
	};

	#onKeyup = (e: KeyboardEvent) => {
		if (this.#isAccel(e)) this.#cancel();
	};

	#cancel = () => {
		if (this.#timer) {
			clearTimeout(this.#timer);
			this.#timer = null;
		}
		this.visible = false;
	};
}

export const modifierHint = new ModifierHintStore();
