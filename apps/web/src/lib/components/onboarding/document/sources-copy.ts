/**
 * sources-copy — the editorial layer over the source catalog for onboarding.
 *
 * The catalog (actions/sources.toml) carries terse, technical descriptions.
 * Onboarding needs a WHY (adults connect a source when they know what it buys
 * them) and a privacy receipt (the honest "stays on your box" line), plus a
 * prominence so the richest sources lead. Anything not listed falls back to
 * the catalog description, a generic receipt, and the "quiet" group.
 */

export type Prominence = 'anchor' | 'prominent' | 'quiet';

export interface SourceCopy {
	/** Why this is worth connecting — concrete, in-voice. */
	why: string;
	/** The honest privacy line, mono receipt. */
	receipt: string;
	prominence: Prominence;
}

export const PROMINENCE_ORDER: Prominence[] = ['anchor', 'prominent', 'quiet'];

export const PROMINENCE_HEADING: Record<Prominence, string> = {
	anchor: 'Start here',
	prominent: 'The richest sources',
	quiet: 'More of your world'
};

export const SOURCE_COPY: Record<string, SourceCopy> = {
	google: {
		prominence: 'anchor',
		why: 'Your Gmail and Google Calendar — who you talk to and what’s on your schedule. The easiest place to start.',
		receipt: 'read-only · stays on your box'
	},
	ios: {
		prominence: 'prominent',
		why: 'Your iPhone — location, messages, calls, health, and photos.',
		receipt: 'read on your device · stays on your box'
	},
	mac: {
		prominence: 'prominent',
		why: 'Your Mac — the apps you use, the sites you visit, and your iMessages.',
		receipt: 'read locally on your Mac · never sent to Virtues'
	},
	chat_import: {
		prominence: 'prominent',
		why: 'Your past conversations with Claude, ChatGPT, and Gemini.',
		receipt: 'parsed on your box · the file never leaves your hardware'
	},
	plaid: {
		prominence: 'quiet',
		why: 'Your bank transactions — what you spend and where.',
		receipt: 'read-only · stays on your box'
	},
	notion: {
		prominence: 'quiet',
		why: 'Your Notion pages, docs, and databases.',
		receipt: 'read-only · stays on your box'
	},
	strava: {
		prominence: 'quiet',
		why: 'Your Strava runs, rides, and swims.',
		receipt: 'read-only · stays on your box'
	}
};

const GENERIC_RECEIPT = 'read-only · stays on your box · Virtues never sees content';

export function copyFor(id: string, fallbackDescription: string): SourceCopy {
	return (
		SOURCE_COPY[id] ?? {
			why: fallbackDescription,
			receipt: GENERIC_RECEIPT,
			prominence: 'quiet'
		}
	);
}
