/**
 * The middleware every floating element is placed with: an offset from its
 * anchor, a flip to the other side when it does not fit, and a shift to
 * stay on screen.
 *
 * While the software keyboard is up, the space a floating element may use
 * ends at the keyboard's top edge. WKWebView paints the keyboard over the
 * layout viewport without resizing it, so a bar placed in that viewport's
 * bottom (the selection toolbar, a suggestion's Accept and Reject) would
 * sit behind the keys: it flips above its anchor, or slides up to the
 * keyboard's edge, instead.
 */

import { flip, offset, shift, type Middleware } from '@floating-ui/dom';
import type { FloatingOptions } from './types';

/** Px of the bottom edge the software keyboard covers (`stores/keyboard.svelte.ts` sets it). */
export function keyboardInset(): number {
	if (typeof document === 'undefined') return 0;
	return parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--keyboard-inset')) || 0;
}

/**
 * The middleware for `options`, with `inset` px of the window's bottom
 * covered by the keyboard: then flipping and shifting keep to the part
 * above it.
 */
export function floatingMiddleware(
	options: Pick<FloatingOptions, 'offset' | 'flip' | 'shift' | 'padding'>,
	inset = 0,
	view: { width: number; height: number } = { width: window.innerWidth, height: window.innerHeight },
): Middleware[] {
	const { offset: offsetValue = 8, flip: enableFlip = true, shift: enableShift = true, padding = 8 } = options;
	const above =
		inset > 0 ? { rootBoundary: { x: 0, y: 0, width: view.width, height: Math.max(0, view.height - inset) } } : {};
	const middleware: Middleware[] = [];
	if (offsetValue) middleware.push(offset(offsetValue));
	if (enableFlip) middleware.push(flip(above));
	if (enableShift) middleware.push(shift({ padding, ...above, ...(inset > 0 ? { crossAxis: true } : {}) }));
	return middleware;
}
