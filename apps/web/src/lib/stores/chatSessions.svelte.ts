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
	notebook_id?: string | null;
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
	 * Load sessions from the API
	 */
	async load() {
		this.isLoading = true;
		this.error = null;

		try {
			const data = await listChats<{ conversations?: ChatSession[] }>();
			this.sessions = data.conversations || [];
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
					notebook_id: null,
					last_updated: null,
					first_message_at: '',
					last_message_at: '',
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
export const chatSessions = new ChatSessionStore();
