/**
 * AI Edit Events
 *
 * Custom events for coordinating AI edit highlights between components.
 * ChatView dispatches these events, PageEditor listens for them.
 */

// =============================================================================
// EVENT TYPES
// =============================================================================

export interface AIEditHighlightEvent {
	pageId: string;
	editId: string;
	text: string; // The replacement text to find and highlight
}

export interface AIEditAcceptEvent {
	pageId: string;
	editId: string;
}

export interface AIEditRejectEvent {
	pageId: string;
	editId: string;
}

// =============================================================================
// EVENT DISPATCHERS
// =============================================================================

/** Dispatch when an AI edit should be highlighted in the editor */
export function dispatchAIEditHighlight(detail: AIEditHighlightEvent) {
	window.dispatchEvent(new CustomEvent('ai-edit-highlight', { detail }));
}

/** Dispatch when an edit is accepted (remove highlight, keep content) */
export function dispatchAIEditAccept(detail: AIEditAcceptEvent) {
	window.dispatchEvent(new CustomEvent('ai-edit-accept', { detail }));
}

/** Dispatch when an edit is rejected (remove highlight, content already reverted via API) */
export function dispatchAIEditReject(detail: AIEditRejectEvent) {
	window.dispatchEvent(new CustomEvent('ai-edit-reject', { detail }));
}

// =============================================================================
// TYPE GUARDS
// =============================================================================

export function isAIEditHighlightEvent(e: Event): e is CustomEvent<AIEditHighlightEvent> {
	return e.type === 'ai-edit-highlight';
}

export function isAIEditAcceptEvent(e: Event): e is CustomEvent<AIEditAcceptEvent> {
	return e.type === 'ai-edit-accept';
}

export function isAIEditRejectEvent(e: Event): e is CustomEvent<AIEditRejectEvent> {
	return e.type === 'ai-edit-reject';
}
