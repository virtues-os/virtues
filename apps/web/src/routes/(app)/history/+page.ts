import type { PageLoad } from './$types';

interface ChatSession {
	conversation_id: string;
	title: string | null;
	last_updated: string | null;
	first_message_at: string;
	last_message_at: string;
	message_count: number;
	model_used: string | null;
	provider: string;
}

export const load: PageLoad = async ({ fetch }) => {
	try {
		const response = await fetch('/api/sessions');

		if (!response.ok) {
			throw new Error(`Failed to load sessions: ${response.statusText}`);
		}

		const data = await response.json();
		return {
			sessions: (data.conversations || []) as ChatSession[]
		};
	} catch (err) {
		console.error('Failed to load chat history:', err);
		return {
			sessions: [],
			error: err instanceof Error ? err.message : 'Failed to load chat history'
		};
	}
};

