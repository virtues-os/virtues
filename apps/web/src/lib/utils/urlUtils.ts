/**
 * URL Utilities
 *
 * Consolidated URL parsing and manipulation functions.
 * Single source of truth for URL operations across the app.
 *
 * URL Structure:
 * - Entity namespaces: /{namespace}/{namespace}_{id} (e.g., /person/person_abc)
 * - List pages: /{namespace} (e.g., /person)
 * - Storage: /drive/{path} (e.g., /drive/docs/file.pdf)
 * - System: /virtues/{page} (e.g., /virtues/account)
 */

// All entity namespaces that follow the /{namespace}/{namespace}_{id} pattern
const ENTITY_NAMESPACES = [
	'chat',
	'page',
	'person',
	'place',
	'org',
	'day',
	'year',
	'source'
] as const;

type EntityNamespace = (typeof ENTITY_NAMESPACES)[number];

/**
 * Extract entity ID from a URL.
 * e.g., '/person/person_abc' → 'person_abc'
 * e.g., '/day/day_2026-01-25' → 'day_2026-01-25'
 */
export function routeToEntityId(route: string): string | null {
	const match = route.match(/^\/[a-z]+\/([^/]+)$/);
	return match?.[1] ?? null;
}

/**
 * Build a URL from an entity ID.
 * e.g., 'person_abc' → '/person/person_abc'
 * e.g., 'day_2026-01-25' → '/day/day_2026-01-25'
 */
export function entityIdToRoute(entityId: string): string {
	const parsed = parseEntityId(entityId);
	if (!parsed) return '/';
	return `/${parsed.namespace}/${entityId}`;
}

/**
 * Extract namespace and ID from an entity ID.
 * e.g., 'person_abc123' → { namespace: 'person', id: 'abc123' }
 */
export function parseEntityId(entityId: string): { namespace: string; id: string } | null {
	const match = entityId.match(/^([a-z]+)_(.+)$/);
	if (!match) return null;
	return { namespace: match[1], id: match[2] };
}

/**
 * Get namespace from a URL.
 * e.g., '/person/person_abc' → 'person'
 * e.g., '/virtues/account' → 'virtues'
 * e.g., '/drive/docs/file.pdf' → 'drive'
 */
export function getNamespace(url: string): string | null {
	if (url === '/') return null;

	// Handle special namespaces with subpaths
	if (url.startsWith('/virtues/')) return 'virtues';
	if (url.startsWith('/drive/')) return 'drive';
	if (url.startsWith('/lake/')) return 'lake';

	// Extract first segment
	const match = url.match(/^\/([a-z]+)(?:\/|$)/);
	return match?.[1] ?? null;
}

/**
 * Get entity type from a URL for context menus.
 * Returns the namespace if it's an entity namespace, null otherwise.
 * e.g., '/page/page_xyz' → 'page'
 * e.g., '/virtues/account' → null (not an entity)
 */
export function getTypeFromUrl(url: string): EntityNamespace | null {
	const namespace = getNamespace(url);
	if (!namespace) return null;

	// Check if it's an entity namespace
	if (ENTITY_NAMESPACES.includes(namespace as EntityNamespace)) {
		return namespace as EntityNamespace;
	}

	return null;
}

/**
 * Check if a URL is an entity detail page.
 * e.g., '/person/person_abc' → true
 * e.g., '/person' → false (list page)
 * e.g., '/virtues/account' → false (system page)
 */
export function isEntityDetailUrl(url: string): boolean {
	return /^\/[a-z]+\/[a-z]+_[^/]+$/.test(url);
}

/**
 * Check if a URL is an entity list page.
 * e.g., '/person' → true
 * e.g., '/person/person_abc' → false
 */
export function isEntityListUrl(url: string): boolean {
	const namespace = getNamespace(url);
	if (!namespace) return false;

	// Must be entity namespace and no subpath
	if (!ENTITY_NAMESPACES.includes(namespace as EntityNamespace)) return false;

	// Check it's just the namespace, no ID
	return url === `/${namespace}`;
}

/**
 * Sanitize a URL by removing duplicate slashes.
 * Preserves protocol double slashes (http://, https://).
 * e.g., '/page//page/page_xyz' → '/page/page_xyz'
 */
export function sanitizeUrl(url: string): string {
	// Preserve protocol double slashes
	if (url.startsWith('http://') || url.startsWith('https://')) {
		return url;
	}
	// Replace multiple consecutive slashes with single slash
	return url.replace(/\/+/g, '/');
}

/**
 * Check if a URL is external (starts with http:// or https://).
 */
export function isExternalUrl(url: string): boolean {
	return url.startsWith('http://') || url.startsWith('https://');
}

/**
 * Build a full URL from parts.
 * e.g., ('person', 'person_abc') → '/person/person_abc'
 */
export function buildEntityUrl(namespace: string, entityId: string): string {
	return `/${namespace}/${entityId}`;
}

/**
 * Get the display name for a namespace.
 * e.g., 'person' → 'Person'
 * e.g., 'org' → 'Organization'
 */
export function getNamespaceDisplayName(namespace: string): string {
	const displayNames: Record<string, string> = {
		chat: 'Chat',
		page: 'Page',
		person: 'Person',
		place: 'Place',
		org: 'Organization',
		day: 'Day',
		year: 'Year',
		source: 'Source',
		drive: 'Drive',
		virtues: 'System'
	};
	return displayNames[namespace] ?? namespace;
}
