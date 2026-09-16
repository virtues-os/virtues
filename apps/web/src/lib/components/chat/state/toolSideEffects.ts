/**
 * toolSideEffects — the two tool results that do something outside the transcript.
 *
 * `create_page` opens the new page beside the chat; `edit_page` runs the AI
 * presence animation over an open one. Both share one hazard: the transcript is
 * re-read on load, so every historical call would fire again. Both therefore
 * SEED on their first settled run — recording what was already there without
 * acting on it — and only act on what arrives after.
 *
 * Tool parts land in the messages array mutably, so the caller drives these
 * from an `$effect` over `chat.messages`; this module holds the seen-sets and
 * the seeding rule. (The interview's write_it_up auto-open is NOT here — it
 * comes through chatInstances.onData as a transient data-narrative-document
 * part, because no effect observes a mutable part landing.)
 */

import { animateChatEdit } from "$lib/ai/aiPresence";
import type { ToolResultPart } from "./transcript";

type Message = { role: string; parts: unknown[] };

/** A page the model just created, for the caller to bind and open. */
export type CreatedPage = { pageId: string; title: string };

export class ToolSideEffects {
	// Track tool calls that were already complete when we mounted (loaded from history)
	// Only auto-open pages created AFTER mount (during streaming)
	#seenCreates: Set<string> | null = null;
	#createsSeeded = false;

	#editSeeded = false;
	#animatedEditIds = new Set<string>();

	/** A tab switch starts a new transcript: forget what this one had settled. */
	reset() {
		this.#seenCreates = null;
		this.#createsSeeded = false;
	}

	/**
	 * Pages created since the last call. Empty on the seeding run, which is what
	 * keeps reopening an old chat from reopening every page it ever made.
	 */
	collectNewPages(messages: Message[]): CreatedPage[] {
		// First run after load: capture already-completed tool calls (loaded from history)
		if (this.#seenCreates === null) {
			this.#seenCreates = new Set();
			for (const message of messages) {
				if (message.role !== "assistant") continue;
				for (const part of message.parts as ToolResultPart[]) {
					if (part.type === "tool-create_page" && part.state === "output-available") {
						this.#seenCreates.add(part.toolCallId);
					}
				}
			}
			this.#createsSeeded = true;
			return []; // Don't auto-open on first run
		}

		// Only process new pages after initial load is complete
		if (!this.#createsSeeded) return [];

		// Subsequent runs: only auto-open for NEW completions (not loaded from history)
		const created: CreatedPage[] = [];
		for (const message of messages) {
			if (message.role !== "assistant") continue;
			for (const part of message.parts as ToolResultPart[]) {
				if (part.type === "tool-create_page" && part.state === "output-available") {
					const output = part.output;
					if (output?.page_id && !this.#seenCreates.has(part.toolCallId)) {
						created.push({ pageId: output.page_id, title: output.title ?? "" });
						this.#seenCreates.add(part.toolCallId); // Mark as handled
					}
				}
			}
		}
		return created;
	}

	/**
	 * Drive the AI presence animation when a chat `edit_page` lands. Mirrors the
	 * create_page path: seed historical edits on the first settled run (so we
	 * don't replay them), then animate only new ones, deduped by edit_id. The
	 * animation is a no-op if the page isn't open in a pane.
	 */
	animateNewEdits(messages: Message[]) {
		const collectNew = (animate: boolean) => {
			for (const message of messages) {
				if (message.role !== "assistant") continue;
				for (const part of message.parts as ToolResultPart[]) {
					if (part.type !== "tool-edit_page" || part.state !== "output-available") continue;
					const output = part.output as any;
					const edit = output?.edit;
					if (!edit?.edit_id || this.#animatedEditIds.has(edit.edit_id)) continue;
					this.#animatedEditIds.add(edit.edit_id);
					if (animate && output?.applied) {
						animateChatEdit(edit.page_id, edit.replace || "");
					}
				}
			}
		};

		// First settled run: seed history without animating.
		if (!this.#editSeeded) {
			collectNew(false);
			this.#editSeeded = true;
			return;
		}
		collectNew(true);
	}
}
