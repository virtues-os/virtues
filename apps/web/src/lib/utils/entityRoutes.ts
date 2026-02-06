/**
 * Entity route utilities
 * Convert between entity IDs and their corresponding routes
 */

// Entity type prefixes (for IDs like person_abc123)
const ENTITY_PREFIXES: Record<string, string> = {
	person_: 'person',
	place_: 'place',
	org_: 'org',
	thing_: 'thing',
	day_: 'day',
	year_: 'year',
	file_: 'file',
	page_: 'page',
	chat_: 'chat',
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
	'/sources': 'source',
};

// Entity types to route bases
const TYPE_TO_ROUTE: Record<string, string> = {
	person: '/person',
	place: '/place',
	org: '/org',
	thing: '/thing',
	day: '/day',
	year: '/year',
	file: '/drive',
	page: '/page',
	chat: '/chat',
	source: '/sources',
};

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
