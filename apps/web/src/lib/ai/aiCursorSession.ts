/**
 * Live AI cursor — session orchestrator.
 *
 * Streams an inline completion into the Yjs document so it reads as *writing*,
 * not a block-replace flash:
 *  - The moving insert point is a Y.RelativePosition, so it survives concurrent
 *    user/peer edits.
 *  - Every insert runs in `ydoc.transact(fn, 'ai')` → undo-isolated (the
 *    Y.UndoManager tracks the 'ai' origin) and synced to all peers.
 *  - Token deltas are coalesced on a ~30ms tick so we don't run a transaction
 *    per token (keeps the whole edit inside one undo group).
 *  - The caret/trail/telegraph are driven via the ai-cursor StateEffects.
 *
 * A REWRITE PROPOSES; it does not overwrite. Rather than deleting the selected
 * passage and streaming a replacement over it, the session wraps the original
 * and streams into a suggestion beside it:
 *
 *     {--the original passage--}{++the streamed rewrite++}
 *
 * which review-marks.ts renders as a before/after pair the writer accepts or
 * rejects (see that module for why the markers live in the text). The original
 * is never sent back through the model to be reproduced — it is wrapped in
 * place, so a model that paraphrases what it was asked to quote cannot quietly
 * corrupt the text it was rewriting.
 *
 * A CONTINUATION still inserts plainly. It adds prose where none existed, so
 * there is no before to compare against and nothing to reject but the whole
 * insertion, which is what undo already is.
 */

import * as Y from "yjs";
import { toast } from "svelte-sonner";
import type { EditorView } from "@codemirror/view";
import type { YjsDocument } from "$lib/yjs";
import { streamCompletion, type AiIntent } from "./inlineComplete";
import { createOutputSanitizer } from "./sanitize";
import { aiSession } from "./aiSession.svelte";
import { aiCaret, aiTrail, aiTelegraph, aiPresenceClear } from "./aiPresence";
import {
	getSelectedModel,
	getDefaultModel,
	getInitializationPromise,
} from "$lib/stores/models.svelte";

const CONTEXT_CHARS = 1200;
const FLUSH_MS = 30;
const TELEGRAPH_MS = 380;
const DONE_DISSOLVE_MS = 650;
// Client-side backstop on top of the server's max_tokens, so a runaway
// completion can never fill the document.
const MAX_OUTPUT_CHARS = 8000;

// The proposal wrapper, split at the pivot: everything before the pivot closes
// around the original, everything after opens the suggestion.
const PROPOSE_OPEN = "{--";
const PROPOSE_PIVOT = "--}{++";
const PROPOSE_CLOSE = "++}";

export interface StartAiOptions {
	view: EditorView;
	yjsDoc: YjsDocument;
	intent: AiIntent;
	instruction: string;
	/** The page's real title (for prompt context — NOT document.title). */
	pageTitle?: string;
}

let current: AiCursorSession | null = null;

export function isAiSessionActive(): boolean {
	return current !== null;
}

export function abortAiSession(): void {
	current?.abort();
}

/** Begin an AI session. Only one runs at a time; a new one supersedes. */
export async function startAiSession(opts: StartAiOptions): Promise<void> {
	current?.abort();
	const session = new AiCursorSession(opts);
	current = session;
	try {
		await session.run();
	} finally {
		if (current === session) current = null;
	}
}

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

class AiCursorSession {
	private view: EditorView;
	private ydoc: Y.Doc;
	private ytext: Y.Text;
	private intent: AiIntent;
	private instruction: string;
	private pageTitle?: string;

	private controller = new AbortController();
	private aborted = false;

	private anchor: Y.RelativePosition | null = null;
	private buffer = "";
	private flushScheduled = false;

	// Rewrite does nothing to the document until the first token arrives. Either
	// the selection gets wrapped as a proposal (the normal path) or — when the
	// passage itself contains the closing delimiter, and wrapping it would
	// produce markup that terminates in the wrong place — it is replaced
	// outright, the old destructive behavior, kept as the honest fallback.
	private pendingWrap: { from: number; to: number } | null = null;
	private pendingDeleteLen = 0;
	/** A `{++` is open in the document and still owes its `++}`. */
	private proposalOpen = false;
	/** The selection was wrapped as a proposal rather than replaced. */
	private didWrap = false;
	private isRewrite = false;
	private didMutate = false;
	private insertedChars = 0;
	private sanitizer = createOutputSanitizer();

