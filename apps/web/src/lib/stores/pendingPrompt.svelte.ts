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
// Optional project to bind the next new chat to (e.g. "Ask this project").
let pendingProject: string | null = null;

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
	/** Stage a project binding for the next new chat. */
	setProject(id: string | null) {
		pendingProject = id;
	},
	/** Claim the staged project binding (once). */
	takeProject(): string | null {
		const t = pendingProject;
		pendingProject = null;
		return t;
	},
};

/**
 * Ask Virtues: stage `text` and open a new (kept) chat tab that will auto-send
 * it. Opens in a new tab so the caller's surface (e.g. Home) stays put. Pass
 * `projectId` to bind the new chat to a project (grounds retrieval there).
 */
export function askVirtues(text: string, projectId?: string | null) {
	const trimmed = text.trim();
	if (!trimmed) return;
	pendingPrompt.set(trimmed);
	pendingPrompt.setProject(projectId ?? null);
	windowShellStore.openTabFromRoute('/', { forceNew: true, label: 'New Chat' });
}
