/**
 * Wiki Types - Base
 *
 * Shared types used across all wiki page types.
 */

// =============================================================================
// PAGE TYPE ENUM
// =============================================================================

export type WikiPageType =
	// Narrative (story structure - subjective meaning)
	| "telos"
	| "act"
	| "chapter"
	// Temporal (calendar-based - objective time)
	| "year"
	| "day"
	// Entity (reference pages - people, places, things)
	| "person"
	| "place"
	| "organization"
	| "thing";

export const PAGE_TYPE_META: Record<WikiPageType, { label: string; icon: string }> = {
	telos: { label: "Telos", icon: "ri:compass-3-line" },
	act: { label: "Act", icon: "ri:book-open-line" },
	chapter: { label: "Chapter", icon: "ri:bookmark-line" },
	year: { label: "Year", icon: "ri:calendar-2-line" },
	day: { label: "Day", icon: "ri:calendar-line" },
	person: { label: "Person", icon: "ri:user-line" },
	place: { label: "Place", icon: "ri:map-pin-line" },
	organization: { label: "Organization", icon: "ri:building-line" },
	thing: { label: "Thing", icon: "ri:box-3-line" },
};

// Narrative page types (story structure)
export const NARRATIVE_PAGE_TYPES: WikiPageType[] = ["telos", "act", "chapter"];

// Temporal page types (calendar-based)
export const TEMPORAL_PAGE_TYPES: WikiPageType[] = ["year", "day"];

// Entity page types (reference pages)
export const ENTITY_PAGE_TYPES: WikiPageType[] = ["person", "place", "organization", "thing"];

// =============================================================================
// AUTHORSHIP
// =============================================================================

export type AuthorType = "human" | "ai";

/**
 * Section-level authorship tracking.
 * - "ai": Pure AI-generated, never touched by human
 * - "human": Written entirely by human
 * - "ai+human": AI-generated but edited by human
 */
export type SectionAuthorType = "ai" | "human" | "ai+human";

// =============================================================================
// CITATIONS
// =============================================================================

/**
 * A citation linking a claim to evidence.
 * Inline syntax: [1], [2], etc.
 */
export interface Citation {
	id: string;
	index: number; // Display number [1], [2], etc.
	sourceType: "ontology" | "aggregated" | "external";
	label: string;
	preview?: string;
	timestamp?: Date;
	addedBy: AuthorType;
}

// =============================================================================
// LINKED PAGES
// =============================================================================

/**
 * A linked page reference.
 * Inline syntax: [[Page Name]]
 * Resolution: look up displayName in linkedPages[] to get slug.
 */
export interface LinkedPage {
	displayName: string; // Matches [[...]] in content
	pageSlug: string; // For navigation: /wiki/{pageSlug}
	pageType?: WikiPageType;
	preview?: string; // Short description or subtitle
}

/**
 * A related page (AI-suggested or manually curated).
 * Displayed in the "Related Pages" section.
 */
export interface RelatedPage {
	slug: string;
	title: string;
	pageType?: WikiPageType;
	preview?: string;
}

// =============================================================================
// DATE RANGE
// =============================================================================

/**
 * A period of time with optional end (ongoing if no end).
 */
export interface DateRange {
	start: Date;
	end?: Date; // Undefined means ongoing
}

// =============================================================================
// INFOBOX
// =============================================================================

/**
 * Infobox field for the right sidebar.
 */
export interface InfoboxField {
	label: string;
	value: string;
	href?: string; // If it's a link
}

/**
 * Infobox data (Wikipedia-style sidebar).
 */
export interface Infobox {
	title?: string; // Optional override
	image?: string; // Cover/profile image URL
	fields: InfoboxField[];
	links?: { label: string; pageSlug: string }[];
}

// =============================================================================
// BASE PAGE INTERFACE
// =============================================================================

/**
 * Base interface shared by all wiki page types.
 * Each specific page type extends this with type-specific fields.
 */
export interface WikiPageBase {
	id: string;
	slug: string;
	title: string;
	subtitle?: string;
	cover?: string; // Cover image URL

	// Citations (data provenance)
	citations: Citation[];

	// Linked pages (resolution for [[wiki links]])
	linkedPages: LinkedPage[];

	// Related pages (AI-suggested or manually curated)
	relatedPages?: RelatedPage[];

	// Infobox (Wikipedia-style sidebar)
	infobox?: Infobox;

	// Tags
	tags: string[];

	// Content (markdown body)
	content: string;

	// Timestamps
	createdAt: Date;
	updatedAt: Date;
	lastEditedBy: AuthorType;
}
