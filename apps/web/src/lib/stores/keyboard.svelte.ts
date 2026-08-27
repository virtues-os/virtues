/**
 * The keyboard inset bridge.
 *
 * WKWebView does not resize the *layout* viewport when the software keyboard
 * appears. `100vh`, `position: fixed` and `env(safe-area-inset-bottom)` all
 * keep their pre-keyboard values, and the keyboard is simply painted over the
 * page — so bottom-anchored chrome (the composer, the editor's footer) ends
 * up underneath it with nothing in the CSS to say so.
 *
 * This module turns "how much of the bottom edge the keyboard covers" into a
 * single custom property:
 *
 *     --keyboard-inset: <px>
 *
 * TWO SOURCES, in strict preference order:
 *
 * 1. **The native bridge.** KeyboardShell.swift (in the phone shell) observes
 *    `keyboardWillChangeFrame` and calls `window.__virtuesKeyboardInset(px,
 *    durationMs)` with the final covered height and UIKit's animation
 *    duration — BEFORE the keyboard has moved. This module then animates the
 *    property itself, frame-by-frame on that clock, which is what makes the
 *    composer move WITH the keys instead of chasing them. (Frame-by-frame on
 *    purpose: a CSS transition keyed on the var()-derived padding simply
 *    never ran in testing — the padding sat at its old value for the whole
 *    duration.) Once the bridge has spoken, it owns the property.
 *
 * 2. **visualViewport, as fallback.** Fires repeatedly DURING the animation
 *    with intermediate values (the staircase that read as a "finicky dance"
 *    on device), and tauri#10631 reports combinations where it doesn't fire
 *    at all — it exists so a browser tab below the breakpoint still works.
 *
 * IMPORTANT: nothing imports this module for its exports — it acts entirely
 * through the custom property — so something must load it for its side
 * effects. MobileShell does. (The old tab bar used to, and deleting it
 * silently orphaned this module: the phone shipped with no inset at all.)
 */

import { mobileLayout } from './mobileLayout.svelte';

/**
 * Below this the inset is treated as nothing. Rounding, rubber-banding and
 * the odd 1–2px of visual-viewport drift all land here; a real keyboard (even
 * a bare accessory bar) is far larger.
 */
const NOISE_PX = 24;

let height = $state(0);
let nativeBridge = false;

// The px currently painted, distinct from `height` (the target): mid-flight
// they differ, and an interrupted animation must resume from the paint.
let painted = 0;
let raf = 0;
let settle: ReturnType<typeof setTimeout> | null = null;

function cancelFlight(): void {
	cancelAnimationFrame(raf);
	if (settle) {
		clearTimeout(settle);
		settle = null;
	}
}

function setVar(px: number): void {
	painted = px;
	document.documentElement.style.setProperty('--keyboard-inset', `${px}px`);
}

/** Jump (fallback path, or zero-duration native events). */
function apply(next: number): void {
	cancelFlight();
	height = next;
	if (next !== painted) setVar(next);
}

/** Animate to `target` over the keyboard's own duration. */
function animateTo(target: number, durationMs: number): void {
	cancelFlight();
	height = target;
	const from = painted;
	if (
		durationMs <= 0 ||
		Math.abs(target - from) < 1 ||
		(typeof matchMedia !== 'undefined' &&
			matchMedia('(prefers-reduced-motion: reduce)').matches)
	) {
		if (target !== painted) setVar(target);
		return;
	}
	const t0 = performance.now();
	const tick = (now: number) => {
		const t = Math.min(1, (now - t0) / durationMs);
		// Cubic ease-out — close enough to UIKit's keyboard curve that the
		// composer and the keys read as one object.
		const p = 1 - Math.pow(1 - t, 3);
		setVar(Math.round(from + (target - from) * p));
		if (t < 1) raf = requestAnimationFrame(tick);
	};
	raf = requestAnimationFrame(tick);
	// rAF stops cold in a hidden webview (app switch mid-keyboard); the
	// backstop lands the final value regardless, just late.
	settle = setTimeout(() => {
		cancelAnimationFrame(raf);
		if (painted !== target) setVar(target);
		settle = null;
	}, durationMs + 80);
}

/**
 * Fallback measurement off visualViewport.
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

if (typeof window !== 'undefined' && mobileLayout.isMobile) {
	// The native side calls this before the keyboard animates; first call
	// permanently claims the property for the bridge.
	(window as unknown as { __virtuesKeyboardInset: (px: number, durationMs: number) => void })
		.__virtuesKeyboardInset = (px, durationMs) => {
		nativeBridge = true;
		animateTo(px >= NOISE_PX ? Math.round(px) : 0, durationMs);
	};

	const vv = window.visualViewport;
	const onChange = () => {
		if (nativeBridge) return;
		apply(measure());
	};

	// Both events matter: `resize` is the keyboard opening or closing, `scroll`
	// is iOS panning the visual viewport to reveal the focused field. Either
	// changes how much of the bottom edge is hidden.
	vv?.addEventListener('resize', onChange);
	vv?.addEventListener('scroll', onChange);
	// The layout viewport changes on rotation, and the inset is relative to it.
	window.addEventListener('resize', onChange);

	onChange();
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
