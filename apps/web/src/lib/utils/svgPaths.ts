/**
 * SVG Path Extraction Utility
 *
 * Extracts SVG `d` attributes from Iconify icon data for use with GSAP MorphSVG.
 * Handles single-path, multi-path, and non-path (circle/rect) icons.
 */

import { getIcon } from '@iconify/svelte';

// Virtues ∴ triangle logo as a single compound path (three circles, r=3)
// Must match the icon registered in icons.ts: cx=12,cy=5 / cx=4.5,cy=18 / cx=19.5,cy=18
export const VIRTUES_LOGO_PATH =
	'M12 2a3 3 0 1 0 0 6 3 3 0 1 0 0-6ZM4.5 15a3 3 0 1 0 0 6 3 3 0 1 0 0-6ZM19.5 15a3 3 0 1 0 0 6 3 3 0 1 0 0-6Z';

// Default folder icon path (ri:folder-line) — used as fallback
export const DEFAULT_FOLDER_PATH =
	'M4 5v14h16V7h-8.414l-2-2H4Zm8.414 0H21a1 1 0 0 1 1 1v14a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h7.414l2 2Z';

/**
 * Extract the SVG path `d` attribute(s) from an Iconify icon name.
 * Returns null for emoji icons or icons that can't be parsed.
 */
export function getIconPath(iconName: string): string | null {
	if (!iconName || !iconName.includes(':')) return null;

	const iconData = getIcon(iconName);
	if (!iconData) return null;

	return extractPathFromBody(iconData.body);
}

/**
 * Extract path `d` attribute(s) from an SVG body string.
 * Combines multiple <path> elements into a single compound path.
 */
function extractPathFromBody(body: string): string | null {
	const paths: string[] = [];

	// Extract all <path d="..."> values
	const pathRegex = /\bd="([^"]+)"/g;
	let match: RegExpExecArray | null;
	while ((match = pathRegex.exec(body)) !== null) {
		paths.push(match[1]);
	}

	if (paths.length > 0) {
		return paths.join(' ');
	}

	// Try to handle <circle>, <rect> etc. by converting to approximate paths
	// For now, return null and let the caller use a fallback
	return null;
}