	constructor(opts: StartAiOptions) {
		this.view = opts.view;
		this.ydoc = opts.yjsDoc.ydoc;
		this.ytext = opts.yjsDoc.ytext;
		this.intent = opts.intent;
		this.instruction = opts.instruction;
		this.pageTitle = opts.pageTitle;
	}

	abort(): void {
		if (this.aborted) return;
		this.aborted = true;
		this.controller.abort();
	}

	private resolve(rel: Y.RelativePosition): number {
		const abs = Y.createAbsolutePositionFromRelativePosition(rel, this.ydoc);
		return abs ? abs.index : this.ytext.length;
	}

	private dispatchCaret(pos: number, phase: "active" | "done" = "active") {
		aiCaret(this.view, pos, phase);
	}

	/** Insert text at the moving anchor, tagging origin 'ai' + adding a trail. */
	private insertAt(text: string) {
		if (!text || !this.anchor) return;

		const wrap = this.pendingWrap;
		const wasPending = wrap !== null || this.pendingDeleteLen > 0;
		let at = 0;

		// Everything the first token triggers happens in ONE transaction with the
		// token itself: the document goes straight from untouched to a complete,
		// well-formed proposal. A pre-stream error or abort therefore cannot
		// leave the writer with a half-open span or a deleted-and-empty passage.
		this.ydoc.transact(() => {
			if (wrap) {
				this.ytext.insert(wrap.from, PROPOSE_OPEN);
				this.ytext.insert(wrap.to + PROPOSE_OPEN.length, PROPOSE_PIVOT);
				this.pendingWrap = null;
				this.proposalOpen = true;
				this.didWrap = true;
				// The stream lands just inside the `{++`, after the original.
				at = wrap.to + PROPOSE_OPEN.length + PROPOSE_PIVOT.length;
			} else {
				at = this.resolve(this.anchor as Y.RelativePosition);
				if (this.pendingDeleteLen > 0) {
					this.ytext.delete(at, this.pendingDeleteLen);
					this.pendingDeleteLen = 0;
				}
			}
			this.ytext.insert(at, text);
		}, "ai");

		if (wasPending) {
			aiTelegraph(this.view, null);
		}
		this.didMutate = true;
		this.insertedChars += text.length;
		const end = at + text.length;
		this.anchor = Y.createRelativePositionFromTypeIndex(this.ytext, end);
		aiTrail(this.view, at, end);
		this.dispatchCaret(end);
		// Backstop against a runaway completion filling the document.
		if (this.insertedChars >= MAX_OUTPUT_CHARS) this.abort();
	}

	private flush() {
		this.flushScheduled = false;
		if (this.aborted || !this.buffer) return;
		const text = this.buffer;
		this.buffer = "";
		this.insertAt(text);
	}

	private scheduleFlush() {
		if (this.flushScheduled) return;
		this.flushScheduled = true;
		setTimeout(() => this.flush(), FLUSH_MS);
	}

