/**
 * Wiki Types - Thing Page
 *
 * Ideas, objects, concepts, frameworks.
 * Philosophies, books, possessions, beliefs.
 *
 * "Thing" is intentionally broad - it covers anything
 * that isn't a person, place, organization, or temporal unit.
 */

import type { WikiPageBase, LinkedPage } from "./base";

// =============================================================================
// THING TYPE
// =============================================================================

export type ThingType =
	| "philosophy" // Stoicism, Buddhism, etc.
	| "framework" // GTD, mental models, etc.
	| "belief" // Personal beliefs, values
	| "book" // Books that shaped you
	| "object" // Physical possessions with meaning
	| "concept" // Abstract ideas
	| "other";

// =============================================================================
// THING PAGE
// =============================================================================

export interface ThingPage extends WikiPageBase {
	type: "thing";

	// ─────────────────────────────────────────────────────────────
	// Classification
	// ─────────────────────────────────────────────────────────────

	/**
	 * What kind of thing this is.
	 */
	thingType: ThingType;

	// ─────────────────────────────────────────────────────────────
	// Discovery
	// ─────────────────────────────────────────────────────────────

	/**
	 * When and how you first encountered this.
	 */
	firstEncountered?: {
		date: Date;
		context: string; // "Philosophy class", "Recommended by Sarah"
	};

	/**
	 * Person who introduced you to this (if any).
	 */
	introducedBy?: LinkedPage;

	// ─────────────────────────────────────────────────────────────
	// Narrative
	// ─────────────────────────────────────────────────────────────

	/**
	 * The main narrative content (markdown).
	 * What this thing is and what it means to you.
	 */
	content: string;

	// ─────────────────────────────────────────────────────────────
	// Structure (for philosophies/frameworks)
	// ─────────────────────────────────────────────────────────────

	/**
	 * Core tenets or principles (for philosophies/frameworks).
	 */
	coreTenets?: string[];

	/**
	 * Key texts or sources.
	 */
	keyTexts?: {
		title: string;
		author?: string;
		year?: number;
	}[];

	// ─────────────────────────────────────────────────────────────
	// Connections
	// ─────────────────────────────────────────────────────────────

	/**
	 * Related things (connected philosophies, frameworks, etc.).
	 */
	relatedThings: LinkedPage[];

	/**
	 * People associated with this thing.
	 * Authors, teachers, fellow practitioners.
	 */
	associatedPeople: LinkedPage[];

	/**
	 * Places associated with this thing.
	 * Where you practice it, where you learned it.
	 */
	associatedPlaces: LinkedPage[];

	/**
	 * Narrative context (acts/chapters where this was significant).
	 */
	narrativeContext: LinkedPage[];
}
