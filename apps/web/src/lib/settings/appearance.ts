/**
 * The appearance settings the settings agent can draft, which live in this
 * browser (the theme and accent are also saved to the profile). The draft
 * request sends their values (`appearanceNow`), since the server can't see
 * them, and an accepted change is applied here (`applyAppearance`), never by
 * the server.
 */
import type { AppearanceValues } from '$lib/api/client';
import { applyAccent, applyTheme, getAccent, getThemeChoice, isAccent, isValidTheme, setAccent, setTheme } from '$lib/utils/theme';
import { glass, type GlassLevel } from '$lib/stores/glass.svelte';
import { pageDisplay, type FontMode, type TextSize } from '$lib/stores/pageDisplay.svelte';
import { motion } from '$lib/stores/motion.svelte';

export function appearanceNow(): AppearanceValues {
	return {
		theme: getThemeChoice(),
		accent: getAccent(),
		glass: glass.level,
		text_size: pageDisplay.textSize,
		reading_font: pageDisplay.fontMode,
		reduce_motion: motion.setting === 'reduce',
	};
}

/**
 * Put an appearance value on. With `save` false (a preview) the theme and
 * accent are painted without saving to the profile, so a preview that is put
 * back leaves nothing in Changed lately. The other four are kept in this
 * browser either way.
 */
export async function applyAppearance(field: string, value: unknown, save: boolean): Promise<void> {
	switch (field) {
		case 'theme':
			if (typeof value === 'string' && isValidTheme(value)) await (save ? setTheme(value) : applyTheme(value));
			return;
		case 'accent':
			if (isAccent(value)) await (save ? setAccent(value) : applyAccent(value));
			return;
		case 'glass':
			glass.set(value as GlassLevel);
			return;
		case 'text_size':
			pageDisplay.setTextSize(value as TextSize);
			return;
		case 'reading_font':
			pageDisplay.setFontMode(value as FontMode);
			return;
		case 'reduce_motion':
			motion.set(value ? 'reduce' : 'device');
			return;
	}
}
