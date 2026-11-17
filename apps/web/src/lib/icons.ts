/**
 * Icon Registry - Pre-imports all icons used in the app for instant rendering
 *
 * This eliminates the Cumulative Layout Shift (CLS) caused by Iconify's default
 * behavior of fetching icons from the API after page load.
 */

import { addCollection, addIcon } from '@iconify/svelte';
import { GLYPHS as ATLAS_GLYPHS } from '$lib/components/sidebar/AtlasIcon.svelte';
import { LEGACY_ICONS } from '$lib/legacyIcons';
import myna from '$lib/myna-icons.json';

// Myna (mynaui:), the app's icon set. Only the icons the source names are in
// the bundle; scripts/myna-icons.mjs writes the file.
addCollection(myna);

// Simple Icons
import siNotion from '@iconify-icons/simple-icons/notion';
import siUnsplash from '@iconify-icons/simple-icons/unsplash';

addIcon('simple-icons:notion', siNotion);
addIcon('simple-icons:unsplash', siUnsplash);

// Custom Icons — Virtues ∴ triangle logo (three dots)
// Sized to look good at 16px: centered in 24x24 viewBox with adequate dot radius
addIcon('virtues:logo', {
	body: '<circle cx="12" cy="5" r="3" fill="currentColor"/><circle cx="4.5" cy="18" r="3" fill="currentColor"/><circle cx="19.5" cy="18" r="3" fill="currentColor"/>',
	width: 24,
	height: 24,
});

// Atlas — the shell's names for its own things (AtlasIcon.svelte), for
// surfaces that take an icon by name: tabs, menus, ⌘K, a pin's stored icon.
// Each draws the Myna icon its row in the table names.
for (const [name, icon] of Object.entries(ATLAS_GLYPHS)) {
	const data = myna.icons[icon.slice('mynaui:'.length) as keyof typeof myna.icons];
	if (data) addIcon(`atlas:${name}`, { ...data, width: 24, height: 24 });
}

// The dot: what a pin, page or chat wears when no icon has been chosen, in
// its color. Filled because a filled mark shows its color across its whole
// area, where a 1.25px line at 14px shows only a trace of it.
addIcon('shape:circle', {
	body: '<circle cx="8" cy="8" r="5.75" fill="currentColor"/>',
	width: 16,
	height: 16,
});

/**
 * The name to draw for an icon name from anywhere: the Myna name for a
 * Remix or Phosphor name stored or sent before the switch, else the name
 * itself.
 */
export function resolveIcon(icon: string): string {
	return LEGACY_ICONS[icon] ?? icon;
}

// Export flag to indicate icons are registered
export const iconsRegistered = true;
