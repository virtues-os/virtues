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
// CONTEXT VECTOR (Day completeness)
// =============================================================================

/**
 * 7-dimension context vector for measuring day completeness.
 * Each dimension is a 0-1 score representing data coverage.
 */
export interface ContextVector {
	who: number; // Self-awareness — is the person's own state tracked?
	whom: number; // Relational — who else was involved?
	what: number; // Events & content — what happened?
	when: number; // Temporal coverage — how much of 24h is observed?
	where: number; // Spatial — do we know locations?
	why: number; // Intent/motivation — do we know purpose?
	how: number; // Means/method/process
}

export function computeCompleteness(cv: ContextVector): number {
	const values = [cv.who, cv.whom, cv.what, cv.when, cv.where, cv.why, cv.how];
	return values.reduce((sum, v) => sum + v, 0) / values.length;
}

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

	// W6H activation vector (7 dimensions: who, whom, what, when, where, why, how)
	w6hActivation: [number, number, number, number, number, number, number] | null;

	// Entropy scores
	/** Semantic distinctness: 1 - cosine_sim(this_event, day_centroid). How different from day average. */
	entropy: number | null;
	/** Shannon entropy of W6H activation vector. Internal complexity/richness. */
	w6hEntropy: number | null;

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

	/** W5H completeness scores */
	contextVector: ContextVector;

	/** Chaos/order score (0 = ordered/routine, 1 = chaotic/novel). Null if not yet computed. */
	chaosScore: number | null;

	/** How many prior days contributed to entropy calibration. Null = never computed, 0 = baseline. */
	entropyCalibrationDays: number | null;

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
}
