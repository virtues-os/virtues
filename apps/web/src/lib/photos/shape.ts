/** How a photo favorite is named and placed on the wall. */

import { backendUrl } from '$lib/config/backend';
import { formatDuration } from '$lib/bookmarks/shape';
import type { PhotoApi } from './api';

/** The picture's address, made reachable from wherever the app runs. */
export function pictureSrc(p: Pick<PhotoApi, 'picture_url'>): string | null {
	return p.picture_url ? backendUrl(p.picture_url) : null;
}

/**
 * Height over width, for placing a photo before its picture loads. Kept
 * between a wide panorama and a tall screenshot, so neither takes over a
 * column; the wall crops to it.
 */
export function ratioOf(p: Pick<PhotoApi, 'width_px' | 'height_px'>): number {
	if (!p.width_px || !p.height_px) return 1;
	return Math.min(1.6, Math.max(0.56, p.height_px / p.width_px));
}

/** "0:42" on a video's frame. */
export function durationOf(p: Pick<PhotoApi, 'kind' | 'duration_seconds'>): string | null {
	return p.kind === 'video' && p.duration_seconds ? formatDuration(Math.round(p.duration_seconds)) : null;
}

/** "June 14, 2031", in the reader's time zone. */
export function takenOn(iso: string): string {
	const d = new Date(iso);
	return Number.isNaN(d.getTime()) ? '' : d.toLocaleDateString('en-US', { month: 'long', day: 'numeric', year: 'numeric' });
}

const NOUN: Record<PhotoApi['kind'], string> = {
	photo: 'Photo',
	live_photo: 'Photo',
	screenshot: 'Screenshot',
	video: 'Video',
};

/** What the tab and the page are called: "Photo, June 14, 2031". */
export function titleOf(p: Pick<PhotoApi, 'kind' | 'occurred_at'>): string {
	return [NOUN[p.kind] ?? 'Photo', takenOn(p.occurred_at)].filter(Boolean).join(', ');
}

/** The month a photo belongs under: "June 2031". */
export function monthOf(iso: string): string {
	return new Date(iso).toLocaleDateString('en-US', { month: 'long', year: 'numeric' });
}

/** Consecutive photos taken in the same month. The input is newest first. */
export function groupByMonth<T extends Pick<PhotoApi, 'occurred_at'>>(items: T[]): { label: string; items: T[] }[] {
	const groups: { label: string; items: T[] }[] = [];
	for (const item of items) {
		const label = monthOf(item.occurred_at);
		const last = groups[groups.length - 1];
		if (last?.label === label) last.items.push(item);
		else groups.push({ label, items: [item] });
	}
	return groups;
}
