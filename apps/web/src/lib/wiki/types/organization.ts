/**
 * Wiki Types - Organization Page
 *
 * Employers, schools, communities, institutions.
 *
 * Organization pages track your relationship with groups -
 * when you joined, your role, key people, and what it meant.
 */

import type { WikiPageBase, LinkedPage, DateRange } from "./base";

// =============================================================================
// ORGANIZATION TYPE
// =============================================================================

export type OrganizationType =
	| "employer" // Company you work(ed) for
	| "school" // Educational institution
	| "community" // Church, club, group
	| "institution" // Government, hospital, etc.
	| "other";

// =============================================================================
// ORGANIZATION PAGE
// =============================================================================

export interface OrganizationPage extends WikiPageBase {
	type: "organization";

	// ─────────────────────────────────────────────────────────────
	// Classification
	// ─────────────────────────────────────────────────────────────

	/**
	 * What kind of organization this is.
	 */
	orgType: OrganizationType;

	// ─────────────────────────────────────────────────────────────
	// Your Involvement
	// ─────────────────────────────────────────────────────────────

	/**
	 * Period of your involvement.
	 * End is undefined if ongoing.
	 */
	period?: DateRange;

	/**
	 * Your role or title.
	 * Examples: "Software Engineer", "Student", "Member"
	 */
	role?: string;

	// ─────────────────────────────────────────────────────────────
	// Narrative
	// ─────────────────────────────────────────────────────────────

	/**
	 * The main narrative content (markdown).
	 * Your experience with this organization.
	 */
	content: string;

	// ─────────────────────────────────────────────────────────────
	// Connections
	// ─────────────────────────────────────────────────────────────

	/**
	 * Key people you know there.
	 */
	keyContacts: LinkedPage[];

	/**
	 * Physical locations of this organization.
	 */
	locations: LinkedPage[];

	/**
	 * Acts/chapters associated with this organization.
	 */
	narrativeContext: LinkedPage[];
}
