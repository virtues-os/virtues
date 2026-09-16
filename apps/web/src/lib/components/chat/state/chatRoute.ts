/**
 * chatRoute — everything a chat tab's route tells the view.
 *
 * A ChatView is addressed entirely by its tab route: which conversation it
 * holds (or that it holds none yet), whether it is showing the context panel
 * instead of the transcript, and whether it is a temporary ("ghost") chat.
 * Pure string work with no state, so the view can read it synchronously at
 * init — which is what lets the composer start in the right position instead
 * of painting docked for a frame and jumping.
 */

/** Generate a random 16-char hex ID (matches backend format) */
export function generateHex16(): string {
	const bytes = new Uint8Array(8);
	crypto.getRandomValues(bytes);
	return Array.from(bytes)
		.map((b) => b.toString(16).padStart(2, "0"))
		.join("");
}

// Extract conversationId from tab route (format: /chat/chat_abc123 or / for new chat)
// Returns the full chat ID including 'chat_' prefix, or undefined for new chat
// Strips query params like ?view=context
export function extractConversationId(route: string): string | undefined {
	// Strip query params first
	const pathOnly = route.split("?")[0];
	if (pathOnly === "/" || pathOnly === "/chat") return undefined;
	// Route format: /chat/chat_abc123 → extract chat_abc123 (full ID)
	const match = pathOnly.match(/^\/chat\/(chat_[^/]+)$/);
	return match?.[1];
}

/** Check if route has ?view=context query param */
export function isContextViewRoute(route: string): boolean {
	return route.includes("?view=context");
}

/** Check if route represents a new/unsaved chat */
export function isNewChat(route: string): boolean {
	const pathOnly = route.split("?")[0];
	return pathOnly === "/" || pathOnly === "/chat";
}

// Temporary ("ghost") chat — opened via /?temporary=1. Never persisted to
// history; nothing is written to the sidebar/session list. The request also
// carries a `temporary` flag so the backend can skip storage.
export function isTemporaryRoute(route: string): boolean {
	return /[?&]temporary=1\b/.test(route);
}
