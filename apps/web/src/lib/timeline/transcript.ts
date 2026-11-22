/**
 * transcript.ts - a recorded conversation's words as speaker turns, never
 * identities: the recorder tags a speaker and does not say who it is.
 */

/** A chunk's turns, split at every "[Speaker]:" or "[Speaker 2]:" tag, whether
 *  the tags start lines or run inline (most chunks are one line). Speakers are
 *  unnamed, so the tags themselves are dropped. */
export function speakerTurns(text: string | null): string[] {
	return (text ?? "")
		.split(/\s*\[Speaker(?: \d+)?\]:\s*/)
		.map((t) => t.trim())
		.filter(Boolean);
}
