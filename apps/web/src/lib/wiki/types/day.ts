/**
 * The shape the day page's Data view reads: a day's EVENTS.
 *
 * This file was 273 lines and the wiki's page-type layer hung off it — a
 * `DayPage` extending a `WikiPageBase`, two "linked" structures nothing ever
 * filled, and an `AutobiographySection` for a three-layer model that was never
 * built. Components take the wire shape from `$lib/wiki/api` now.
 *
 * What survives is the one genuine translation. `DayEvent` is read by
 * DaylineChart, EventTimeline and DayPage, and it is a different shape from
 * the wire rather than a renaming of it — so moving those three onto
 * snake_case is a change to make on its own, with the charts in front of you.
 */

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

	// How unlike the owner's usual: global (rare in your life at all) and
	// local (off-pattern for its kind). `usualScore` picks between them.
	noveltyZ: number | null;
	localNoveltyZ: number | null;
	// Dayline: Autonomic (Stress ↑ / Recovery ↓)
	autonomicZ: number | null; // z-scored HR/HRV vs embedding-similar past events
	avgHr: number | null; // average heart rate during event
	hrZ: number | null; // HR z-score (raw, before context gating)

	// Dayline: Event structure
	topics: string[]; // Activity contexts (e.g., "code review", "grocery run")
	eventSummary: string | null; // 1-3 factual sentences (embedded for novelty)
	agentAction: "NEW" | "CONTINUE" | "REVISE" | "NO_DATA" | null;

	// Dayline: Classification
	isSleep: boolean;
	userHidden: boolean; // Soft delete

	// Entity/topic novelty
	entities: string[]; // Wiki subject ids — person_…, place_…, org_…
	/** `{id: name}` for the ids above, resolved by the server, because nothing
	 *  on this side can turn `person_a1b2c3d4` into a person. An id missing
	 *  from this map no longer exists and is not drawn. */
	entityNames: Record<string, string>;
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
