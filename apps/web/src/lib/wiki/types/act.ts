/**
 * Wiki Types - Act Page
 *
 * Major life seasons.
 * Resolution: Multi-year
 *
 * An act represents a significant era of life - college years,
 * a career chapter, raising children, etc. Acts contain chapters
 * and serve a parent telos.
 */

import type { WikiPageBase, LinkedPage, DateRange } from "./base";

// =============================================================================
// ACT PAGE
// =============================================================================

export interface ActPage extends WikiPageBase {
	type: "act";

	// ─────────────────────────────────────────────────────────────
	// Temporal
	// ─────────────────────────────────────────────────────────────

	/**
	 * The time span of this act.
	 * End is undefined if the act is ongoing.
	 */
	period: DateRange;

	/**
	 * Primary location during this act.
	 * Examples: "Austin, Texas", "Remote / Nomadic"
	 */
	location?: string;

	// ─────────────────────────────────────────────────────────────
	// Narrative Structure
	// ─────────────────────────────────────────────────────────────

	/**
	 * The main narrative content (markdown).
	 * The story of this act - what happened, what it meant.
	 */
	content: string;

	/**
	 * Parent telos this act serves.
	 */
	telos?: LinkedPage;

	/**
	 * Child chapters within this act.
	 */
	chapters: LinkedPage[];

	// ─────────────────────────────────────────────────────────────
	// Connections
	// ─────────────────────────────────────────────────────────────

	/**
	 * Central figures in this act.
	 * The people who defined this era.
	 */
	keyPeople: LinkedPage[];

	/**
	 * Key places associated with this act.
	 */
	keyPlaces: LinkedPage[];

	/**
	 * Recurring themes in this act.
	 * Examples: "growth", "struggle", "discovery"
	 */
	themes: string[];
}
