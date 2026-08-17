/**
 * Wiki Types - Index
 *
 * Exports all wiki page types as a discriminated union.
 */

// =============================================================================
// RE-EXPORTS
// =============================================================================

// Base types
export type {
	WikiPageType,
	AuthorType,
	SectionAuthorType,
	Citation,
	LinkedPage,
	RelatedPage,
	DateRange,
	InfoboxField,
	Infobox,
	WikiPageBase,
} from "./base";

export { PAGE_TYPE_META, TEMPORAL_PAGE_TYPES, ENTITY_PAGE_TYPES } from "./base";

// Temporal pages (calendar-based)
export type {
	DayPage,
	DayEvent,
	ScoredSleepCycle,
	LinkedEntities,
	LinkedTemporal,
	AutobiographySection,
} from "./day";

export {
	getEventDisplayLabel,
	getEventDisplayLocation,
	emptyLinkedEntities,
	flattenLinkedEntities,
	emptyLinkedTemporal,
} from "./day";

// Entity pages
export type { PersonPage, ConnectionTier, ContactFrequency, SocialLinks } from "./person";
export type { PlacePage, PlaceType, Coordinates } from "./place";
export type { OrganizationPage, OrganizationType } from "./organization";

// =============================================================================
// UNION TYPE
// =============================================================================

import type { DayPage } from "./day";
import type { PersonPage } from "./person";
import type { PlacePage } from "./place";
import type { OrganizationPage } from "./organization";

/**
 * Discriminated union of all wiki page types.
 * Use type guards to narrow to specific page types.
 */
export type WikiPage =
	| DayPage
	| PersonPage
	| PlacePage
	| OrganizationPage;

// =============================================================================
// TYPE GUARDS
// =============================================================================





export function isDayPage(page: WikiPage): page is DayPage {
	return page.type === "day";
}

export function isPersonPage(page: WikiPage): page is PersonPage {
	return page.type === "person";
}

export function isPlacePage(page: WikiPage): page is PlacePage {
	return page.type === "place";
}

export function isOrganizationPage(page: WikiPage): page is OrganizationPage {
	return page.type === "organization";
}

// =============================================================================
// PAGE CATEGORY TYPES
// =============================================================================

// Temporal pages - calendar-based (objective time)
//
// `NarrativePage` (telos | act | chapter) went with migration 0107: four tables
// with a read path, a render branch and no writer, in a schema that claimed the
// product had a life-story hierarchy it had never built.
export type TemporalPage = DayPage;

// Entity pages - reference pages (people, places, orgs)
export type EntityPage = PersonPage | PlacePage | OrganizationPage;

export function isTemporalPage(page: WikiPage): page is TemporalPage {
	return page.type === "day";
}

export function isEntityPage(page: WikiPage): page is EntityPage {
	return ["person", "place", "organization"].includes(page.type);
}
