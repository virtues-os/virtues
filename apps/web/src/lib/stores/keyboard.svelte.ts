/**
 * The keyboard inset bridge.
 *
 * WKWebView does not resize the *layout* viewport when the software keyboard
 * appears. `100vh`, `position: fixed` and `env(safe-area-inset-bottom)` all
 * keep their pre-keyboard values, and the keyboard is simply painted over the
 * page — so bottom-anchored chrome (the tab bar, the composer, the editor's
 * footer) ends up underneath it with nothing in the CSS to say so.
 *
 * The one API that knows a keyboard exists is `visualViewport`. This module
 * turns it into a single custom property:
 *
 *     --keyboard-inset: <px of the bottom edge the keyboard is covering>
 *
 * That property is the interface. Consumers depend on it, not on how it was
 * derived — which matters, because the derivation is the part that may yet
 * have to change: tauri#10631 reports combinations where `visualViewport`
 * does not shrink for the keyboard at all. If the on-device probe (This
 * Device → "Keyboard probe") shows that is true here, the fix is to replace
 * `measure()` with a Tauri event fed by `UIKeyboardWillShow`/`WillHide` on
 * the Swift side. Nothing downstream changes.
 *
 * Until then the property is 0 everywhere it can't be measured, which is
 * exactly the behaviour the app had before this existed — so a wrong guess
 * degrades to the status quo rather than to a broken layout.
 */

import { mobileLayout } from '$lib/stores/mobileLayout.svelte';

/**
 * Below this the inset is treated as nothing. Rounding, rubber-banding and
 * the odd 1–2px of visual-viewport drift all land here; a real keyboard (even
 * a bare accessory bar) is far larger.
 */
const NOISE_PX = 24;

let height = $state(0);

/**
 * Px of the layout viewport's bottom edge currently covered.
 *
 * NOT `innerHeight - visualViewport.height`, which is the tempting version and
 * is wrong precisely when it matters: iOS scrolls the *visual* viewport to
 * bring a focused field above the keyboard, and that scroll shows up in
 * `offsetTop`. Without the offset term, a composer at the bottom of the screen
 * — the whole reason this exists — measures short by however far iOS moved.
 */
function measure(): number {
	const vv = window.visualViewport;
	if (!vv) return 0;
	const covered = window.innerHeight - (vv.height + vv.offsetTop);
	return covered > NOISE_PX ? Math.round(covered) : 0;
}

function apply(next: number): void {
	if (next === height) return;
	height = next;
	document.documentElement.style.setProperty('--keyboard-inset', `${next}px`);
}

if (typeof window !== 'undefined' && mobileLayout.isMobile) {
	const vv = window.visualViewport;
	const onChange = () => apply(measure());

	// Both events matter: `resize` is the keyboard opening or closing, `scroll`
	// is iOS panning the visual viewport to reveal the focused field. Either
	// changes how much of the bottom edge is hidden.
	vv?.addEventListener('resize', onChange);
	vv?.addEventListener('scroll', onChange);
	// The layout viewport changes on rotation, and the inset is relative to it.
	window.addEventListener('resize', onChange);

	apply(measure());
}

export const keyboard = {
	/** Px of the bottom edge the keyboard covers; 0 when closed or unmeasurable. */
	get height(): number {
		return height;
	},
	/** True while the keyboard is up. */
	get open(): boolean {
		return height > 0;
	}
};

// Reachable from the on-device probe and the console while the source of the
// measurement is still being settled.
if (typeof window !== 'undefined') {
	(window as unknown as { virtuesKeyboard: typeof keyboard }).virtuesKeyboard = keyboard;
}
