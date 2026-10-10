/**
 * Where a menu opened at a point sits: inside the window, and above the
 * software keyboard while it is up. WKWebView paints the keyboard over the
 * page without resizing it, so `innerHeight` reaches under the keys, and a
 * menu long-pressed open in a page being typed in (the editor keeps the
 * focus, so the keyboard stays) would open behind them.
 */

import { keyboardInset } from '$lib/floating/core/middleware';

/** The part of the window a menu may use: its width, and its height above the keyboard. */
export function menuRoom(): { width: number; height: number } {
	if (typeof window === 'undefined') return { width: Infinity, height: Infinity };
	return { width: window.innerWidth, height: Math.max(0, window.innerHeight - keyboardInset()) };
}

/** `pos` moved so a menu of `size` there stays inside `room`, `padding` from its edges. */
export function fitMenu(
	pos: { x: number; y: number },
	size: { width: number; height: number },
	room: { width: number; height: number } = menuRoom(),
	padding = 8,
): { x: number; y: number } {
	let { x, y } = pos;
	if (x + size.width + padding > room.width) x = room.width - size.width - padding;
	if (x < padding) x = padding;
	if (y + size.height + padding > room.height) y = room.height - size.height - padding;
	if (y < padding) y = padding;
	return { x, y };
}
