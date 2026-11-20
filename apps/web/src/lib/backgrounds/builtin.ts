/**
 * Styles the app draws itself and ships with, so nothing is downloaded for
 * them: today, Dots (agents/plan/backgrounds-plan.md, slice 5). They resolve
 * like any installed style (lib/backgrounds/resolve.ts), and the store lists
 * them before the server's.
 */
import type { DotsStyle, StyleManifest } from './resolve';

export const DOTS: DotsStyle = {
	version: 1,
	style: 'dots',
	title: 'Dots',
	renderer: 'dots',
	collections: [
		{
			id: 'current',
			title: 'Current',
			pictures: [
				{ id: 'fine', title: 'Fine', dots: { motion: 'drift', gap: 14, radius: 1.1, reach: 6, period: 40 } },
				{ id: 'wide', title: 'Wide', dots: { motion: 'drift', gap: 26, radius: 1.6, reach: 10, period: 60 } },
			],
		},
		{
			id: 'swell',
			title: 'Swell',
			pictures: [
				{ id: 'tide', title: 'Tide', dots: { motion: 'swell', gap: 18, radius: 1.4, reach: 2, period: 14 } },
				{ id: 'breath', title: 'Breath', dots: { motion: 'swell', gap: 24, radius: 1.8, reach: 3, period: 20 } },
			],
		},
		{
			id: 'rain',
			title: 'Rain',
			pictures: [
				{ id: 'drizzle', title: 'Drizzle', dots: { motion: 'rain', gap: 16, radius: 1, reach: 3, period: 30 } },
				{ id: 'snowfall', title: 'Snowfall', dots: { motion: 'rain', gap: 24, radius: 1.8, reach: 8, period: 50 } },
			],
		},
	],
};

/** Every built-in style, in the order the pickers list them. */
export const BUILTIN_STYLES: StyleManifest[] = [DOTS];

export function isBuiltin(style: string): boolean {
	return BUILTIN_STYLES.some((s) => s.style === style);
}
