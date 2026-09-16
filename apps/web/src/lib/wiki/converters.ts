/**
 * The one converter left: a day's EVENT, from the wire into the shape four
 * chart components read.
 *
 * There were five. The other four turned a person, place, organization and day
 * into "page types" whose distinctive contributions were camelCase renames and
 * fields no converter ever filled — `company`, `role`, `location`,
 * `connectionTier`, `city`, `subtitle`, and two permanently-empty objects that
 * a whole section of the day page was gated on. Components take the wire shape
 * now, as the year page always has.
 *
 * **This one stays, and not for lack of appetite.** It looks like the four that
 * went and is nothing like them. It parses two strings into dates; it COMPUTES
 * `durationMinutes`, which is not on the wire at all; it applies defaults; it
 * distinguishes `|| undefined` from `?? undefined` so that an empty string and
 * an absent value are told apart; and it guards two jsonb columns that arrive
 * untyped. Delete it and every one of those moves into the eighty-eight places
 * that read the result.
 *
 * The four that went renamed fields and filled in blanks that stayed blank.
 * The test is whether the translation does work the caller would otherwise
 * have to do — not whether a translation exists.
 */

import type { TemporalEventApi } from "./api";
import type { DayEvent } from "./types/day";

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

