/**
 * Which chats are working and which have a reply nobody has read — the
 * sidebar's spinner and its dot.
 *
 * Running is two sources, because a turn outlives the tab that started it
 * (live_turn.rs): the views this client has open report their own turns the
 * instant they start, and the box lists every turn it is still running,
 * including ones a closed tab or another device started. The box is asked on
 * focus and then every few seconds while it reports anything, never while it
 * reports nothing.
 *
 * Unread is the box's: the chat list says whether a reply landed after the
 * chat was last on screen, on any device. Opening a chat, or watching a
 * reply finish in it, marks it seen there.
 */

import { listLiveChats, markChatSeen } from '$lib/api/client';
import { chatSessions } from './chatSessions.svelte';

const POLL_MS = 3000;

class ChatActivityStore {
	/** Turns the box is running, by chat id. */
	private boxRunning = $state<string[]>([]);
	/** Turns this client's own views are running. */
	private localRunning = $state<string[]>([]);
	private timer: ReturnType<typeof setTimeout> | null = null;
	/** Focus and visibility fire together; one ask at a time keeps one loop. */
	private asking = false;

	running(chatId: string): boolean {
		return this.localRunning.includes(chatId) || this.boxRunning.includes(chatId);
	}

	/** A finished reply nobody has seen. A chat still running shows the spinner instead. */
	unread(chatId: string): boolean {
		if (this.running(chatId)) return false;
		return chatSessions.sessions.find((s) => s.conversation_id === chatId)?.unread ?? false;
	}

	/** A view's own turn started or ended. An end refreshes the list, which carries unread. */
	setLocalRunning(chatId: string, running: boolean) {
		const has = this.localRunning.includes(chatId);
		if (running && !has) {
			this.localRunning = [...this.localRunning, chatId];
		} else if (!running && has) {
			this.localRunning = this.localRunning.filter((id) => id !== chatId);
			void this.poll();
			void chatSessions.refresh();
		}
	}

	/** The person has this chat on screen now. */
	markSeen(chatId: string) {
		chatSessions.markRead(chatId);
		// The seen mark is a courtesy: a failure leaves a dot, never an error.
		markChatSeen(chatId).catch(() => {});
	}

	/** Ask the box what it is running; keep asking while the answer is not empty. */
	async poll() {
		if (this.asking) return;
		if (this.timer) {
			clearTimeout(this.timer);
			this.timer = null;
		}
		this.asking = true;
		let running: string[];
		try {
			running = (await listLiveChats()).running ?? [];
		} catch {
			return;
		} finally {
			this.asking = false;
		}
		// A turn the box finished: its reply is in the list now, and so is its dot.
		if (this.boxRunning.some((id) => !running.includes(id))) void chatSessions.refresh();
		if (running.length !== this.boxRunning.length || running.some((id) => !this.boxRunning.includes(id))) {
			this.boxRunning = running;
		}
		if (running.length > 0 && typeof document !== 'undefined' && !document.hidden) {
			this.timer = setTimeout(() => void this.poll(), POLL_MS);
		}
	}
}

export const chatActivity = new ChatActivityStore();

if (typeof window !== 'undefined') {
	window.addEventListener('focus', () => void chatActivity.poll());
	document.addEventListener('visibilitychange', () => {
		if (!document.hidden) void chatActivity.poll();
	});
	queueMicrotask(() => void chatActivity.poll());
}
