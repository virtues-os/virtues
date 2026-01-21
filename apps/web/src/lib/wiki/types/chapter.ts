/**
 * Wiki Types - Chapter Page
 *
 * Coherent arcs within acts.
 * Resolution: Months to a year
 *
 * A chapter represents a distinct phase within an act -
 * a project, a relationship arc, a period of growth or struggle.
 * Chapters have narrative shape (rising, falling, etc.).
 */

import type { WikiPageBase, LinkedPage, DateRange } from "./base";

// =============================================================================
// NARRATIVE ARC
// =============================================================================

/**
 * The shape of a chapter's narrative arc.
 */
export type NarrativeArc =
	| "rising" // Things getting better, building toward something
	| "falling" // Decline, loss, struggle
	| "stable" // Maintenance, steady state
	| "transition" // Between states, liminal
	| "cyclical"; // Repeating pattern

// =============================================================================
// CHAPTER PAGE
// =============================================================================

export interface ChapterPage extends WikiPageBase {
	type: "chapter";

	// ─────────────────────────────────────────────────────────────
	// Temporal
	// ─────────────────────────────────────────────────────────────

	/**
	 * The time span of this chapter.
	 * End is undefined if the chapter is ongoing.
	 */
	period: DateRange;

	// ─────────────────────────────────────────────────────────────
	// Narrative Structure
	// ─────────────────────────────────────────────────────────────

	/**
	 * The main narrative content (markdown).
	 * The story of this chapter.
	 */
	content: string;

	/**
	 * Parent act this chapter belongs to.
	 */
	act?: LinkedPage;

	/**
	 * The narrative arc of this chapter.
	 */
	arc: NarrativeArc;

	// ─────────────────────────────────────────────────────────────
	// Connections
	// ─────────────────────────────────────────────────────────────

	/**
	 * Key people in this chapter.
	 */
	keyPeople: LinkedPage[];

	/**
	 * Key places in this chapter.
	 */
	keyPlaces: LinkedPage[];

	/**
	 * Notable days within this chapter.
	 * Days that were particularly significant.
	 */
	notableDays: LinkedPage[];

	/**
	 * What was learned during this chapter.
	 * Insights, lessons, realizations.
	 */
	lessons: string[];
}
