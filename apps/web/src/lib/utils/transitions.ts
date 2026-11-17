/**
 * The app's Svelte transitions. Import `fade`, `fly`, `slide`, `scale` and
 * `flip` from here, never from `svelte/transition` or `svelte/animate`.
 *
 * Svelte 5 runs transitions on the Web Animations API, which neither the
 * `prefers-reduced-motion` media query nor the Settings choice in app.css
 * (`:root[data-reduce-motion]`) can reach. Each function here has the same
 * signature as Svelte's and asks `reducedMotion()` first. Under reduced motion
 * a fade stays, short, because it says that something came or went; travel,
 * growth and reordering finish at once.
 *
 * Left unset, the duration and easing come from the motion scale
 * (themes.css): an entrance on `--motion-enter` and `--ease-premium`, an exit
 * on `--motion-exit` and `--ease-exit`, which is quicker and leaves faster
 * than it arrived.
 *
 * TODO(2026-11-17): migrate UniversalDataGrid.svelte, which still imports
 * from svelte/transition and svelte/animate, then add the arch-lint rule that
 * keeps every import here. The file holds another change in progress, so it
 * moves in its own commit.
 */
import {
	fade as svelteFade,
	fly as svelteFly,
	slide as svelteSlide,
	scale as svelteScale,
	type FadeParams,
	type FlyParams,
	type SlideParams,
	type ScaleParams,
	type TransitionConfig,
} from 'svelte/transition';
import type { FlipParams, AnimationConfig } from 'svelte/animate';
import { reducedMotion } from '$lib/stores/motion.svelte';
import { cubicBezier, easePremium, motionMs } from '$lib/utils/motion';

export type { TransitionConfig };

/** `--ease-exit`. */
export const easeExit = cubicBezier(0.4, 0, 1, 1);

/** How long a fade lasts under reduced motion. */
export const REDUCED_FADE_MS = 150;

type Direction = 'in' | 'out' | 'both';
interface TransitionOptions {
	direction?: Direction;
}

/** The scale's duration and easing for a transition playing in `direction`. */
export function timing(direction: Direction = 'in'): { duration: number; easing: (t: number) => number } {
	return direction === 'out'
		? { duration: motionMs('--motion-exit', 120), easing: easeExit }
		: { duration: motionMs('--motion-enter', 180), easing: easePremium };
}

/**
 * Something leaving stops taking the pointer at once, so a click during its
 * exit lands on what is under it. Only for an `out:`, which cannot turn
 * back into an entrance.
 */
function letGo(node: Element, options: TransitionOptions) {
	if (options.direction === 'out') (node as HTMLElement).style.pointerEvents = 'none';
}

function reducedFade(node: Element, duration?: number): TransitionConfig {
	return svelteFade(node, { duration: Math.min(duration ?? REDUCED_FADE_MS, REDUCED_FADE_MS) });
}

export function fade(node: Element, params: FadeParams = {}, options: TransitionOptions = {}): TransitionConfig {
	letGo(node, options);
	if (reducedMotion()) return reducedFade(node, params.duration);
	return svelteFade(node, { ...timing(options.direction), ...params });
}

export function fly(node: Element, params: FlyParams = {}, options: TransitionOptions = {}): TransitionConfig {
	letGo(node, options);
	if (reducedMotion()) return reducedFade(node, params.duration);
	return svelteFly(node, { ...timing(options.direction), ...params });
}

export function scale(node: Element, params: ScaleParams = {}, options: TransitionOptions = {}): TransitionConfig {
	letGo(node, options);
	if (reducedMotion()) return reducedFade(node, params.duration);
	return svelteScale(node, { ...timing(options.direction), ...params });
}

/** A fold: the height opens or closes. It is all travel, so reduced motion makes it instant. */
export function slide(node: Element, params: SlideParams = {}, options: TransitionOptions = {}): TransitionConfig {
	if (reducedMotion()) return { duration: 0 };
	const easing = options.direction === 'out' ? easeExit : easePremium;
	return svelteSlide(node, { duration: motionMs('--motion-state', 150), easing, ...params });
}

/**
 * Items gliding to new places in a keyed list. Unlike Svelte's `flip`, it
 * only moves an item: an item whose size changed takes its new size at once.
 * Svelte's version scales the item from its old size, which stretches its
 * text sideways for the length of the glide (a tab whose title changed).
 */
export function flip(node: Element, { from, to }: { from: DOMRect; to: DOMRect }, params: FlipParams = {}): AnimationConfig {
	const dx = from.left - to.left;
	const dy = from.top - to.top;
	if (reducedMotion() || (dx === 0 && dy === 0)) return { duration: 0 };
	const { delay = 0, duration = motionMs('--motion-state', 150), easing = easePremium } = params;
	const current = getComputedStyle(node).transform;
	const base = current === 'none' ? '' : current;
	return {
		delay,
		duration: typeof duration === 'function' ? duration(Math.hypot(dx, dy)) : duration,
		easing,
		css: (_t, u) => `transform: ${base} translate(${u * dx}px, ${u * dy}px);`,
	};
}

/**
 * A floating surface (a popover, a menu, a hover card) leaving: it fades and
 * settles a little smaller toward where it came from, set by its
 * `transform-origin`, and stops taking the pointer at once so a click lands
 * on what is under it.
 */
export function floatOut(node: Element, params: { duration?: number } = {}): TransitionConfig {
	letGo(node, { direction: 'out' });
	if (reducedMotion()) return reducedFade(node, params.duration);
	const { duration, easing } = timing('out');
	return {
		duration: params.duration ?? duration,
		easing,
		// An outro runs t from 1 down to 0.
		css: (t) => `opacity: ${t}; transform: scale(${0.98 + 0.02 * t})`,
	};
}

/**
 * For a see-through layer that only catches clicks (a menu's backdrop): it
 * stays while what it holds plays its exit, and stops taking the pointer at
 * once so the next click reaches the page.
 */
export function release(node: Element): TransitionConfig {
	letGo(node, { direction: 'out' });
	return { duration: 0 };
}

/**
 * The `transform-origin` that makes a floating surface grow out of, and
 * shrink back into, the side facing what opened it, for a Floating UI
 * placement such as `bottom-start`.
 */
export function originFor(placement: string | undefined): string {
	const [side, align] = (placement ?? 'bottom-start').split('-');
	const along = align === 'start' ? 'start' : align === 'end' ? 'end' : 'center';
	if (side === 'top' || side === 'bottom') {
		const x = along === 'start' ? 'left' : along === 'end' ? 'right' : 'center';
		return `${x} ${side === 'bottom' ? 'top' : 'bottom'}`;
	}
	const y = along === 'start' ? 'top' : along === 'end' ? 'bottom' : 'center';
	return `${side === 'right' ? 'left' : 'right'} ${y}`;
}
