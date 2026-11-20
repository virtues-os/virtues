/**
 * toolSideEffects — the tool results that do something outside the transcript.
 *
 * `create_page` opens the new page beside the chat, an applet `setup` that has
 * a view opens the site beside it, and `edit_page` runs the AI presence
 * animation over an open page. They share one hazard: the transcript is
 * re-read on load, so every historical call would fire again. Each therefore
 * SEEDS on its first settled run — recording what was already there without
 * acting on it, and acts only on what arrives after.
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

/**
 * Whether an `edit_page` result's card shows the whole page as written: a
 * markdown edit with an empty `find`. A block page's edit names the blocks
 * it wrote (`blocks`), and its empty `find` means it only added blocks.
 */
export function isFullReplace(edit: { find?: string | null; replace?: string | null; blocks?: string[] | null }): boolean {
	return !edit.find && !edit.blocks;
}

/** A site the assistant just made or rebuilt, for the caller to open. */
export type BuiltSite = { appletId: string; name: string };

/**
 * The finished tool calls of one kind that a transcript has already been
 * asked about. The first ask records every call that is there and returns
 * nothing, because those were loaded from history. Later asks return what
 * `read` makes of each call that finished since.
 */
class SettledCalls {
	#seen: Set<string> | null = null;

	collect<T>(messages: Message[], read: (part: ToolResultPart) => T | null): T[] {
		const seeding = this.#seen === null;
		const seen = (this.#seen ??= new Set());
		const fresh: T[] = [];
		for (const message of messages) {
			if (message.role !== "assistant") continue;
			for (const part of message.parts as ToolResultPart[]) {
				if (part.state !== "output-available" || seen.has(part.toolCallId)) continue;
				const found = read(part);
				if (found === null) continue;
				seen.add(part.toolCallId);
				if (!seeding) fresh.push(found);
			}
		}
		return fresh;
	}
}

export class ToolSideEffects {
	#pages = new SettledCalls();
	#sites = new SettledCalls();

	#editSeeded = false;
	#animatedEditIds = new Set<string>();

	/** A tab switch starts a new transcript: forget what this one had settled. */
	reset() {
		this.#pages = new SettledCalls();
		this.#sites = new SettledCalls();
	}

	/**
	 * Pages created since the last call. Empty on the seeding run, which is what
	 * keeps reopening an old chat from reopening every page it ever made.
	 */
	collectNewPages(messages: Message[]): CreatedPage[] {
		return this.#pages.collect(messages, (part) => {
			if (part.type !== "tool-create_page") return null;
			const output = part.output;
			return output?.page_id ? { pageId: output.page_id, title: output.title ?? "" } : null;
		});
	}

	/**
	 * Sites the assistant made or rebuilt since the last call: an applet setup
	 * that succeeded and has a view. Seeded like pages, so an old chat opens
	 * nothing.
	 */
	collectNewSites(messages: Message[]): BuiltSite[] {
		return this.#sites.collect(messages, (part) => {
			const setup =
				part.type === "tool-setup_applet" ||
				(part.type === "tool-applet" && (part as { input?: { action?: string } }).input?.action === "setup");
			if (!setup) return null;
			const output = part.output as
				| { applet_id?: string; name?: string; has_view?: boolean; status?: string }
				| undefined;
			if (!output?.applet_id || !output.has_view) return null;
			if (output.status !== "created" && output.status !== "updated") return null;
			return { appletId: output.applet_id, name: output.name ?? "" };
		});
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
						animateChatEdit(edit.page_id, edit.blocks ?? []);
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
