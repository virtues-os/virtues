/**
 * Wiki Types - Person Page
 *
 * Personal CRM for people in your life.
 * Friends, family, colleagues, acquaintances.
 */

import type { WikiPageBase } from "./base";

// =============================================================================
// CONNECTION TIER
// =============================================================================

/**
 * How close/important this person is to you.
 * Helps prioritize who to stay in touch with.
 */
export type ConnectionTier = "inner-circle" | "close" | "regular" | "distant" | "acquaintance";

// =============================================================================
// CONTACT FREQUENCY
// =============================================================================

/**
 * How often you typically stay in touch with this person.
 */
export type ContactFrequency = "daily" | "weekly" | "monthly" | "quarterly" | "yearly" | "rare" | "lost-touch";

// =============================================================================
// SOCIAL LINKS
// =============================================================================

export interface SocialLinks {
	linkedin?: string;
	twitter?: string;
	instagram?: string;
	facebook?: string;
}

// =============================================================================
// PERSON PAGE
// =============================================================================

export interface PersonPage extends WikiPageBase {
	type: "person";

	// ─────────────────────────────────────────────────────────────
	// Identity
	// ─────────────────────────────────────────────────────────────

	/**
	 * Nickname or preferred name (if different from title).
	 */
	nickname?: string;

	/**
	 * Primary relationship category.
	 * Examples: "Best Friend", "Colleague", "Family", "Mentor"
	 */
	relationship: string;

	/**
	 * Connection tier - how close/important.
	 */
	connectionTier?: ConnectionTier;

	// ─────────────────────────────────────────────────────────────
	// Contact Info
	// ─────────────────────────────────────────────────────────────

	/**
	 * Email addresses (primary first).
	 */
	emails?: string[];

	/**
	 * Phone numbers (primary first).
	 */
	phones?: string[];

	/**
	 * Social media links.
	 */
	socials?: SocialLinks;

	// ─────────────────────────────────────────────────────────────
	// About
	// ─────────────────────────────────────────────────────────────

	/**
	 * Where they currently live (city/region).
	 */
	location?: string;

	/**
	 * Alias for location (used by PersonTable).
	 */
	currentLocation?: string;

	/**
	 * Company or organization they work at.
	 */
	company?: string;

	/**
	 * Their role/job title.
	 */
	role?: string;

	/**
	 * Birthday.
	 */
	birthday?: Date;

	// ─────────────────────────────────────────────────────────────
	// Contact Tracking (CRM)
	// ─────────────────────────────────────────────────────────────

	/**
	 * How often you typically stay in touch.
	 */
	contactFrequency?: ContactFrequency;

	/**
	 * When you last contacted or met this person.
	 */
	lastContact?: Date;

	// ─────────────────────────────────────────────────────────────
	// Notes
	// ─────────────────────────────────────────────────────────────

	/**
	 * Free-form notes (markdown).
	 * How you met, context, memories, anything relevant.
	 */
	content: string;
}
