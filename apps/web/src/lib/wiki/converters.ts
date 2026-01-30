/**
 * Wiki Type Converters
 *
 * Convert API response types to frontend page types.
 * These converters bridge the gap between database-backed entities
 * and the rich frontend types used for rendering.
 */

import type {
	WikiPersonApi,
	WikiPlaceApi,
	WikiOrganizationApi,
	WikiDayApi,
	WikiActApi,
	WikiChapterApi,
	WikiTelosApi,
} from "./api";

import type { PersonPage } from "./types/person";
import type { PlacePage, PlaceType } from "./types/place";
import type { OrganizationPage, OrganizationType } from "./types/organization";
import type { DayPage, ContextVector, LinkedEntities, LinkedTemporal } from "./types/day";
import type { ActPage } from "./types/act";
import type { ChapterPage } from "./types/chapter";
import type { TelosPage } from "./types/telos";

// ============================================================================
// Helper Functions
// ============================================================================

function emptyContextVector(): ContextVector {
	return { when: 0, where: 0, who: 0, what: 0, why: 0, how: 0 };
}

function emptyLinkedEntities(): LinkedEntities {
	return { people: [], places: [], organizations: [] };
}

function emptyLinkedTemporal(): LinkedTemporal {
	return { events: [], related: [] };
}

// ============================================================================
// Person Converter
// ============================================================================

export function apiToPersonPage(api: WikiPersonApi): PersonPage {
	return {
		type: "person",
		id: api.id,
		slug: api.slug ?? api.id,
		title: api.canonical_name,
		cover: api.picture ?? undefined,

		// Person-specific fields
		nickname: api.nickname ?? undefined,
		relationship: api.relationship_category ?? "Contact",
		emails: api.emails,
		phones: api.phones,
		socials: {
			linkedin: api.linkedin ?? undefined,
			twitter: api.x ?? undefined,
			instagram: api.instagram ?? undefined,
			facebook: api.facebook ?? undefined,
		},
		birthday: api.birthday ? new Date(api.birthday) : undefined,

		// Content
		content: api.content ?? "",

		// Metadata (empty for now - will be computed from entity_edges)
		citations: [],
		linkedPages: [],
		tags: [],
		createdAt: new Date(api.created_at),
		updatedAt: new Date(api.updated_at),
		lastEditedBy: "ai",
	};
}

// ============================================================================
// Place Converter
// ============================================================================

export function apiToPlacePage(api: WikiPlaceApi): PlacePage {
	// Map category to placeType
	const placeTypeMap: Record<string, PlaceType> = {
		home: "home",
		work: "work",
		gym: "third-place",
		cafe: "third-place",
		library: "third-place",
		airport: "transit",
		station: "transit",
		travel: "travel",
	};

	return {
		type: "place",
		id: api.id,
		slug: api.slug ?? api.id,
		title: api.name,
		cover: api.cover_image ?? undefined,

		// Place-specific fields
		placeType: api.category ? (placeTypeMap[api.category.toLowerCase()] ?? "other") : "other",
		address: api.address ?? undefined,
		coordinates:
			api.latitude && api.longitude ? { lat: api.latitude, lng: api.longitude } : undefined,
		visitCount: api.visit_count ?? 0,
		firstVisit: api.first_visit ? new Date(api.first_visit) : undefined,
		lastVisit: api.last_visit ? new Date(api.last_visit) : undefined,

		// Content
		content: api.content ?? "",

		// Connections (populated from entity_edges later)
		associatedPeople: [],
		activities: [],
		narrativeContext: [],

		// Metadata
		citations: [],
		linkedPages: [],
		tags: [],
		createdAt: new Date(api.created_at),
		updatedAt: new Date(api.updated_at),
		lastEditedBy: "ai",
	};
}

// ============================================================================
// Organization Converter
// ============================================================================

export function apiToOrganizationPage(api: WikiOrganizationApi): OrganizationPage {
	// Map organization_type to orgType
	const orgTypeMap: Record<string, OrganizationType> = {
		employer: "employer",
		company: "employer",
		school: "school",
		university: "school",
		community: "community",
		church: "community",
		club: "community",
		institution: "institution",
		government: "institution",
		hospital: "institution",
	};

	return {
		type: "organization",
		id: api.id,
		slug: api.slug ?? api.id,
		title: api.canonical_name,
		cover: api.cover_image ?? undefined,

		// Org-specific fields
		orgType: api.organization_type
			? (orgTypeMap[api.organization_type.toLowerCase()] ?? "other")
			: "other",
		period: api.start_date
			? {
					start: new Date(api.start_date),
					end: api.end_date ? new Date(api.end_date) : undefined,
				}
			: undefined,
		role: api.role_title ?? undefined,

		// Content
		content: api.content ?? "",

		// Connections (populated from entity_edges later)
		keyContacts: [],
		locations: [],
		narrativeContext: [],

		// Metadata
		citations: [],
		linkedPages: [],
		tags: [],
		createdAt: new Date(api.created_at),
		updatedAt: new Date(api.updated_at),
		lastEditedBy: "ai",
	};
}

