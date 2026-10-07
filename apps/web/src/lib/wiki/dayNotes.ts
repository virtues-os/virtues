/**
 * Where a day page's notes sit.
 *
 * You write a note beside a sentence; the note keeps that sentence's words as
 * its anchor (`{quote, sentence}`), not a position, because the page can be
 * rewritten under it. On the next read the note goes back beside the sentence
 * that still says those words. A note whose words are gone, or that was never
 * written on a sentence, sits beside the Abstract, with the words it was about
 * shown so it still makes sense.
 */

import type { ArticleBlock } from "./dayArticle";
import { tokenize, type Token } from "./inlineMarkdown";

export interface NoteAnchor {
	/** The sentence as plain text, the way `plainSentence` writes it. */
	quote: string;
	/** Its index within its paragraph, when it was written on one sentence. */
	sentence?: number;
}

export interface Placed<N> {
	note: N;
	/** The paragraph block and sentence the note sits beside; null beside the Abstract. */
	at: { block: number; sentence: number } | null;
	/** The words it was about, when it couldn't be placed beside them. */
	lost: string | null;
}

const flat = (ts: Token[]): string => ts.map((t) => ("children" in t ? flat(t.children) : t.text)).join("");

/** A sentence's words with its markup gone, read by the same rules the page draws with. */
export function plainSentence(markdown: string): string {
	return flat(tokenize(markdown))
		.replace(/[⟦⟧]/g, "")
		.replace(/\s+/g, " ")
		.trim();
}

export function placeNotes<N extends { anchor?: NoteAnchor | null }>(blocks: ArticleBlock[], notes: N[]): Placed<N>[] {
	const plain = blocks.map((b) => b.sentences.map((s) => plainSentence(s.markdown)));
	return notes.map((note) => {
		const quote = note.anchor?.quote?.trim();
		if (!quote) return { note, at: null, lost: null };
		const want = plainSentence(quote);
		// The same words at the same index first (a sentence can repeat), then anywhere.
		const hint = note.anchor?.sentence;
		for (let b = 0; b < plain.length; b++) {
			if (hint != null && plain[b][hint] === want) return { note, at: { block: b, sentence: hint }, lost: null };
		}
		for (let b = 0; b < plain.length; b++) {
			const s = plain[b].indexOf(want);
			if (s >= 0) return { note, at: { block: b, sentence: s }, lost: null };
		}
		return { note, at: null, lost: want };
	});
}
