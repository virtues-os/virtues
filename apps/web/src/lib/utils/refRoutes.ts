/**
 * Entity route utilities
 * Convert between entity IDs and their corresponding routes
 */

// Entity type prefixes (for IDs like person_abc123)
const ENTITY_PREFIXES: Record<string, string> = {
	person_: 'person',
	place_: 'place',
	org_: 'org',
	day_: 'day',
	year_: 'year',
	file_: 'file',
	page_: 'page',
	chat_: 'chat',
	space_: 'space',
	source_: 'source',
};

// Route bases to entity types (for URLs like /person/slug)
const ROUTE_TO_TYPE: Record<string, string> = {
	'/person': 'person',
	'/place': 'place',
	'/org': 'org',
	'/thing': 'thing',
	'/day': 'day',
	'/year': 'year',
	'/drive': 'file',
	'/page': 'page',
	'/chat': 'chat',
	'/space': 'space',
	'/sources': 'source',
	'/source': 'source',
};

// Entity types to route bases
const TYPE_TO_ROUTE: Record<string, string> = {
	person: '/person',
	place: '/place',
	org: '/org',
	day: '/day',
	year: '/year',
	file: '/drive',
	page: '/page',
	chat: '/chat',
	space: '/space',
	source: '/sources',
};

// Entity type → iconify icon name (for typed pills). All registered in icons.ts.
const TYPE_TO_ICON: Record<string, string> = {
	person: 'ri:user-line',
	place: 'ri:map-pin-line',
	org: 'ri:building-line',
	thing: 'ri:lightbulb-line',
	page: 'ri:file-text-line',
	chat: 'ri:chat-3-line',
	space: 'ri:folder-line',
	file: 'ri:file-line',
	day: 'ri:calendar-line',
	year: 'ri:calendar-line',
	source: 'ri:at-line',
};

/** Icon name for an entity type (falls back to the generic @ icon). */
export function entityTypeIcon(type: string | null | undefined): string {
	return (type && TYPE_TO_ICON[type]) || 'ri:at-line';
}

// File sub-type icons, resolved from mime type first, then filename extension.
// Lets a file ref show its true nature (image / pdf / audio / video) instead of
// the generic file icon — even inline, where only the filename is known.
const MIME_ICON: Array<[RegExp, string]> = [
	[/^image\//, 'ri:image-line'],
	[/^audio\//, 'ri:music-2-line'],
	[/^video\//, 'ri:movie-line'],
	[/^application\/pdf$/, 'ri:file-pdf-line'],
];

const EXT_ICON: Record<string, string> = {
	jpg: 'ri:image-line', jpeg: 'ri:image-line', png: 'ri:image-line',
	gif: 'ri:image-line', webp: 'ri:image-line', svg: 'ri:image-line',
	bmp: 'ri:image-line', ico: 'ri:image-line', heic: 'ri:image-line',
	pdf: 'ri:file-pdf-line',
	mp3: 'ri:music-2-line', wav: 'ri:music-2-line', ogg: 'ri:music-2-line',
	flac: 'ri:music-2-line', aac: 'ri:music-2-line', m4a: 'ri:music-2-line',
	mp4: 'ri:movie-line', webm: 'ri:movie-line', mov: 'ri:movie-line',
	avi: 'ri:movie-line', mkv: 'ri:movie-line',
};

export type RefIconHint = { mimeType?: string | null; filename?: string | null };

/** Icon for a file ref, sharpened by mime type or filename extension. */
export function fileIcon(hint?: RefIconHint): string {
	const mime = hint?.mimeType;
	if (mime) {
		for (const [re, icon] of MIME_ICON) if (re.test(mime)) return icon;
	}
	const name = hint?.filename;
	if (name) {
		const dot = name.lastIndexOf('.');
		const ext = dot >= 0 ? name.slice(dot + 1).toLowerCase() : '';
		if (ext && EXT_ICON[ext]) return EXT_ICON[ext];
	}
	return 'ri:file-line';
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

// All valid entity prefixes (exported for backward compatibility)
export const ENTITY_PREFIXES_LIST = Object.keys(ENTITY_PREFIXES);

/**
 * Convert an entity ID to its route URL
 * @example getEntityRoute('person_abc123') → '/person/person_abc123'
 */
export function getEntityRoute(entityId: string): string {
	for (const [prefix, type] of Object.entries(ENTITY_PREFIXES)) {
		if (entityId.startsWith(prefix)) {
			const base = TYPE_TO_ROUTE[type];
			if (base) {
				return `${base}/${entityId}`;
			}
		}
	}
	return `/entity/${entityId}`; // fallback
}

/**
 * Parse a route URL to extract entity info
 * Returns the slug/id portion and entity type, or null if not an entity route
 *
 * Now supports both ID-based and slug-based routes:
 * @example parseEntityRoute('/person/person_abc123') → 'person_abc123'
 * @example parseEntityRoute('/person/adam-jace') → 'adam-jace'
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
 * Get entity type from a route URL
 * @example getEntityTypeFromRoute('/person/adam-jace') → 'person'
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

/**
 * Get entity type from entity ID (prefix-based)
 * @example getEntityType('person_abc123') → 'person'
 */
export function getEntityType(entityId: string): string | null {
	for (const [prefix, type] of Object.entries(ENTITY_PREFIXES)) {
		if (entityId.startsWith(prefix)) {
			return type;
		}
	}
	return null;
}
