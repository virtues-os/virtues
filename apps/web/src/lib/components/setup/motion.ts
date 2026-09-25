/**
 * Setup's motion system for Svelte transitions: the same three durations
 * and one ease as setup.css, so a transition written in a component and a
 * CSS animation in the stylesheet move alike. Reduced motion resolves every
 * transition at once.
 */
import type { TransitionConfig } from 'svelte/transition';

export const M = { quick: 180, base: 360, slow: 640 } as const;

/** cubic-bezier(0.22, 0.8, 0.24, 1), as a function of t. */
export function ease(t: number): number {
	// Newton's method on the x(t) of the bezier, then y at that t.
	const x1 = 0.22, y1 = 0.8, x2 = 0.24, y2 = 1;
	const bx = (u: number) => 3 * x1 * u * (1 - u) ** 2 + 3 * x2 * u ** 2 * (1 - u) + u ** 3;
	const by = (u: number) => 3 * y1 * u * (1 - u) ** 2 + 3 * y2 * u ** 2 * (1 - u) + u ** 3;
	const dx = (u: number) => 3 * x1 * (1 - u) ** 2 + 6 * (x2 - x1) * u * (1 - u) + 3 * (1 - x2) * u ** 2;
	let u = t;
	for (let i = 0; i < 6; i++) {
		const d = dx(u);
		if (Math.abs(d) < 1e-6) break;
		u -= (bx(u) - t) / d;
		u = Math.min(1, Math.max(0, u));
	}
	return by(u);
}

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
