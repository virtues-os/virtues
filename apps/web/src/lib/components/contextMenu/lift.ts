/**
 * After a held press opens a menu, the press goes on: the finger lifts, and
 * the platform may answer the hold itself. Either would land on the menu's
 * backdrop, which closes the menu on a click and on a `contextmenu`: the
 * click that follows the lift (WebKit sends one), and the `contextmenu`
 * Android fires at its own touch-and-hold delay, which an Accessibility
 * setting lengthens past the 450 ms ours opens at. Both are swallowed from
 * the hold until the finger lifts and the click after it has come.
 */

/** How long after the lift its click can still come. */
const LIFT_CLICK_MS = 700;
/** How long a held finger is waited for, should its lift never reach the window. */
const HOLD_LIMIT_MS = 5000;

/** Swallow the click and the `contextmenu` that follow a hold that opened a menu. */
export function swallowLift(target: Window = window): void {
	const swallow = (e: Event) => {
		e.preventDefault();
		e.stopPropagation();
		if (e.type === 'click') stop();
	};
	let timer = setTimeout(() => stop(), HOLD_LIMIT_MS);
	const lifted = () => {
		clearTimeout(timer);
		timer = setTimeout(() => stop(), LIFT_CLICK_MS);
	};
	const stop = () => {
		clearTimeout(timer);
		target.removeEventListener('click', swallow, true);
		target.removeEventListener('contextmenu', swallow, true);
		target.removeEventListener('pointerup', lifted, true);
		target.removeEventListener('pointercancel', lifted, true);
	};
	target.addEventListener('click', swallow, true);
	target.addEventListener('contextmenu', swallow, true);
	target.addEventListener('pointerup', lifted, true);
	target.addEventListener('pointercancel', lifted, true);
}
