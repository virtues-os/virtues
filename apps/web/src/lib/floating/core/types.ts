/**
 * Floating UI Type Definitions
 *
 * Shared types for the floating UI system.
 */

import type { Placement, Strategy } from '@floating-ui/dom';

export type { Placement, Strategy };

export interface FloatingOptions {
	/** Where to place the floating element relative to anchor */
	placement?: Placement;
	/** Positioning strategy */
	strategy?: Strategy;
	/** Offset from anchor in pixels */
	offset?: number;
	/** Enable automatic flipping when constrained */
	flip?: boolean;
	/** Enable shifting to stay in viewport */
	shift?: boolean;
	/** Padding from viewport edges */
	padding?: number;
	/** Show arrow pointer */
	arrow?: boolean;
}

export interface FloatingState {
	x: number;
	y: number;
	placement: Placement;
	arrowX?: number;
	arrowY?: number;
}

export interface FloatingContext {
	open: boolean;
	setOpen: (open: boolean) => void;
	anchorEl: HTMLElement | null;
	floatingEl: HTMLElement | null;
	arrowEl: HTMLElement | null;
	state: FloatingState;
}

/**
 * Virtual anchor for context menus and click-positioned elements.
 * Represents a point or rect in viewport coordinates.
 */
export interface VirtualAnchor {
	x: number;
	y: number;
	width: number;
	height: number;
}

export type Anchor = HTMLElement | VirtualAnchor;

/**
 * Type guard to check if an anchor is a virtual anchor (not an HTMLElement)
 */
export function isVirtualAnchor(anchor: Anchor): anchor is VirtualAnchor {
	return !(anchor instanceof HTMLElement);
}