	async run(): Promise<void> {
		// Models may not be loaded yet — the pages editor, unlike chat, never
		// triggers the fetch. Ensure they're loaded before resolving one, or the
		// first inline edit on a fresh session fails with "no model available".
		await getInitializationPromise();
		if (this.aborted) return this.cleanup();

		const model = getSelectedModel()?.id ?? getDefaultModel()?.id;
		if (!model) {
			aiSession.set("error", "No model available");
			return;
		}

		aiSession.set("thinking");

		const { state } = this.view;
		const docText = this.ytext.toString();
		const sel = state.selection.main;
		const isRewrite = this.intent === "rewrite" && sel.from !== sel.to;
		this.isRewrite = isRewrite;

		// Resolve the work region + context windows.
		let insertIndex: number;
		let selection: string | undefined;
		let before: string;
		let after: string;

		if (isRewrite) {
			selection = docText.slice(sel.from, sel.to);
			before = docText.slice(Math.max(0, sel.from - CONTEXT_CHARS), sel.from);
			after = docText.slice(sel.to, sel.to + CONTEXT_CHARS);

			// Telegraph: show what's about to change, glide the caret to it.
			// The highlight stays until the first replacement token arrives.
			aiSession.set("telegraphing");
			this.dispatchCaret(sel.from);
			aiTelegraph(this.view, { from: sel.from, to: sel.to });
			await sleep(TELEGRAPH_MS);
			if (this.aborted) return this.cleanup();

			// Wrapping is only safe while the passage cannot terminate the markup
			// early. A selection already containing `--}` (or an open `{++`) would
			// close the deletion span in the middle of itself, and the result would
			// be markup that parses somewhere the writer never chose. Rather than
			// silently mangle it, that case falls back to replacing outright.
			const wrappable = !selection.includes("--}") && !selection.includes("{++");
			if (wrappable) {
				this.pendingWrap = { from: sel.from, to: sel.to };
			} else {
				this.pendingDeleteLen = sel.to - sel.from;
			}
			insertIndex = sel.from;
		} else {
			insertIndex = sel.head;
			before = docText.slice(Math.max(0, sel.head - CONTEXT_CHARS), sel.head);
			after = docText.slice(sel.head, sel.head + CONTEXT_CHARS);
			this.dispatchCaret(insertIndex);
		}

		this.anchor = Y.createRelativePositionFromTypeIndex(this.ytext, insertIndex);

		try {
			aiSession.set("streaming");
			const pageTitle = this.pageTitle?.trim() || undefined;
			const stream = streamCompletion(
				{
					model,
					intent: this.intent,
					instruction: this.instruction,
					selection,
					context_before: before,
					context_after: after,
					page_title: pageTitle,
				},
				this.controller.signal,
			);

			for await (const chunk of stream) {
				if (this.aborted) break;
				const safe = this.sanitizer.push(chunk);
				if (safe) {
					this.buffer += safe;
					this.scheduleFlush();
				}
			}

			if (this.aborted) {
				// Land whatever arrived and close the span. An interrupted rewrite
				// leaves a real, if partial, suggestion the writer can reject in one
				// click — which is a better resting place than half-open markup.
				this.finalize();
				this.notifyInterrupted();
				this.cleanup();
				return;
			}

			// Drain the sanitizer's held tail (e.g. a stripped closing fence), flush.
			const tail = this.sanitizer.flush();
			if (tail) this.buffer += tail;
			this.finalize();

			// Hand-off: pulse the caret at its final spot, then dissolve.
			const finalPos = this.anchor ? this.resolve(this.anchor) : insertIndex;
			this.dispatchCaret(finalPos, "done");
			aiSession.set("done");
			await sleep(DONE_DISSOLVE_MS);
			this.cleanup();
		} catch (err) {
			// Either way the span must be closed before anything else happens —
			// unterminated `{++` is not a suggestion, it is literal braces in the
			// middle of the writer's prose.
			this.finalize();
			if (this.aborted || (err instanceof DOMException && err.name === "AbortError")) {
				this.notifyInterrupted();
				this.cleanup();
			} else {
				console.error("AI cursor session failed:", err);
				aiSession.set("error", err instanceof Error ? err.message : "AI error");
				if (this.isRewrite && this.didMutate) {
					toast.error("AI edit failed", { description: this.recoveryHint() });
				}
				this.cleanup(false);
			}
		}
	}

	/**
	 * Land any buffered text, then close an open proposal.
	 *
	 * Every exit from `run` goes through here — success, abort, and error alike
	 * — because the one state the document must never be left in is a `{++` with
	 * no `++}`. A no-op when nothing was ever written.
	 */
	private finalize() {
		if (this.buffer) {
			const text = this.buffer;
			this.buffer = "";
			this.insertAt(text);
		}
		if (!this.proposalOpen || !this.anchor) return;

		const at = this.resolve(this.anchor);
		this.ydoc.transact(() => {
			this.ytext.insert(at, PROPOSE_CLOSE);
		}, "ai");
		this.proposalOpen = false;
		this.anchor = Y.createRelativePositionFromTypeIndex(this.ytext, at + PROPOSE_CLOSE.length);
	}

	/** How to get rid of what the AI just did, phrased for what it actually did. */
	private recoveryHint(): string {
		return this.didWrap
			? "The suggestion is marked in the page — reject it to restore your text."
			: "Press ⌘Z to undo.";
	}

	/** A rewrite that began writing was interrupted — tell the user how to recover. */
	private notifyInterrupted() {
		if (this.isRewrite && this.didMutate) {
			toast("AI edit interrupted", { description: this.recoveryHint() });
		}
	}

	/** Remove the caret/trail/telegraph. `resetStatus` false keeps an error visible. */
	private cleanup(resetStatus = true) {
		aiPresenceClear(this.view);
		if (resetStatus) aiSession.reset();
	}
}
