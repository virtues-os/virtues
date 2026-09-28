/**
 * Turn the theme through the ∴.
 *
 * The new theme is pushed out of the mark itself: its three dots draw in a
 * breath (anticipation), pop a little past their size, then swell until the
 * three circles meet and cover the window — the new colors arriving through
 * the logo rather than across it. Built on a view transition: the page
 * under the old theme stays as a still frame, the page under the new theme
 * is revealed through a mask of three circles centered on the dots.
 *
 * The keyframes and mask live in app.css (`html.theme-reveal`), because
 * view-transition pseudo-elements only take global styles. Without view
 * transitions, or with reduced motion, the theme just changes.
 */
import { setTheme, type Theme } from '$lib/utils/theme';

/** The ∴'s dots on its 24-unit box: left foot, right foot, apex. */
const DOTS = [
	[4.5, 18],
	[19.5, 18],
	[12, 5],
] as const;

/**
 * @param mark the ∴ as drawn on screen, and the viewBox origin and width it
 *   is drawn with (Hello draws it on a -8..32 box, 40 units wide).
 */
export async function revealTheme(
	theme: Theme,
	mark: { el: Element; origin: number; span: number } | null,
): Promise<void> {
	const doc = document as Document & {
		startViewTransition?: (cb: () => void) => { finished: Promise<void> };
	};
	const reduced = !!window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
	if (!mark || !doc.startViewTransition || reduced) {
		await setTheme(theme);
		return;
	}

	const r = mark.el.getBoundingClientRect();
	const unit = r.width / mark.span;
	const root = document.documentElement;
	DOTS.forEach(([ux, uy], i) => {
		root.style.setProperty(`--rx${i + 1}`, `${r.left + (ux - mark.origin) * unit}px`);
		root.style.setProperty(`--ry${i + 1}`, `${r.top + (uy - mark.origin) * unit}px`);
	});
	// A dot's radius is 3 units.
	root.style.setProperty('--r0', `${3 * unit}px`);
	root.classList.add('theme-reveal');

	const t = doc.startViewTransition(() => {
		// setTheme applies synchronously before it saves, so the new frame
		// is the new theme.
		void setTheme(theme);
	});
	try {
		await t.finished;
	} finally {
		root.classList.remove('theme-reveal');
		for (const k of ['--rx1', '--ry1', '--rx2', '--ry2', '--rx3', '--ry3', '--r0']) root.style.removeProperty(k);
	}
}
