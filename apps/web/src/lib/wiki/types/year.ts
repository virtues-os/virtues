/**
 * Wiki Types - Year Page
 *
 * Calendar year overview.
 * Resolution: Annual
 *
 * A year page provides a calendar-based view of a specific year,
 * showing activity density by month, linking to significant days,
 * and summarizing the year's key events and themes.
 */

import type { WikiPageBase, LinkedPage, DateRange } from "./base";

// =============================================================================
// YEAR PAGE
// =============================================================================

/**
 * Month summary for the year overview.
 */
export interface MonthSummary {
	/** Month number (1-12) */
	month: number;
	/** Number of days with recorded activity */
	activeDays: number;
	/** Total days in the month */
	totalDays: number;
	/** Key events/highlights for this month */
	highlights: string[];
}

export interface YearPage extends WikiPageBase {
	type: "year";

	// ─────────────────────────────────────────────────────────────
	// Temporal
	// ─────────────────────────────────────────────────────────────

	/**
	 * The year number (e.g., 2024).
	 */
	year: number;

	/**
	 * Full date range for the year.
	 */
	period: DateRange;

	// ─────────────────────────────────────────────────────────────
	// Content
	// ─────────────────────────────────────────────────────────────

	/**
	 * Year summary/reflection (markdown).
	 * Written retrospectively about what the year meant.
	 */
	content: string;

	/**
	 * Month-by-month activity summaries.
	 */
	months: MonthSummary[];

	// ─────────────────────────────────────────────────────────────
	// Connections
	// ─────────────────────────────────────────────────────────────

	/**
	 * Acts that overlap with this year.
	 */
	acts: LinkedPage[];

	/**
	 * Chapters that overlap with this year.
	 */
	chapters: LinkedPage[];

	/**
	 * Significant days worth highlighting.
	 */
	significantDays: LinkedPage[];

	/**
	 * Key people from this year.
	 */
	keyPeople: LinkedPage[];

	/**
	 * Key places visited/lived in this year.
	 */
	keyPlaces: LinkedPage[];

	/**
	 * Recurring themes for the year.
	 */
	themes: string[];
}
