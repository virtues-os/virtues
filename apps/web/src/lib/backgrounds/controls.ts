/**
 * What the home control and the Settings picker do with the background
 * store: step to the picture before or after, keep today's picture, name
 * the current one, and draw thumbnails. These load with the pickers, never
 * at launch (agents/plan/backgrounds-plan.md, "The home control").
 */
import { background, fileUrl } from '$lib/stores/background.svelte';
import { formatAddress, parseSetting, pool, type Picture } from './resolve';
import { narrowest, stepFrom } from './browse';

/** Whether the setting changes picture each day. */
export function isRotating(): boolean {
	const s = parseSetting(background.setting);
	return s.kind === 'path' && !s.picture;
}

/** Today's picture with the names of its collection, or null when there is none. */
export function current(): { address: string; title: string; collectionTitle: string } | null {
	const today = background.today;
	if (!today || !background.styles) return null;
	const [style, collection] = today.address.split('/');
	const c = background.styles.find((s) => s.style === style)?.collections.find((x) => x.id === collection);
	return { address: today.address, title: today.picture[background.appearance].title, collectionTitle: c?.title ?? collection };
}

/**
 * The previous or next picture. Under a rotating setting it is today's
 * picture until midnight; under one picture it becomes the setting.
 */
export async function step(dir: 1 | -1): Promise<void> {
	await background.ensureStyles();
	const at = background.today?.address ?? '';
	if (isRotating()) {
		const next = stepFrom(background.pool, at, dir);
		if (next) await background.chooseForToday(formatAddress(next));
		return;
	}
	const setting = parseSetting(background.setting);
	if (setting.kind !== 'path' || !background.styles) return;
	const siblings = pool(background.styles, { kind: 'path', style: setting.style, collection: setting.collection });
	const next = stepFrom(siblings, at, dir);
	if (next) await background.set(formatAddress(next));
}

/** Keep today's picture: the setting becomes that one picture. */
export async function keep(): Promise<void> {
	const address = background.today?.address;
	if (address) await background.set(address);
}

/** A small picture for a thumbnail, in the current appearance. */
export function thumbnail(style: string, picture: Picture): string {
	return narrowest(picture[background.appearance], fileUrl(style));
}
