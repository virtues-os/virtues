/**
 * Styles the app draws itself and ships with, so nothing is downloaded for
 * them: Dots and ASCII (agents/plan/backgrounds-plan.md, "Moving styles"). They resolve
 * like any installed style (lib/backgrounds/resolve.ts), and the store lists
 * them before the server's.
 */
import type { AsciiStyle, DotsStyle, StyleManifest } from './resolve';

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
				{ id: 'fine', title: 'Fine', params: { motion: 'drift', gap: 14, radius: 1.1, reach: 6, period: 40 } },
				{ id: 'wide', title: 'Wide', params: { motion: 'drift', gap: 26, radius: 1.6, reach: 10, period: 60 } },
			],
		},
		{
			id: 'swell',
			title: 'Swell',
			pictures: [
				{ id: 'tide', title: 'Tide', params: { motion: 'swell', gap: 18, radius: 1.4, reach: 2, period: 14 } },
				{ id: 'breath', title: 'Breath', params: { motion: 'swell', gap: 24, radius: 1.8, reach: 3, period: 20 } },
			],
		},
		{
			id: 'rain',
			title: 'Rain',
			pictures: [
				{ id: 'drizzle', title: 'Drizzle', params: { motion: 'rain', gap: 16, radius: 1, reach: 3, period: 30 } },
				{ id: 'snowfall', title: 'Snowfall', params: { motion: 'rain', gap: 24, radius: 1.8, reach: 8, period: 50 } },
			],
		},
	],
};

export const ASCII: AsciiStyle = {
	version: 1,
	style: 'ascii',
	title: 'ASCII',
	renderer: 'ascii',
	collections: [
		{
			id: 'sea',
			title: 'Sea',
			pictures: [
				{ id: 'sharks', title: 'Sharks', params: { scene: 'sharks', cell: 11, speed: 1 } },
				{ id: 'whale', title: 'Whale', params: { scene: 'whale', cell: 12, speed: 1 } },
			],
		},
		{
			id: 'night',
			title: 'Night',
			pictures: [{ id: 'moon', title: 'Moon', params: { scene: 'moon', cell: 10, speed: 1 } }],
		},
	],
};

/** Every built-in style, in the order the pickers list them. */
export const BUILTIN_STYLES: StyleManifest[] = [DOTS, ASCII];

export function isBuiltin(style: string): boolean {
	return BUILTIN_STYLES.some((s) => s.style === style);
}
