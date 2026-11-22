/**
 * The assistant's switches for a source's own tools (`api/capabilities.rs`),
 * in words. Settings' Tools list and the source's page both use these, so a
 * switch reads the same in both places.
 */
import type { Capability } from '$lib/api/client';

type Words = { label: string; help: string };

export const GOOGLE_WORDS: Record<'email' | 'calendar', Words> = {
	email: {
		label: 'Archive, label and draft mail',
		help: 'In your Google mail. It never sends mail: drafts wait in Gmail until you send them.'
	},
	calendar: {
		label: 'Add and change events',
		help: 'In your Google calendar. It asks you before it deletes one.'
	}
};

/** The switches a source's page shows, by source id. */
export const SOURCE_CAPABILITIES: Record<string, Partial<Record<Capability['id'], Words>>> = {
	google: GOOGLE_WORDS
};
