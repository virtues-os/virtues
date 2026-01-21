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

export { PAGE_TYPE_META, NARRATIVE_PAGE_TYPES, TEMPORAL_PAGE_TYPES, ENTITY_PAGE_TYPES } from "./base";

// Narrative pages (story structure)
export type { TelosPage } from "./telos";
export type { ActPage } from "./act";
export type { ChapterPage, NarrativeArc } from "./chapter";

// Temporal pages (calendar-based)
export type { YearPage, MonthSummary } from "./year";
export type {
	DayPage,
	DayEvent,
	ContextVector,
	LinkedEntities,
	LinkedTemporal,
	AutobiographySection,
} from "./day";

export {
	computeCompleteness,
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
export type { ThingPage, ThingType } from "./thing";

// =============================================================================
// UNION TYPE
// =============================================================================

import type { TelosPage } from "./telos";
import type { ActPage } from "./act";
import type { ChapterPage } from "./chapter";
import type { YearPage } from "./year";
import type { DayPage } from "./day";
import type { PersonPage } from "./person";
import type { PlacePage } from "./place";
import type { OrganizationPage } from "./organization";
import type { ThingPage } from "./thing";

/**
 * Discriminated union of all wiki page types.
 * Use type guards to narrow to specific page types.
 */
export type WikiPage =
	| TelosPage
	| ActPage
	| ChapterPage
	| YearPage
	| DayPage
	| PersonPage
	| PlacePage
	| OrganizationPage
	| ThingPage;

// =============================================================================
// TYPE GUARDS
// =============================================================================

export function isTelosPage(page: WikiPage): page is TelosPage {
	return page.type === "telos";
}

export function isActPage(page: WikiPage): page is ActPage {
	return page.type === "act";
}

export function isChapterPage(page: WikiPage): page is ChapterPage {
	return page.type === "chapter";
}

export function isYearPage(page: WikiPage): page is YearPage {
	return page.type === "year";
}

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

export function isThingPage(page: WikiPage): page is ThingPage {
	return page.type === "thing";
}

// =============================================================================
// PAGE CATEGORY TYPES
// =============================================================================

// Narrative pages - story structure (subjective meaning)
export type NarrativePage = TelosPage | ActPage | ChapterPage;

// Temporal pages - calendar-based (objective time)
export type TemporalPage = YearPage | DayPage;

// Entity pages - reference pages (people, places, things)
export type EntityPage = PersonPage | PlacePage | OrganizationPage | ThingPage;

export function isNarrativePage(page: WikiPage): page is NarrativePage {
	return ["telos", "act", "chapter"].includes(page.type);
}

export function isTemporalPage(page: WikiPage): page is TemporalPage {
	return ["year", "day"].includes(page.type);
}

export function isEntityPage(page: WikiPage): page is EntityPage {
	return ["person", "place", "organization", "thing"].includes(page.type);
}
