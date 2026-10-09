/**
 * wheel.ts - which hand is on the map: a mouse's notched wheel, or two
 * fingers on a trackpad (or a Magic Mouse's surface).
 *
 * The Timeline's map pans on two fingers, as Apple Maps does, and zooms on a
 * pinch; a mouse has no pinch and no sideways scroll, so its wheel zooms. A
 * browser sends both as the same `wheel` event, told apart by the same tells
 * MapLibre's scroll zoom reads (maplibre-gl src/ui/handler/scroll_zoom.ts):
 *
 * - Firefox counts a wheel in lines, a trackpad in pixels.
 * - Chrome and Safari on a Mac report one notch as 4.000244140625 px, and an
 *   accelerated turn as a multiple of it; a trackpad's deltas are fractions.
 * - A wheel only turns up and down, so any sideways part is fingers.
 * - A delta under 4 px is fingers; a whole 50 px or more with nothing
 *   sideways is a notch (Windows sends 100 a notch).
 *
 * A gesture keeps the source its first event gave it, so a trackpad's
 * momentum tail, whose deltas can land on any value, never flips to zoom
 * mid-swipe. A pause of GESTURE_GAP_MS starts a new gesture.
 */

export type WheelSource = "mouse" | "trackpad";

/** One wheel notch on a Mac, in Chrome and Safari (MapLibre's wheelZoomDelta). */
export const MAC_NOTCH = 4.000244140625;
/** Longer than this between events, and the next one starts a new gesture. */
export const GESTURE_GAP_MS = 400;

type Wheel = Pick<WheelEvent, "deltaMode" | "deltaX" | "deltaY" | "timeStamp">;

/** The source of one event on its own, or `null` when it gives no tell. */
export function sourceOf(e: Wheel): WheelSource | null {
	if (e.deltaMode !== 0) return "mouse";
	if (e.deltaX !== 0) return "trackpad";
	const y = Math.abs(e.deltaY);
	if (y === 0) return null;
	if (y % MAC_NOTCH === 0) return "mouse";
	if (y < 4) return "trackpad";
	if (Number.isInteger(y) && y >= 50) return "mouse";
	return "trackpad";
}

/** A reader that remembers the gesture: call it with every wheel event. */
export function wheelSource(): (e: Wheel) => WheelSource {
	let kind: WheelSource = "trackpad";
	let last = -Infinity;
	return (e) => {
		const fresh = e.timeStamp - last > GESTURE_GAP_MS;
		last = e.timeStamp;
		if (fresh) kind = sourceOf(e) ?? "trackpad";
		return kind;
	};
}
