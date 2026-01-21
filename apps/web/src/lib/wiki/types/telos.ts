/**
 * Wiki Types - Telos Page
 *
 * Life aims and highest-order orientation.
 * Resolution: Lifetime
 *
 * A telos represents your ultimate purpose or direction in life.
 * It's the "why" behind everything - the North Star that guides
 * all other narrative structures (acts, chapters, days).
 */

import type { WikiPageBase, LinkedPage } from "./base";

// =============================================================================
// TELOS PAGE
// =============================================================================

export interface TelosPage extends WikiPageBase {
	type: "telos";

	// ─────────────────────────────────────────────────────────────
	// Core Identity
	// ─────────────────────────────────────────────────────────────

	/**
	 * The 3-5 core values that guide this telos.
	 * Examples: "wisdom", "courage", "justice", "temperance"
	 */
	coreValues: string[];

	/**
	 * Long-term vision statement (markdown).
	 * What does fulfillment of this telos look like?
	 */
	visionStatement: string;

	// ─────────────────────────────────────────────────────────────
	// Narrative Structure
	// ─────────────────────────────────────────────────────────────

	/**
	 * The main narrative content (markdown).
	 * Articulates the telos, its origins, and its meaning.
	 */
	content: string;

	/**
	 * Child acts under this telos.
	 * Major life seasons that serve this higher purpose.
	 */
	acts: LinkedPage[];

	// ─────────────────────────────────────────────────────────────
	// Connections
	// ─────────────────────────────────────────────────────────────

	/**
	 * Key ideas/philosophies that inform this telos.
	 */
	guidingIdeas: LinkedPage[];

	/**
	 * Key people who shaped or share this telos.
	 */
	influentialPeople: LinkedPage[];
}
