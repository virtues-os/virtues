/**
 * "Press ⌘ to see what the number keys do."
 *
 * Reveals the ⌘1/⌘2 pane labels while the accelerator is down, so the shortcut
 * is discoverable without a permanent badge cluttering the toolbar.
 *
 * FIRES ON KEYDOWN, with no hold delay. A hint you have to wait for isn't a
 * hint — by the time a 400ms gate opened, anyone reaching for ⌘1 had already
 * pressed 1, so the reveal only ever appeared for people who were hesitating.
 *
 * The delay was there to stop the labels flashing during ordinary chords (⌘S,
 * ⌘K, ⌘N all start with ⌘). Two things make that cheap enough to accept:
 *
 *  · the moment a non-modifier key joins, `#onKeydown` cancels — so a chord
 *    shows the label for only the ~80ms between the two presses;
 *  · the flip is a transition, not a jump, so an aborted chord reads as a
 *    label that started to move and settled back, rather than a blink.
 *
 * The guard that matters is not duration but PURITY: only a bare accelerator
 * arms the hint. ⌘⇧4 holds ⌘ for as long as it takes to drag a screenshot
 * marquee, which is why the old badge kept turning up in screenshots of this
 * very app.
 */

import { isAppleKeyboard } from '$lib/utils/platform';

class ModifierHintStore {
	visible = $state(false);
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
		// ...and only a BARE one. Duration alone can't tell "thinking about
		// panes" apart from "using the OS": ⌘⇧4 holds ⌘ for as long as it takes
		// to drag a screenshot marquee, and ⌘⌥/⌘⌃ chords hold it too. Every one
		// of those was firing the hint, which is why it kept appearing in
		// screenshots of the app — the act of capturing one triggered it.
		if (e.shiftKey || e.altKey || (isAppleKeyboard ? e.ctrlKey : e.metaKey)) {
			this.#cancel();
			return;
		}
		// Synchronously, in the keydown handler — not on a timer, not on a
		// microtask. Anything deferred costs a frame the user can feel, and the
		// whole point is that the labels are already there when you look.
		// keydown repeats while a modifier is held; the guard makes that a no-op.
		if (this.visible) return;
		this.visible = true;
	};

	#onKeyup = (e: KeyboardEvent) => {
		if (this.#isAccel(e)) this.#cancel();
	};

	#cancel = () => {
		this.visible = false;
	};
}

export const modifierHint = new ModifierHintStore();
