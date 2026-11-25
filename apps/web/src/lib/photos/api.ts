/**
 * Photo favorites API (`data_content_photo`): the photos the owner favorited
 * on their iPhone, each with its picture and what is in it.
 */

import { request, ApiError } from '$lib/api/client';
import type { ExtractionRecord } from '$lib/bookmarks/api';

export interface PhotoApi {
	id: string;
	/** A `video`'s picture is one frame of it. */
	kind: 'photo' | 'live_photo' | 'screenshot' | 'video';
	/** When it was taken. */
	occurred_at: string;
	width_px: number | null;
	height_px: number | null;
	duration_seconds: number | null;
	/** The owner's own words. Never machine-written. */
	note: string | null;
	/** What the picture shows, or null until it is described. */
	description: string | null;
	/** The picture's address on your server. */
	picture_url: string | null;
}

/** What the picture pass adds to a bookmark's record for a photo. */
export interface PhotoRecord extends ExtractionRecord {
	colors?: string[];
	setting?: string | null;
	visible_text?: string | null;
}

export interface PhotoDetailApi extends PhotoApi {
	/** Set once the photo is no longer a favorite on the phone. */
	deleted_at_source: string | null;
	extraction: PhotoRecord | null;
	enrichment_model: string | null;
	enriched_at: string | null;
	state: 'described' | 'waiting' | 'not_described';
	/** The place you named where it was taken. */
	place: { id: string; name: string } | null;
	/** Names written in the picture, each with its Wiki page when one matches. */
	mentions: { name: string; route: string | null }[];
}

export interface PhotoPageApi {
	items: PhotoApi[];
	has_more: boolean;
}

/** A failed call as the sentence a person reads; the detail goes to the console. */
function said(e: unknown, words: string, missing?: string): Error {
	console.warn('photos:', e);
	return new Error(missing && e instanceof ApiError && e.status === 404 ? missing : words);
}

export async function getPhotos(opts: { offset: number; limit: number; search?: string }): Promise<PhotoPageApi> {
	try {
		return await request<PhotoPageApi>('/photos', { query: opts });
	} catch (e) {
		throw said(e, "Your server couldn't list your photos. Try again.");
	}
}

export async function getPhoto(id: string): Promise<PhotoDetailApi> {
	try {
		return await request<PhotoDetailApi>(`/photos/${encodeURIComponent(id)}`);
	} catch (e) {
		throw said(
			e,
			"Your server couldn't open that photo. Try again.",
			"That photo isn't on your server. Go back to Photo favorites to see what is."
		);
	}
}

/** Write the note. Blank clears it. */
export async function updatePhotoNote(id: string, note: string | null): Promise<PhotoDetailApi> {
	try {
		return await request<PhotoDetailApi>(`/photos/${encodeURIComponent(id)}/note`, {
			method: 'PATCH',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ note }),
		});
	} catch (e) {
		throw said(e, "Your server couldn't save that note. Try again.");
	}
}
