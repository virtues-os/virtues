/**
 * Pending Prompt — the "ask → real chat" bridge.
 *
 * Both the ⌘K command palette and the Home composer open a fresh chat that
 * auto-sends the typed prompt. Rather than encode the prompt in the URL (which
 * would leak it into history — a no-go for a privacy-first product), we hand it
 * off through this consume-once module singleton: the opener `set()`s it, the
 * freshly-mounted ChatView `take()`s it exactly once.
 */

import { windowShellStore } from './window-shell.svelte';

let pending: string | null = null;

export const pendingPrompt = {
	/** Stage a prompt for the next new chat to consume. */
	set(text: string) {
		pending = text;
	},
	/** Claim the staged prompt (once). Returns null if none is pending. */
	take(): string | null {
		const t = pending;
		pending = null;
		return t;
	},
};

/**
 * Ask Virtues: stage `text` and open a new (kept) chat tab that will auto-send
 * it. Opens in a new tab so the caller's surface (e.g. Home) stays put.
 */
export function askVirtues(text: string) {
	const trimmed = text.trim();
	if (!trimmed) return;
	pendingPrompt.set(trimmed);
	windowShellStore.openTabFromRoute('/', { forceNew: true, label: 'New Chat' });
}
