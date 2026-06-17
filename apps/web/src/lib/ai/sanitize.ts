/**
 * Streaming output sanitizer for inline AI completions.
 *
 * Models sometimes wrap prose in a Markdown code fence (```` ```markdown … ``` ````)
 * despite being told not to. Since deltas stream straight into the document, we
 * strip a leading and trailing fence as the text flows:
 *  - Leading: buffer until we can see whether the output opens with a fence line,
 *    then drop that line.
 *  - Trailing: hold back a few trailing chars that could be the start of a closing
 *    fence, and drop a closing fence at flush().
 *
 * This is deliberately conservative — it only removes fences, never real prose, so
 * a false positive can't eat content. Preambles/quotes are left alone (too risky
 * to strip heuristically); the system prompt handles those.
 */

const LEAD_FENCE = /^\s*```[^\n]*\n/;
const TAIL_FENCE = /\n?```\s*$/;
// Trailing chars that might grow into a closing fence ("\n```") — hold them back.
const TAIL_HOLD = /\n?`{0,3}$/;

export function createOutputSanitizer() {
	let started = false; // leading edge resolved?
	let lead = ""; // pre-buffer while deciding the leading edge
	let pending = ""; // emittable buffer, minus a possibly-growing tail fence

	function emitSafe(): string {
		const m = pending.match(TAIL_HOLD);
		const hold = m ? m[0].length : 0;
		const emit = pending.slice(0, pending.length - hold);
		pending = pending.slice(pending.length - hold);
		return emit;
	}

	return {
		/** Feed a raw delta; returns the safe-to-insert text (possibly empty). */
		push(chunk: string): string {
			if (!started) {
				lead += chunk;
				// Wait until we have a full first line or enough to rule out a fence.
				if (!lead.includes("\n") && lead.length < 16) return "";
				const m = lead.match(LEAD_FENCE);
				pending = m ? lead.slice(m[0].length) : lead;
				lead = "";
				started = true;
			} else {
				pending += chunk;
			}
			return emitSafe();
		},

		/** Flush remaining buffer at end of stream, dropping a trailing fence. */
		flush(): string {
			if (!started) {
				// Short output that never hit a newline — resolve leading now.
				const m = lead.match(/^\s*```[^\n]*\n?/);
				pending = m && lead.includes("```") ? lead.slice(m[0].length) : lead;
				lead = "";
				started = true;
			}
			const out = pending.replace(TAIL_FENCE, "");
			pending = "";
			return out;
		},
	};
}
