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
	anchor: 'Your devices — start here',
	prominent: 'Your accounts',
	quiet: 'More of your world'
};

// THIS MAC LEADS, then the phone, then accounts.
//
// Google was the anchor, on the reasoning that it is the easiest to connect. It
// is not the one that pays off first. The Mac is LOCAL: no OAuth, no round-trip
// out of the app, no server between you and years of iMessage — and it is the
// only source that can produce something to LOOK at while you are still sitting
// here. Everything else makes you wait.
//
// The devices also carry the argument. "Read locally on your Mac, never sent to
// Virtues" is the claim the letter just made, arriving as the first thing you
// are asked to do rather than a footnote under an OAuth screen.
export const SOURCE_COPY: Record<string, SourceCopy> = {
	mac: {
		prominence: 'anchor',
		why: 'This Mac — your iMessages, the apps you use, the sites you visit. Nothing to sign into, and years of history already on the disk.',
		receipt: 'read locally on your Mac · never sent to Virtues'
	},
	ios: {
		prominence: 'anchor',
		why: 'Your iPhone — where you go, who you call, your health, your photos. The half of your life that never touches a computer.',
		receipt: 'read on your device · stays on your box'
	},
	google: {
		prominence: 'prominent',
		why: 'Gmail and Google Calendar — who you write to, and what your weeks actually looked like.',
		receipt: 'read-only · stays on your box'
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
