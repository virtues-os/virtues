/**
 * Wiki Types - Day Page
 *
 * Daily log with events, people, reflections.
 * Resolution: 24 hours
 *
 * Day pages are the atomic unit of the personal wiki.
 * They use a three-layer model:
 *   Layer 1: Data (citations, entities, context) - always additive
 *   Layer 2: Timeline (events) - user-editable with preservation
 *   Layer 3: Autobiography (narrative) - on-demand regeneration
 */

import type { WikiPageBase, LinkedPage, Citation, AuthorType, SectionAuthorType } from "./base";

// =============================================================================
// DAY EVENT (Layer 2: Timeline)
// =============================================================================

/**
 * A single event in the day timeline.
 *
 * Timeline events are semi-structured: auto-generated from ontology data,
 * but users can edit labels, add notes, or create manual events.
 * User edits are preserved when new data triggers regeneration.
 */
export interface DayEvent {
	id: string;
	startTime: Date;
	endTime: Date;
	durationMinutes: number;

	// Auto-generated from ontology data
	autoLabel: string; // "Work", "Transit", "Sleep", "Unknown"
	autoLocation?: string; // From location_visit
	sourceIds: string[]; // Which ontology rows generated this

	// User overrides (preserved on regeneration)
	userLabel?: string; // "Architecture review with team"
	userLocation?: string; // Override auto-detected place
	userNotes?: string; // Brief annotation

	// Dayline: Novelty (Novel ↑ / Routine ↓)
	noveltyZ: number | null; // z-scored novelty vs 12-week baseline
	// Dayline: Autonomic (Stress ↑ / Recovery ↓)
	autonomicZ: number | null; // z-scored HR/HRV vs embedding-similar past events
	avgHr: number | null; // average heart rate during event
	hrZ: number | null; // HR z-score (raw, before context gating)
	hrvZ: number | null; // HRV z-score (raw, when available)

	// Dayline: Event structure
	topics: string[]; // Activity contexts (e.g., "code review", "grocery run")
	eventSummary: string | null; // 1-3 factual sentences (embedded for novelty)
	agentAction: "NEW" | "CONTINUE" | "REVISE" | "NO_DATA" | null;

	// Dayline: Classification
	isSleep: boolean;
	userHidden: boolean; // Soft delete
	userCreated: boolean; // User-created, never modified by recompute

	// Entity/topic novelty
	entities: string[]; // Wiki entity IDs (person_demo_maya, place_demo_office, etc.)
	topicNovelty: Record<string, number> | null; // Per-topic z-scores
	entityNovelty: Record<string, number> | null; // Per-entity z-scores
	entityTimestamps: Record<string, string> | null; // entity_id → earliest ISO timestamp within event

	// Tracking
	isUserAdded: boolean; // Manually created by user (never auto-update)
	isUserEdited: boolean; // Auto-event but user modified something
	isTransit?: boolean;
	isUnknown?: boolean;
}

/**
 * Get the display label for an event (user override or auto-generated).
 */
export function getEventDisplayLabel(event: DayEvent): string {
	return event.userLabel ?? event.autoLabel;
}

/**
 * Get the display location for an event (user override or auto-generated).
 */
export function getEventDisplayLocation(event: DayEvent): string | undefined {
	return event.userLocation ?? event.autoLocation;
}

// =============================================================================
// LINKED ENTITIES (Layer 1: Data)
// =============================================================================

/**
 * Linked entities grouped by type.
 * These are the "nouns" of a day — people, places, orgs mentioned.
 */
export interface LinkedEntities {
	people: LinkedPage[];
	places: LinkedPage[];
	organizations: LinkedPage[];
}

/**
 * Create an empty LinkedEntities structure.
 */
export function emptyLinkedEntities(): LinkedEntities {
	return {
		people: [],
		places: [],
		organizations: [],
	};
}

/**
 * Get all linked pages from a LinkedEntities structure as a flat array.
 * Useful for [[wiki link]] resolution in the editor.
 */
export function flattenLinkedEntities(entities: LinkedEntities): LinkedPage[] {
	return [...entities.people, ...entities.places, ...entities.organizations];
}

// =============================================================================
// LINKED TEMPORAL (Layer 1: Data)
// =============================================================================

/**
 * Linked temporal pages — the narrative context of a day.
 * Where does this day sit in the story hierarchy?
 */
export interface LinkedTemporal {
	// Parent context (what chapter/act is this day part of?)
	act?: LinkedPage;
	chapter?: LinkedPage;

	// Sibling context (adjacent days)
	previousDay?: LinkedPage;
	nextDay?: LinkedPage;

	// Notable moments from this day (could become standalone pages)
	events: LinkedPage[];

