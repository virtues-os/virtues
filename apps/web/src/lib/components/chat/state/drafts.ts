/**
 * drafts — the composer's text, kept across a closed tab, a reload, a quit.
 *
 * Each open tab already keeps its own composer; what was lost was the draft on
 * a closed tab, a reload, or a quit. The composer's document is a string, so it
 * is stored per conversation and removed the moment a send empties the input.
 * Ghost chats persist nothing, drafts included — the caller enforces that.
 */

const DRAFT_KEY = "virtues.draft.";

/**
 * A chat that has never been sent has no id anyone else will recognize: the
 * route is bare `/chat` and the view's id is minted per mount, so a reload
 * would mint another and orphan the draft. Unsent chats share this one key
 * until the first send moves the route to `/chat/<id>`.
 */
export const NEW_CHAT_DRAFT_ID = "new";

export function readDraft(id: string): string {
	try {
		return localStorage.getItem(DRAFT_KEY + id) ?? "";
	} catch {
		return "";
	}
}

export function writeDraft(id: string, text: string) {
	try {
		if (text.trim()) localStorage.setItem(DRAFT_KEY + id, text);
		else localStorage.removeItem(DRAFT_KEY + id);
	} catch {
		// Storage unavailable: a lost draft is the old behavior, not an error.
	}
}
