/**
 * Chat Sessions Store (Svelte 5 Runes)
 *
 * Manages loading and refreshing chat session data from the API.
 */

export interface ChatSession {
	conversation_id: string;
	title: string | null;
	icon: string | null;
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
			const response = await fetch('/api/chats');

			if (!response.ok) {
				throw new Error(`Failed to load sessions: ${response.statusText}`);
			}

			const data = await response.json();
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
	 * Clear all sessions
	 */
	clear() {
		this.sessions = [];
		this.error = null;
	}
}

// Export singleton instance
export const chatSessions = new ChatSessionStore();
