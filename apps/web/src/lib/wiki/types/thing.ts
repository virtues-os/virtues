/**
 * Wiki Types - Thing Page
 *
 * Catchall entity type for anything that isn't a person, place, or organization.
 * Dogs, projects, concepts, tools, hobbies, etc.
 */

import type { WikiPageBase } from "./base";

// =============================================================================
// THING PAGE
// =============================================================================

export interface ThingPage extends WikiPageBase {
	type: "thing";

	/**
	 * Freeform category: "pet", "project", "concept", etc.
	 */
	category?: string;

	/**
	 * Short description / subtitle.
	 */
	description?: string;

	/**
	 * The main narrative content (markdown).
	 */
	content: string;
}
