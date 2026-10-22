/**
 * Setup's motion system for Svelte transitions: the same three durations
 * and one ease as setup.css, so a transition written in a component and a
 * CSS animation in the stylesheet move alike. Reduced motion resolves every
 * transition at once.
 */
import type { TransitionConfig } from 'svelte/transition';
import { cubicBezier } from '$lib/utils/motion';

export const M = { quick: 180, base: 360, slow: 640 } as const;

/** cubic-bezier(0.22, 0.8, 0.24, 1), the ease setup.css uses. */
export const ease = cubicBezier(0.22, 0.8, 0.24, 1);

export function reducedMotion(): boolean {
	return typeof window !== 'undefined' && !!window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
}

/** The one entrance: up a little, out of a little blur. */
export function rise(_node: Element, { delay = 0, duration = M.slow, y = 8 }: { delay?: number; duration?: number; y?: number } = {}): TransitionConfig {
	if (reducedMotion()) return { duration: 0 };
	return {
		delay,
		duration,
		easing: ease,
		css: (t) => `opacity:${t};transform:translateY(${(1 - t) * y}px);filter:blur(${(1 - t) * 4}px)`,
	};
}

/** The one exit: quicker than the entrance, a little upward, gone. */
export function sink(_node: Element, { duration = M.quick + 60, y = -6 }: { duration?: number; y?: number } = {}): TransitionConfig {
	if (reducedMotion()) return { duration: 0 };
	return {
		duration,
		easing: (t) => t,
		css: (t) => `opacity:${t};transform:translateY(${(1 - t) * y}px)`,
	};
}
