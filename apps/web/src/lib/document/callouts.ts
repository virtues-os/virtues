/**
 * A callout's tones, as the contract names them, each with its label and
 * mark. A page's callout (CalloutNode) and chat's alert (Markdown.svelte)
 * draw the same ones.
 */

export const CALLOUT_TONES = [
	{ tone: 'note', label: 'Note', icon: 'ri:information-line' },
	{ tone: 'tip', label: 'Tip', icon: 'ri:lightbulb-line' },
	{ tone: 'warning', label: 'Warning', icon: 'ri:alert-line' },
] as const;

export type CalloutTone = (typeof CALLOUT_TONES)[number];

/** The tone named `tone`, or Note. */
export function calloutTone(tone: unknown): CalloutTone {
	return CALLOUT_TONES.find((t) => t.tone === tone) ?? CALLOUT_TONES[0];
}

/**
 * A GitHub alert's kind (`> [!IMPORTANT]`) as a callout's tone, as the
 * converter reads it (crates/virtues-document migrate.rs).
 */
export function alertTone(kind: string): CalloutTone {
	switch (kind.toLowerCase()) {
		case 'tip':
			return CALLOUT_TONES[1];
		case 'warning':
		case 'caution':
			return CALLOUT_TONES[2];
		default:
			return CALLOUT_TONES[0];
	}
}
