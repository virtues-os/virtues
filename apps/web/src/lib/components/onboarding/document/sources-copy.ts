/**
 * sources-copy — the editorial layer over the source catalog for onboarding.
 *
 * The catalog (actions/sources.toml) carries terse, technical descriptions.
 * Onboarding needs a WHY (adults connect a source when they know what it buys
 * them) and a prominence so the richest sources lead. Anything not listed
 * falls back to the catalog description and the "quiet" group.
 *
 * Whys are ONE short line of concrete nouns — what the source holds, nothing
 * else. The argumentative second sentences were cut 2026-08-21 along with the
 * per-row privacy receipts (written for a margin that no longer exists, never
 * rendered by SourceRow).
 */

export type Prominence = 'anchor' | 'prominent' | 'quiet';

export interface SourceCopy {
	/** Why this is worth connecting — concrete nouns, one line. */
	why: string;
	prominence: Prominence;
	/**
	 * Order WITHIN a prominence group, low first.
	 *
	 * Prominence alone was not enough: the Mac and the iPhone are both anchors,
	 * and with nothing to separate them the list fell back to catalog order and
	 * put the phone first. The Mac has to lead — it is the machine the person is
	 * sitting at, it needs no OAuth, and it is the only source that can produce
	 * something to look at before they get up.
	 */
	rank?: number;
}

export const PROMINENCE_ORDER: Prominence[] = ['anchor', 'prominent', 'quiet'];

// THIS MAC LEADS, then the phone, then accounts.
//
// Google was the anchor, on the reasoning that it is the easiest to connect. It
// is not the one that pays off first. The Mac is LOCAL: no OAuth, no round-trip
// out of the app, no server between you and years of iMessage — and it is the
// only source that can produce something to LOOK at while you are still sitting
// here. Everything else makes you wait.
export const SOURCE_COPY: Record<string, SourceCopy> = {
	mac: {
		prominence: 'anchor',
		rank: 0,
		why: 'Your iMessages, apps, and browsing — years of it already on this disk.'
	},
	ios: {
		prominence: 'anchor',
		rank: 1,
		why: 'Where you go, who you call, your health, your photos.'
	},
	google: {
		prominence: 'prominent',
		why: 'Gmail and Calendar — who you write to, where your weeks go.'
	},
	chat_import: {
		prominence: 'prominent',
		why: 'Your past conversations with Claude, ChatGPT, and Gemini.'
	},
	plaid: {
		prominence: 'quiet',
		why: 'What you spend, and where.'
	},
	notion: {
		prominence: 'quiet',
		why: 'Your pages, docs, and databases.'
	},
	strava: {
		prominence: 'quiet',
		why: 'Your runs, rides, and swims.'
	}
};

export function copyFor(id: string, fallbackDescription: string): SourceCopy {
	return (
		SOURCE_COPY[id] ?? {
			why: fallbackDescription,
			prominence: 'quiet'
		}
	);
}
