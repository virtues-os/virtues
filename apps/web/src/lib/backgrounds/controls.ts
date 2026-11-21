/**
 * What the home control and the Settings picker do with the background
 * store: step to the picture before or after, keep today's picture, name
 * the current one, draw thumbnails, and download a style the server does
 * not have yet. These load with the pickers, never at launch
 * (agents/plan/backgrounds-plan.md, "The home control").
 */
import { request } from '$lib/api/client';
import { background, fileUrl } from '$lib/stores/background.svelte';
import { find, formatAddress, isDrawn, parseSetting, pictureTitle, pool, srcset, widest, type Address, type DotsParams, type Picture } from './resolve';
import { narrowest, stepFrom } from './browse';
import { dotsPreview } from './dots';

/** Whether the setting changes picture each day. */
export function isRotating(): boolean {
	const s = parseSetting(background.setting);
	return s.kind === 'path' && !s.picture;
}

/** Today's picture with the names of its collection, or null when there is none. */
export function current(): { address: string; title: string; collectionTitle: string } | null {
	const today = background.today;
	if (!today) return null;
	const [style, collection] = today.address.split('/');
	const c = background.allStyles.find((s) => s.style === style)?.collections.find((x) => x.id === collection);
	return { address: today.address, title: pictureTitle(today.picture, background.appearance), collectionTitle: c?.title ?? collection };
}

/**
 * The pictures the arrows step through: today's pool under a rotating
 * setting, the chosen picture's collection under one picture.
 */
function stepPool(): Address[] {
	if (isRotating()) return background.pool;
	const setting = parseSetting(background.setting);
	if (setting.kind !== 'path') return [];
	return pool(background.allStyles, { kind: 'path', style: setting.style, collection: setting.collection });
}

/**
 * The previous or next picture. Under a rotating setting it is today's
 * picture until midnight; under one picture it becomes the setting.
 */
export async function step(dir: 1 | -1): Promise<void> {
	await background.ensureStyles();
	const next = stepFrom(stepPool(), background.today?.address ?? '', dir);
	if (!next) return;
	if (isRotating()) await background.chooseForToday(formatAddress(next));
	else await background.set(formatAddress(next));
}

/**
 * The sources of the pictures either side of today's, in the current
 * appearance, so the control can load them before an arrow asks for them.
 */
export function neighbors(): { key: string; avif: string; src: string }[] {
	const at = background.today?.address;
	if (!at) return [];
	const items = stepPool();
	const out: { key: string; avif: string; src: string }[] = [];
	for (const dir of [1, -1] as const) {
		const a = stepFrom(items, at, dir);
		const picture = a && find(background.allStyles, a);
		if (!a || !picture || isDrawn(picture)) continue;
		const key = formatAddress(a);
		if (key === at || out.some((n) => n.key === key)) continue;
		const v = picture[background.appearance];
		const url = fileUrl(a.style);
		out.push({ key, avif: srcset(v, 'avif', url), src: widest(v, url) });
	}
	return out;
}

/** Keep today's picture: the setting becomes that one picture. */
export async function keep(): Promise<void> {
	const address = background.today?.address;
	if (address) await background.set(address);
}

/**
 * A still of a drawn picture, by its style's renderer. Each renderer in
 * lib/backgrounds/renderers.ts has its thumbnail here, so the drawing code
 * for thumbnails loads with the pickers and not at launch.
 */
function drawnThumbnail(style: string, id: string, params: unknown): string {
	const renderer = background.allStyles.find((s) => s.style === style)?.renderer;
	const dark = background.appearance === 'dark';
	switch (renderer) {
		case 'dots':
			return dotsPreview(`${style}/${id}`, params as DotsParams, dark);
		default:
			return '';
	}
}

/** A small picture for a thumbnail, in the current appearance. */
export function thumbnail(style: string, picture: Picture): string {
	if (isDrawn(picture)) return drawnThumbnail(style, picture.id, picture.params);
	return narrowest(picture[background.appearance], fileUrl(style));
}

/** How often a picker asks the server how a download is going. */
const DOWNLOAD_POLL_MS = 1000;

/**
 * Download a style the server does not have, and make it the setting once
 * it is installed, unless the setting changed while it downloaded. The
 * server does the download (api/backgrounds.rs); this starts it and follows
 * it, so a picker can show its progress from the store. A failure, including
 * one that stops it starting, is left in `background.failed` for the card.
 */
export async function download(style: string): Promise<void> {
	const settingAtStart = background.setting;
	try {
		await request(`/backgrounds/install/${encodeURIComponent(style)}`, { method: 'POST' });
	} catch (error) {
		background.noteFailure(style, startFailure(error));
		return;
	}
	try {
		await background.refreshDownloads();
		while (background.installing[style]) {
			await new Promise((resolve) => setTimeout(resolve, DOWNLOAD_POLL_MS));
			await background.refreshDownloads();
		}
	} catch (error) {
		console.warn('Following the background download failed:', error);
	}
	await background.refresh();
	const installed = background.styles?.some((s) => s.style === style);
	if (installed && background.setting === settingAtStart) await background.set(style);
}

/** What a card says when a download did not start. */
function startFailure(error: unknown): string {
	const message = error instanceof Error ? error.message : '';
	if (/free space|not published/i.test(message)) return message.replace(/^[^:]*:\s*/, '');
	return 'The download did not start. Try again.';
}

/** A size in megabytes, rounded, for "Download (95 MB)". */
export function megabytes(bytes: number): string {
	return `${Math.max(1, Math.round(bytes / 1_000_000))} MB`;
}