// ============================================================================
// Day Converter
// ============================================================================

export function apiToDayPage(api: WikiDayApi): DayPage {
	const date = new Date(api.date);
	const dayNames = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

	return {
		type: "day",
		id: api.id,
		slug: api.date,
		title: formatDayTitle(date),
		cover: api.cover_image ?? undefined,

		// Day-specific fields
		date,
		dayOfWeek: dayNames[date.getDay()],
		startTimezone: api.start_timezone,
		endTimezone: api.end_timezone,

		// Layers (will be populated from separate queries)
		contextVector: emptyContextVector(),
		linkedEntities: emptyLinkedEntities(),
		linkedTemporal: emptyLinkedTemporal(),
		events: [],
		autobiography: api.autobiography ?? "",
		autobiographySections: api.autobiography_sections
			? (api.autobiography_sections as DayPage["autobiographySections"])
			: undefined,

		// Metadata
		citations: [],
		linkedPages: [],
		tags: [],
		content: api.autobiography ?? "", // DayPage uses autobiography as main content
		createdAt: new Date(api.created_at),
		updatedAt: new Date(api.updated_at),
		lastEditedBy: (api.last_edited_by as "ai" | "human") ?? "ai",
	};
}

function formatDayTitle(date: Date): string {
	return date.toLocaleDateString("en-US", {
		weekday: "long",
		year: "numeric",
		month: "long",
		day: "numeric",
	});
}

// ============================================================================
// Act Converter
// ============================================================================

export function apiToActPage(api: WikiActApi): ActPage {
	return {
		type: "act",
		id: api.id,
		slug: api.slug ?? api.id,
		title: api.title,
		subtitle: api.subtitle ?? undefined,
		cover: api.cover_image ?? undefined,

		// Act-specific fields
		period: {
			start: new Date(api.start_date),
			end: api.end_date ? new Date(api.end_date) : undefined,
		},
		location: api.location ?? undefined,
		themes: api.themes ?? [],

		// Content
		content: api.content ?? api.description ?? "",

		// Connections (populated from queries later)
		telos: undefined,
		chapters: [],
		keyPeople: [],
		keyPlaces: [],

		// Metadata
		citations: [],
		linkedPages: [],
		tags: [],
		createdAt: new Date(api.created_at),
		updatedAt: new Date(api.updated_at),
		lastEditedBy: "ai",
	};
}

// ============================================================================
// Chapter Converter
// ============================================================================

export function apiToChapterPage(api: WikiChapterApi): ChapterPage {
	return {
		type: "chapter",
		id: api.id,
		slug: api.slug ?? api.id,
		title: api.title,
		subtitle: api.subtitle ?? undefined,
		cover: api.cover_image ?? undefined,

		// Chapter-specific fields
		period: {
			start: new Date(api.start_date),
			end: api.end_date ? new Date(api.end_date) : undefined,
		},
		arc: "stable", // Default arc, could be stored in metadata

		// Content
		content: api.content ?? api.description ?? "",

		// Connections (populated from queries later)
		act: undefined,
		keyPeople: [],
		keyPlaces: [],
		notableDays: [],
		lessons: [],

		// Metadata
		citations: [],
		linkedPages: [],
		tags: [],
		createdAt: new Date(api.created_at),
		updatedAt: new Date(api.updated_at),
		lastEditedBy: "ai",
	};
}

// ============================================================================
// Telos Converter
// ============================================================================

export function apiToTelosPage(api: WikiTelosApi): TelosPage {
	return {
		type: "telos",
		id: api.id,
		slug: api.slug ?? api.id,
		title: api.title,
		cover: api.cover_image ?? undefined,

		// Telos-specific fields
		coreValues: [], // Could be parsed from content or stored in metadata
		visionStatement: api.description ?? "",

		// Content
		content: api.content ?? api.description ?? "",

		// Connections (populated from queries later)
		acts: [],
		guidingIdeas: [],
		influentialPeople: [],

		// Metadata
		citations: [],
		linkedPages: [],
		tags: [],
		createdAt: new Date(api.created_at),
		updatedAt: new Date(api.updated_at),
		lastEditedBy: "ai",
	};
}
