/**
 * Lucide, the set people pick from for a page, chat, pin or project.
 *
 * Lucide is drawn with real strokes, round caps and round joins, like Atlas,
 * so its line weight can be set to Atlas's. It ships at 2 units on a
 * 24-unit grid; 1.875 is 1.25px at the 16px the sidebar draws, the same line
 * as the Atlas glyphs beside it.
 *
 * Loaded from the bundle, once, on first need. Without it, iconify fetches an
 * unregistered name from its public API, which a box may not reach and which
 * should not learn what someone named their pages after.
 */
import { addCollection } from '@iconify/svelte';

const ATLAS_STROKE = '1.875';

let loading: Promise<string[]> | null = null;

/** Register the collection and resolve to every icon name, `lucide:` prefixed. */
export function loadLucide(): Promise<string[]> {
	loading ??= import('@iconify-json/lucide').then(({ icons }) => {
		const data = typeof icons === 'string' ? JSON.parse(icons) : icons;
		for (const icon of Object.values(data.icons) as { body: string }[]) {
			icon.body = icon.body.replaceAll('stroke-width="2"', `stroke-width="${ATLAS_STROKE}"`);
		}
		addCollection(data);
		return Object.keys(data.icons)
			.map((n) => `lucide:${n}`)
			.sort();
	});
	return loading;
}
