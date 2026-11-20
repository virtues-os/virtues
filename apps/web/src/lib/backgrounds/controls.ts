/**
 * What the home control and the Settings picker do with the background
 * store: step to the picture before or after, keep today's picture, name
 * the current one, draw thumbnails, and download a style the server does
 * not have yet. These load with the pickers, never at launch
 * (agents/plan/backgrounds-plan.md, "The home control").
 */
import { request } from '$lib/api/client';
import { background, fileUrl } from '$lib/stores/background.svelte';
import { formatAddress, isDots, parseSetting, pictureTitle, pool, type Picture } from './resolve';
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
	if (setting.kind !== 'path') return;
	const siblings = pool(background.allStyles, { kind: 'path', style: setting.style, collection: setting.collection });
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
	if (isDots(picture)) return dotsPreview(`${style}/${picture.id}`, picture.dots, background.appearance === 'dark');
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