	// Related temporal pages (AI-suggested similar days)
	related: LinkedPage[];
}

/**
 * Create an empty LinkedTemporal structure.
 */
export function emptyLinkedTemporal(): LinkedTemporal {
	return {
		events: [],
		related: [],
	};
}

// =============================================================================
// AUTOBIOGRAPHY SECTIONS (Layer 3: Narrative)
// =============================================================================

/**
 * A section of the autobiography with authorship tracking.
 * Sections have freeform headings (not fixed to Morning/Afternoon/Evening).
 */
export interface AutobiographySection {
	id: string;
	heading: string; // Freeform: "Morning", "The Call", "Reflections", etc.
	content: string; // Markdown content
	authoredBy: SectionAuthorType;
	lastEditedAt: Date;
}

// =============================================================================
// DAY PAGE
// =============================================================================

/**
 * A Day Page with three layers: Data, Timeline, Autobiography.
 *
 * Day pages are the atomic unit of the personal wiki — each day is a potential
 * wiki page that can be auto-generated from ontology data and refined by the user.
 *
 * The three layers have different update semantics:
 * - Layer 1 (Data): Always additive, no conflict — citations, entities, context
 * - Layer 2 (Timeline): Auto-update with preservation — new events added, user edits kept
 * - Layer 3 (Autobiography): On-demand regeneration — user clicks "Regenerate" to update
 */
export interface DayPage extends WikiPageBase {
	type: "day";

	// ─────────────────────────────────────────────────────────────
	// Temporal Identity
	// ─────────────────────────────────────────────────────────────

	date: Date;
	dayOfWeek: string;
	startTimezone: string | null;
	endTimezone: string | null;

	// ─────────────────────────────────────────────────────────────
	// LAYER 1: Data (always additive, no conflict)
	// ─────────────────────────────────────────────────────────────

	/** Entities mentioned this day (people, places, organizations, things) */
	linkedEntities: LinkedEntities;

	/** Narrative hierarchy context (act, chapter, adjacent days) */
	linkedTemporal: LinkedTemporal;

	// ─────────────────────────────────────────────────────────────
	// LAYER 2: Timeline (semi-structured, user-editable)
	// ─────────────────────────────────────────────────────────────

	/** Timeline events from 00:00 to 24:00 */
	events: DayEvent[];

	// ─────────────────────────────────────────────────────────────
	// LAYER 3: Autobiography (narrative, on-demand regeneration)
	// ─────────────────────────────────────────────────────────────

	/** The AI-generated narrative of the day (markdown) */
	autobiography: string;

	/** Section-level tracking for granular authorship */
	autobiographySections?: AutobiographySection[];

	/** One-line literary subtitle for the day (Austen register, generated alongside autobiography) */
	epigraph?: string;

	/** Whether this day has a generated illustration BLOB (served via /api/wiki/day/:date/illustration) */
	hasIllustration: boolean;

	// ─────────────────────────────────────────────────────────────
	// Data Quality (W6H journalist assessment, nightly)
	// ─────────────────────────────────────────────────────────────

	/** W6H data quality assessment — 1-5 per dimension, overall score, and note */
	dataQuality?: DataQuality;

	/** Count of entities first referenced on this day */
	newEntityCount: number;
	/** Count of topics first seen on this day */
	newTopicCount: number;

	// ─────────────────────────────────────────────────────────────
	// Readiness (morning autonomic state, 0-100)
	// ─────────────────────────────────────────────────────────────

	/** Morning readiness score (0-100) from overnight HRV, RHR, sleep */
	readinessScore: number | null;
	/** Component breakdown */
	readinessDetails: ReadinessDetails | null;

	// ─────────────────────────────────────────────────────────────
	// Sleep cycles (computed at query time from ontology data)
	// ─────────────────────────────────────────────────────────────

	/** Scored sleep cycles derived from sleep stages + heart rate data */
	sleepCycles: ScoredSleepCycle[];
}

/** A scored sleep cycle, derived at query time from sleep stages + HR data */
export interface ScoredSleepCycle {
	startTime: Date;
	endTime: Date;
	dominantStage: string; // "deep", "core", "rem"
	avgHr: number | null;
	autonomicZ: number | null;
}

/** Readiness score component breakdown (each 0-100) */
export interface ReadinessDetails {
	hrv: number;
	rhr: number;
	sleep_duration: number;
	deep_rem: number;
	consistency: number;
}

/** LLM-assessed data quality using the W6H journalist framework */
export interface DataQuality {
	coverage: {
		who: number;
		whom: number;
		what: number;
		when: number;
		where: number;
		why: number;
		how: number;
	};
	overall: number;
	note: string;
}
