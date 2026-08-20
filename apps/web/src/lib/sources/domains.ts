/**
 * Life-domains — the coarse buckets every ontology falls into.
 *
 * An ontology's name carries its domain as the first segment
 * (`health_heart_rate`, `location_visit`, `financial_transaction`), and the box
 * derives a source's `domains` the same way (`list_sources_handler` in
 * server/api.rs). This module is the display side of that: one label map and
 * one order, so the Catalog's "Provides" column and the Overview's arrival
 * grid name and sequence the same buckets identically.
 *
 * They were two copies of the label map, in two components, and only one of
 * them knew that `financial` reads as "Finance".
 */

/** Display name per domain. Anything unlisted falls back to Title Case. */
export const DOMAIN_LABEL: Record<string, string> = {
	health: 'Health',
	location: 'Location',
	communication: 'Communication',
	calendar: 'Calendar',
	activity: 'Activity',
	content: 'Content',
	financial: 'Finance',
	audio: 'Audio'
};

/**
 * Reading order: body first, then where you were, then who you spoke to and
 * what you had planned, then what you did and read, then money, then sound.
 * Fixed rather than sorted by volume — a section that moves when the data
 * moves is a section you have to find again every visit.
 */
export const DOMAIN_ORDER = [
	'health',
	'location',
	'communication',
	'calendar',
	'activity',
	'content',
	'financial',
	'audio'
] as const;

/** The domain an ontology name belongs to (its first segment). */
export function domainOf(ontologyName: string): string {
	return ontologyName.split('_')[0] ?? '';
}

export function domainLabel(domain: string): string {
	return DOMAIN_LABEL[domain] ?? domain.replace(/\b\w/g, (c) => c.toUpperCase());
}

/** Sort key: canonical domains in `DOMAIN_ORDER`, then anything else. */
export function domainRank(domain: string): number {
	const i = (DOMAIN_ORDER as readonly string[]).indexOf(domain);
	return i === -1 ? DOMAIN_ORDER.length : i;
}
