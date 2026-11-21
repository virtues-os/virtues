/**
 * Essay layout for chats: replies in the article serif under the chat's
 * name, with figures, tables and page cards framed like the Labs essay
 * pages. Set in Settings · Appearance and from a chat's menu; both read
 * and write this one store.
 *
 * Kept per browser, like Glass and the reading font, since it is about the
 * screen in front of you.
 *
 * TODO(2026-11-20): a prototype the owner is trying against today's chat.
 * Make it the default or delete it, with this store, once the owner picks.
 */

const STORAGE_KEY = 'virtues-chat-essay';

function readSaved(): boolean {
	try {
		return localStorage.getItem(STORAGE_KEY) === '1';
	} catch {
		return false;
	}
}

class ChatEssay {
	on = $state(false);

	constructor() {
		if (typeof window === 'undefined') return;
		this.on = readSaved();
	}

	set(next: boolean) {
		this.on = next;
		try {
			if (next) localStorage.setItem(STORAGE_KEY, '1');
			else localStorage.removeItem(STORAGE_KEY);
		} catch {
			// No storage: the choice lasts until the page reloads.
		}
	}

	toggle() {
		this.set(!this.on);
	}
}

export const chatEssay = new ChatEssay();
