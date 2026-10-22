/**
 * Motion tokens for code that animates in JS.
 *
 * Svelte's `animate:` and `transition:` and the Web Animations API cannot
 * read a CSS easing or a `prefers-reduced-motion` media query. These helpers
 * give them the same curves and durations the stylesheets use (themes.css,
 * "The scale").
 */

/** A CSS `cubic-bezier(x1, y1, x2, y2)` as an easing function of progress 0..1. */
export function cubicBezier(x1: number, y1: number, x2: number, y2: number): (p: number) => number {
	const cx = 3 * x1;
	const bx = 3 * (x2 - x1) - cx;
	const ax = 1 - cx - bx;
	const cy = 3 * y1;
	const by = 3 * (y2 - y1) - cy;
	const ay = 1 - cy - by;
	const x = (t: number) => ((ax * t + bx) * t + cx) * t;
	const y = (t: number) => ((ay * t + by) * t + cy) * t;
	const dx = (t: number) => (3 * ax * t + 2 * bx) * t + cx;

	return (p) => {
		if (p <= 0) return 0;
		if (p >= 1) return 1;
		// Newton's method finds the curve's t for this x in a few steps; where
		// the slope is too flat for it, bisection does.
		let t = p;
		for (let i = 0; i < 8; i++) {
			const err = x(t) - p;
			if (Math.abs(err) < 1e-6) return y(t);
			const d = dx(t);
			if (Math.abs(d) < 1e-6) break;
			t = Math.min(1, Math.max(0, t - err / d));
		}
		let lo = 0;
		let hi = 1;
		t = p;
		for (let i = 0; i < 32; i++) {
			const v = x(t);
			if (Math.abs(v - p) < 1e-6) break;
			if (v < p) lo = t;
			else hi = t;
			t = (lo + hi) / 2;
		}
		return y(t);
	};
}

/** `--ease-premium`. */
export const easePremium = cubicBezier(0.2, 0, 0, 1);

/** A token on the root as written, or `fallback` when it is not set. */
export function tokenString(token: string, fallback: string): string {
	if (typeof document === 'undefined') return fallback;
	return getComputedStyle(document.documentElement).getPropertyValue(token).trim() || fallback;
}

/**
 * A duration token's value in ms, read from the root's computed style, or
 * `fallback` when it is missing or not in ms or s.
 */
export function motionMs(token: string, fallback: number): number {
	const raw = tokenString(token, '');
	const n = parseFloat(raw);
	if (!Number.isFinite(n)) return fallback;
	if (raw.endsWith('ms')) return n;
	if (raw.endsWith('s')) return n * 1000;
	return fallback;
}

/** A length token's value in px, or `fallback` when it is missing or not in px. */
export function lengthPx(token: string, fallback: number): number {
	const raw = tokenString(token, '');
	const n = parseFloat(raw);
	return Number.isFinite(n) && raw.endsWith('px') ? n : fallback;
}
