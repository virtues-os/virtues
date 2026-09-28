/**
 * Chat Sessions Store (Svelte 5 Runes)
 *
 * Manages loading and refreshing chat session data from the API.
 */

import { listChats } from '$lib/api/client';

export interface ChatSession {
	conversation_id: string;
	title: string | null;
	icon: string | null;
	/** `--cat-*` token key, never a hex. Migration 0079. */
	icon_color?: string | null;
	project_id?: string | null;
	last_updated: string | null;
	first_message_at: string;
	last_message_at: string;
	message_count: number;
	model_used: string | null;
	provider: string;
}

class ChatSessionStore {
	sessions = $state<ChatSession[]>([]);
	isLoading = $state(false);
	error = $state<string | null>(null);

	/**
	 * A counter, not a boolean: the sidebar's "Search chats" door bumps it
	 * and the All chats page focuses its search field when it changes. A
	 * boolean would need clearing, and a request that arrives while the
	 * page is still mounting would be cleared before it was honored.
	 */
	searchFocusToken = $state(0);

	requestSearchFocus() {
		this.searchFocusToken += 1;
	}

	/**
	 * Projects of chats the list does not hold. The list is the box's most
	 * recent chats only, so an older chat opened from a project would read as
	 * unfiled; its own detail (and every filing this client does) lands here.
	 */
	private knownProjects = $state<Record<string, string | null>>({});

	/** The project a chat is filed in: its row when listed, else what this client learned. */
	projectOf(chatId: string): string | null {
		const row = this.sessions.find((s) => s.conversation_id === chatId);
		if (row) return row.project_id ?? null;
		return this.knownProjects[chatId] ?? null;
	}

	/**
	 * Record where a chat now lives, at once, on its row and off it. Every
	 * surface that shows a chat's project reads it through `projectOf`, so a
	 * filing shows the moment the server says yes, not a round trip later.
	 */
	noteProject(chatId: string, projectId: string | null) {
		if ((this.knownProjects[chatId] ?? null) !== projectId) {
			this.knownProjects = { ...this.knownProjects, [chatId]: projectId };
		}
		if (this.sessions.some((s) => s.conversation_id === chatId && (s.project_id ?? null) !== projectId)) {
			this.sessions = this.sessions.map((s) =>
				s.conversation_id === chatId ? { ...s, project_id: projectId } : s,
			);
		}
	}

	/**
	 * Load sessions from the API
	 */
	async load() {
		this.isLoading = true;
		this.error = null;

		try {
			const data = await listChats<{ conversations?: ChatSession[] }>();
			// The getting-started chat is not a conversation anyone started:
			// the server seeds it, and since 2026-09-24 it only carries the
			// interview behind Setup's one-question pages. Listed, it showed
			// up as "Getting started — chat" in every recents list.
			this.sessions = (data.conversations || []).filter((c) => c.conversation_id !== HIDDEN_CHAT);
		} catch (err) {
			console.error('Error loading chat sessions:', err);
			this.error = err instanceof Error ? err.message : 'Failed to load sessions';
			this.sessions = [];
		} finally {
			this.isLoading = false;
		}
	}

	/**
	 * Refresh sessions (alias for load)
	 */
	async refresh() {
		await this.load();
	}

	/**
	 * Update a chat's icon locally (after API call succeeds)
	 */
	updateSessionIcon(chatId: string, icon: string | null) {
		this.sessions = this.sessions.map(s =>
			s.conversation_id === chatId ? { ...s, icon } : s
		);
	}

	/**
	 * Same, for the icon's color. Separate from `updateSessionIcon` because
	 * the picker sets them independently — a color can change without an icon
	 * changing, and folding them into one call would make each overwrite the
	 * other's untouched half.
	 */
	updateSessionIconColor(chatId: string, icon_color: string | null) {
		this.sessions = this.sessions.map(s =>
			s.conversation_id === chatId ? { ...s, icon_color } : s
		);
	}

	/**
	 * Apply a title locally (optimistic) so every surface bound to this store
	 * updates immediately, independent of the server-persist / refetch race.
	 * Upserts a stub row if the brand-new chat isn't in the list yet.
	 */
	applyTitle(chatId: string, title: string) {
		const existing = this.sessions.find(s => s.conversation_id === chatId);
		if (existing) {
			this.sessions = this.sessions.map(s =>
				s.conversation_id === chatId ? { ...s, title } : s
			);
		} else {
			this.sessions = [
				{
					conversation_id: chatId,
					title,
					icon: null,
					project_id: null,
					last_updated: null,
					first_message_at: '',
					// A chat is only titled once it has been talked in.
					last_message_at: new Date().toISOString(),
					message_count: 0,
					model_used: null,
					provider: '',
				},
				...this.sessions,
			];
		}
	}

	/**
	 * Remove a chat locally (optimistic) after a successful delete.
	 */
	remove(chatId: string) {
		this.sessions = this.sessions.filter(s => s.conversation_id !== chatId);
	}

	/**
	 * Clear all sessions
	 */
	clear() {
		this.sessions = [];
		this.error = null;
	}
}

// Export singleton instance
/** Mirrors getting_started::GETTING_STARTED_CHAT_ID on the server. */
const HIDDEN_CHAT = 'chat_getting_started';

export const chatSessions = new ChatSessionStore();
