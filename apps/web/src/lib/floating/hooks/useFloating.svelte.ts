/**
 * useFloating Hook
 *
 * Core positioning hook that wraps @floating-ui/dom.
 * Handles automatic repositioning on scroll/resize for HTMLElement anchors.
 */

import {
	computePosition,
	autoUpdate,
	flip,
	shift,
	offset,
	arrow as arrowMiddleware,
	type Middleware
} from '@floating-ui/dom';
import type { Anchor, FloatingOptions, FloatingState } from '../core/types';
import { isVirtualAnchor } from '../core/types';

export function useFloating(
	getAnchor: () => Anchor | null,
	getFloating: () => HTMLElement | null,
	getArrow: () => HTMLElement | null = () => null,
	options: FloatingOptions = {}
) {
	const {
		placement = 'bottom-start',
		strategy = 'fixed',
		offset: offsetValue = 8,
		flip: enableFlip = true,
		shift: enableShift = true,
		padding = 8
	} = options;

	let state = $state<FloatingState>({
		x: 0,
		y: 0,
		placement
	});

	$effect(() => {
		const anchor = getAnchor();
		const floatingEl = getFloating();
		const arrowEl = getArrow();

		if (!anchor || !floatingEl) return;

		// Capture for closure
		const floating = floatingEl;

		// Convert virtual anchor to Floating UI format
		const reference = isVirtualAnchor(anchor)
			? {
					getBoundingClientRect: () => ({
						x: anchor.x,
						y: anchor.y,
						top: anchor.y,
						left: anchor.x,
						bottom: anchor.y + anchor.height,
						right: anchor.x + anchor.width,
						width: anchor.width,
						height: anchor.height
					})
				}
			: anchor;

		// Build middleware stack
		const middleware: Middleware[] = [];
		if (offsetValue) middleware.push(offset(offsetValue));
		if (enableFlip) middleware.push(flip());
		if (enableShift) middleware.push(shift({ padding }));
		if (arrowEl) middleware.push(arrowMiddleware({ element: arrowEl }));

		async function updatePosition() {
			const result = await computePosition(reference, floating, {
				placement,
				strategy,
				middleware
			});

			state = {
				x: result.x,
				y: result.y,
				placement: result.placement,
				arrowX: result.middlewareData.arrow?.x,
				arrowY: result.middlewareData.arrow?.y
			};
		}

		// For HTMLElement anchors, use autoUpdate for scroll/resize tracking
		if (!isVirtualAnchor(anchor)) {
			return autoUpdate(anchor, floating, updatePosition);
		} else {
			// Virtual anchors: compute once (context menus reposition on new show())
			updatePosition();
		}
	});

	return {
		get state() {
			return state;
		},
		get style() {
			return `position: ${strategy}; left: ${state.x}px; top: ${state.y}px;`;
		}
	};
}
