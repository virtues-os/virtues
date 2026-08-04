/**
 * Context gesture — right-click OR long-press, one attachment
 *
 * Every contextual affordance in the editor (link Edit, table row/column
 * operations, media menus) listened for `contextmenu` only, which iOS WebKit
 * does not fire for arbitrary elements — on the Tauri mobile build those
 * menus simply did not exist. This helper is the one place both input paths
 * are wired: mouse right-click stays exactly as it was, and a touch/pen
 * press held for 450ms with less than 8px of drift opens the same menu at
 * the same spot.
 *
 * Consumers get (x, y, target) instead of the raw event, so the same
 * callback serves both gestures; `target` is the pointerdown target on the
 * long-press path, where there is no contextmenu event to ask.
 */

const HOLD_MS = 450;
const DRIFT_PX = 8;

export function onContextGesture(
	el: HTMLElement,
	callback: (x: number, y: number, target: EventTarget | null) => void,
): void {
	el.addEventListener('contextmenu', (e) => {
		e.preventDefault();
		e.stopPropagation();
		callback(e.clientX, e.clientY, e.target);
	});

	let timer: ReturnType<typeof setTimeout> | null = null;
	let startX = 0;
	let startY = 0;

	const cancel = () => {
		if (timer !== null) {
			clearTimeout(timer);
			timer = null;
		}
	};

	el.addEventListener('pointerdown', (e) => {
		// Mouse has a real right-click; the hold path is for touch and pen.
		if (e.pointerType === 'mouse') return;
		startX = e.clientX;
		startY = e.clientY;
		const target = e.target;
		cancel();
		timer = setTimeout(() => {
			timer = null;
			callback(startX, startY, target);
		}, HOLD_MS);
	});

	el.addEventListener('pointermove', (e) => {
		if (timer === null) return;
		if (Math.hypot(e.clientX - startX, e.clientY - startY) > DRIFT_PX) cancel();
	});

	el.addEventListener('pointerup', cancel);
	el.addEventListener('pointercancel', cancel);
}
