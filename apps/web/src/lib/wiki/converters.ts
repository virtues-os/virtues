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
	TemporalEventApi,
} from "./api";

import type { PersonPage } from "./types/person";
import type { PlacePage, PlaceType } from "./types/place";
import type { OrganizationPage, OrganizationType } from "./types/organization";
import type { DayPage, DayEvent, LinkedEntities, LinkedTemporal } from "./types/day";
import { parseDateSlug, formatLongDate } from "$lib/utils/dateUtils";

// ============================================================================
// Helper Functions
// ============================================================================

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
				title: api.name,
		cover: api.picture ?? undefined,

		// Person-specific fields
		nickname: api.nickname ?? undefined,
		aliases: api.aliases ?? [],
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
		article: api.article ?? undefined,
		articleUpdatedAt: api.article_updated_at ? new Date(api.article_updated_at) : undefined,
		articleAutoUpdate: api.article_auto_update ?? false,

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
				title: api.name,
		cover: api.cover_image ?? undefined,

		// Place-specific fields
		placeType: api.category ? (placeTypeMap[api.category.toLowerCase()] ?? "other") : "other",
		address: api.address ?? undefined,
		coordinates:
			api.latitude && api.longitude ? { lat: api.latitude, lng: api.longitude } : undefined,
		visitCount: api.seen_count ?? 0,
		firstVisit: api.first_seen ? new Date(api.first_seen) : undefined,
		lastVisit: api.last_seen ? new Date(api.last_seen) : undefined,

		// Content
		content: api.content ?? "",
		article: api.article ?? undefined,
		articleUpdatedAt: api.article_updated_at ? new Date(api.article_updated_at) : undefined,
		articleAutoUpdate: api.article_auto_update ?? false,

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
				title: api.name,
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
		aliases: api.aliases ?? [],

		// Content
		content: api.content ?? "",
		article: api.article ?? undefined,
		articleUpdatedAt: api.article_updated_at ? new Date(api.article_updated_at) : undefined,
		articleAutoUpdate: api.article_auto_update ?? false,

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
	// Parse as local date — new Date("2026-02-10") would be UTC midnight (wrong timezone)
	const date = parseDateSlug(api.date);
	const dayNames = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

	return {
		type: "day",
		id: api.id,
				title: formatLongDate(date),
		cover: api.cover_image ?? undefined,

		// Day-specific fields
		date,
		dayOfWeek: dayNames[date.getDay()],
		startTimezone: api.start_timezone,

		// Layers (will be populated from separate queries)
		linkedEntities: emptyLinkedEntities(),
		linkedTemporal: emptyLinkedTemporal(),
		events: [],
		autobiography: api.article ?? "",
		epigraph: api.epigraph ?? undefined,
		dataQuality: api.data_quality ?? undefined,
		newEntityCount: api.new_entity_count ?? 0,
		newTopicCount: api.new_topic_count ?? 0,
		readinessScore: api.readiness_score ?? null,
		readinessDetails: api.readiness_details ?? null,
		sleepCycles: (api.sleep_cycles ?? []).map((c: { start_time: string; end_time: string; dominant_stage: string; avg_hr: number | null; autonomic_z: number | null }) => ({
			startTime: new Date(c.start_time),
			endTime: new Date(c.end_time),
			dominantStage: c.dominant_stage,
			avgHr: c.avg_hr,
			autonomicZ: c.autonomic_z,
		})),

		// Metadata
		citations: [],
		linkedPages: [],
		tags: [],
		// The day's prose: the article page (via wiki_day_prose) with the
		content: api.article ?? "",
		createdAt: new Date(api.created_at),
		updatedAt: new Date(api.updated_at),
		lastEditedBy: (api.last_edited_by as "ai" | "human") ?? "ai",
	};
}

// ============================================================================
// Day Event Converter
// ============================================================================

export function apiToDayEvent(api: TemporalEventApi): DayEvent {
	const start = new Date(api.start_time);
	const end = new Date(api.end_time);
	return {
		id: api.id,
		startTime: start,
		endTime: end,
		durationMinutes: Math.round((end.getTime() - start.getTime()) / 60000),
		autoLabel: api.auto_label ?? "Unknown",
		autoLocation: api.auto_location ?? undefined,
		sourceIds: Array.isArray(api.source_ontologies) ? api.source_ontologies : [],
		userLabel: api.user_label || undefined,
		userLocation: api.user_location || undefined,
		userNotes: api.user_notes || undefined,
		noveltyZ: api.novelty_z ?? null,
		autonomicZ: api.autonomic_z ?? null,
		avgHr: api.avg_hr ?? null,
		hrZ: api.hr_z ?? null,
		hrvZ: api.hrv_z ?? null,
		topics: api.topics ?? [],
		eventSummary: api.event_summary ?? null,
		agentAction: (api.agent_action as DayEvent["agentAction"]) ?? null,
		isSleep: api.is_sleep ?? false,
		userHidden: api.user_hidden ?? false,
		entities: Array.isArray(api.entities) ? api.entities : [],
		topicNovelty: api.topic_novelty ?? null,
		entityNovelty: api.entity_novelty ?? null,
		entityTimestamps: api.entity_timestamps ?? null,
		isUserAdded: api.is_user_added ?? false,
		isUserEdited: api.is_user_edited ?? false,
		isTransit: api.is_transit ?? false,
		isUnknown: api.is_unknown ?? false,
	};
}

// ============================================================================
// Act Converter
// ============================================================================


// ============================================================================
// Chapter Converter
// ============================================================================


// ============================================================================
// Telos Converter
// ============================================================================

