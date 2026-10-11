/**
 * Entity route utilities
 * Convert between entity IDs and their corresponding routes
 */

import { kindOfLink, type MediaKind } from '$lib/document/media-kind';
import { PROJECT_ICON } from '$lib/utils/iconHelpers';
import { decoded } from '$lib/utils/urlUtils';

// Route bases to entity types (for URLs like /person/slug)
const ROUTE_TO_TYPE: Record<string, string> = {
	'/person': 'person',
	'/place': 'place',
	'/org': 'org',
	'/day': 'day',
	'/year': 'year',
	'/drive': 'file',
	'/page': 'page',
	'/chat': 'chat',
	'/project': 'project',
	'/notebook': 'project', // legacy spelling — projects were notebooks until 2026-09; old refs still carry it
	'/sources': 'source',
	'/source': 'source',
	'/record': 'record',
};

// Entity type → iconify icon name (for typed pills). All registered in icons.ts.
const TYPE_TO_ICON: Record<string, string> = {
	person: 'ri:user-line',
	place: 'ri:map-pin-line',
	org: 'ri:building-line',
	page: 'ri:file-text-line',
	chat: 'ri:chat-3-line',
	project: PROJECT_ICON,
	file: 'ri:file-line',
	day: 'ri:calendar-line',
	year: 'ri:calendar-line',
	source: 'ri:at-line',
	record: 'ri:database-2-line',
};

/** Icon name for an entity type (falls back to the generic @ icon). */
export function entityTypeIcon(type: string | null | undefined): string {
	return (type && TYPE_TO_ICON[type]) || 'ri:at-line';
}

// File sub-type icons, resolved from mime type first, then the filename, read
// as a page reads a link's (`kindOfLink`). Lets a file ref show its true nature
// (image / pdf / audio / video) instead of the generic file icon — even inline,
// where only the filename is known.
const MIME_ICON: Array<[RegExp, string]> = [
	[/^image\//, 'ri:image-line'],
	[/^audio\//, 'ri:music-2-line'],
	[/^video\//, 'ri:movie-line'],
	[/^application\/pdf$/, 'ri:file-pdf-line'],
];

const KIND_ICON: Record<MediaKind, string> = {
	image: 'ri:image-line',
	audio: 'ri:music-2-line',
	video: 'ri:movie-line',
	file: 'ri:file-line',
};

export type RefIconHint = { mimeType?: string | null; filename?: string | null };

/** Icon for a file ref, sharpened by mime type or filename extension. */
export function fileIcon(hint?: RefIconHint): string {
	const mime = hint?.mimeType;
	if (mime) {
		for (const [re, icon] of MIME_ICON) if (re.test(mime)) return icon;
	}
	const name = hint?.filename;
	if (!name) return KIND_ICON.file;
	const kind = kindOfLink('', name);
	if (kind === 'file' && name.toLowerCase().endsWith('.pdf')) return 'ri:file-pdf-line';
	return KIND_ICON[kind];
}

/**
 * Icon name for a reference, given its target type and an optional hint.
 * File refs are sharpened by mime type / filename (Server.jpg → image icon);
 * all other types map straight from their entity type.
 * Single source of truth for the leading icon across every ref renderer.
 */
export function refIcon(type: string | null | undefined, hint?: RefIconHint): string {
	if (type === 'file') return fileIcon(hint);
	return entityTypeIcon(type);
}

/**
 * Parse a route URL to extract entity info
 * Returns the slug/id portion and entity type, or null if not an entity route
 *
 * Now supports both ID-based and slug-based routes:
 * @example parseEntityRoute('/person/person_abc123') → 'person_abc123'
 * @example parseEntityRoute('/person/david-okafor') → 'david-okafor'
 */
export function parseEntityRoute(route: string): string | null {
	for (const base of Object.keys(ROUTE_TO_TYPE)) {
		if (route.startsWith(`${base}/`)) {
			const idOrSlug = route.slice(base.length + 1);
			if (idOrSlug) {
				return idOrSlug;
			}
		}
	}
	return null;
}

/**
 * `/kind/id` → its kind and id: the client's twin of the server's
 * `refs::split_ref`, and the grammar every consumer should parse with. The
 * legacy `/notebook/` reads as `project`; `?` and `#` never belong to an id.
 * Null for an external URL or anything that is not a record route. Never
 * throws: an id whose %-escapes are malformed (`/person/%`) is read as
 * written, since a page holds any path and is drawn link by link.
 */
export function parseRef(url: string): { kind: string; id: string } | null {
	const m = /^\/([^/?#]+)\/([^?#]+)/.exec(url);
	if (!m) return null;
	const kind = ROUTE_TO_TYPE[`/${m[1]}`];
	if (!kind) return null;
	return { kind, id: decoded(m[2]) };
}

/**
 * Get entity type from a route URL
 * @example getEntityTypeFromRoute('/person/david-okafor') → 'person'
 */
export function getEntityTypeFromRoute(route: string): string | null {
	for (const [base, type] of Object.entries(ROUTE_TO_TYPE)) {
		if (route.startsWith(base + '/')) {
			return type;
		}
	}
	return null;
}

/**
 * Check if a URL is an entity route
 */
export function isEntityRoute(url: string): boolean {
	return getEntityTypeFromRoute(url) !== null;
}
