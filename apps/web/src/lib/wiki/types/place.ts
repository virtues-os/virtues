/**
 * Wiki Types - Place Page
 *
 * Locations with meaning.
 * Home, work, third places, travel destinations.
 *
 * Place pages track your relationship with locations -
 * when you first visited, how often, what you do there,
 * and what they mean to you.
 */

import type { WikiPageBase, LinkedPage } from "./base";

// =============================================================================
// PLACE TYPE
// =============================================================================

export type PlaceType =
	| "home" // Where you live
	| "work" // Office, workplace
	| "third-place" // Café, library, gym - neither home nor work
	| "transit" // Airport, station
	| "travel" // Vacation, trip destination
	| "other";

// =============================================================================
// COORDINATES
// =============================================================================

export interface Coordinates {
	lat: number;
	lng: number;
}

// =============================================================================
// PLACE PAGE
// =============================================================================

export interface PlacePage extends WikiPageBase {
	type: "place";

	// ─────────────────────────────────────────────────────────────
	// Location
	// ─────────────────────────────────────────────────────────────

	/**
	 * GPS coordinates (optional).
	 */
	coordinates?: Coordinates;

	/**
	 * Human-readable address.
	 */
	address?: string;

	/**
	 * City/region for display.
	 */
	city?: string;

	/**
	 * What kind of place this is.
	 */
	placeType: PlaceType;

	// ─────────────────────────────────────────────────────────────
	// Temporal Relationship
	// ─────────────────────────────────────────────────────────────

	/**
	 * First time you visited/lived here.
	 */
	firstVisit?: Date;

	/**
	 * Most recent visit.
	 */
	lastVisit?: Date;

	/**
	 * Total number of visits (from location data).
	 */
	visitCount?: number;

	// ─────────────────────────────────────────────────────────────
	// Narrative
	// ─────────────────────────────────────────────────────────────

	/**
	 * The main narrative content (markdown).
	 * What this place means to you.
	 */
	content: string;

	/**
	 * Why this place matters - a brief significance statement.
	 */
	significance?: string;

	// ─────────────────────────────────────────────────────────────
	// Connections
	// ─────────────────────────────────────────────────────────────

	/**
	 * People associated with this place.
	 */
	associatedPeople: LinkedPage[];

	/**
	 * Things you do here (activities, routines).
	 */
	activities: string[];

	/**
	 * Acts/chapters that took place here.
	 */
	narrativeContext: LinkedPage[];
}
