/**
 * AI presence — the single driver for the "colored collaborator" animation.
 *
 * The ai-cursor extension (caret + trail + telegraph) is rendered from a set of
 * CodeMirror StateEffects. This module is the ONE place those effects are
 * dispatched, so every AI edit path drives the same animation and nothing is
 * duplicated or forked:
 *   - the inline Cmd+J session (which holds the EditorView) calls these directly;
 *   - a chat-driven `edit_page` (which only knows a pageId) resolves the view via
 *     the registry below, then drives the exact same caret/trail.
 *
 * Keeping this seam narrow means the presence look-and-feel is defined once, in
 * the extension, and both callers stay dumb about the DOM.
 */

import type { EditorView } from "@codemirror/view";
import {
	addAiTrail,
	clearAiSession,
	setAiCaret,
	setAiTelegraph,
	type AiCaretPhase,
} from "$lib/codemirror/extensions/ai-cursor";

// ── Editor registry ─────────────────────────────────────────────────────────
// Live page editors, keyed by pageId. An AI edit that only knows a pageId (the
// chat `edit_page` tool) uses this to reach the right EditorView.
const editors = new Map<string, EditorView>();

export function registerPageEditor(pageId: string, view: EditorView): void {
	if (pageId) editors.set(pageId, view);
}

export function unregisterPageEditor(pageId: string, view: EditorView): void {
	// Only clear if we still own the slot (guards against a late unmount racing
	// a remount that already re-registered).
	if (pageId && editors.get(pageId) === view) editors.delete(pageId);
}

export function getPageEditor(pageId: string): EditorView | undefined {
	return editors.get(pageId);
}

// ── Presence effects (view-based) ────────────────────────────────────────────
export function aiCaret(view: EditorView, pos: number, phase: AiCaretPhase = "active"): void {
	view.dispatch({ effects: setAiCaret.of({ pos, phase }) });
}

export function aiTrail(view: EditorView, from: number, to: number): void {
	if (to > from) view.dispatch({ effects: addAiTrail.of({ from, to }) });
}

export function aiTelegraph(view: EditorView, range: { from: number; to: number } | null): void {
	view.dispatch({ effects: setAiTelegraph.of(range) });
}

export function aiPresenceClear(view: EditorView): void {
	view.dispatch({ effects: clearAiSession.of(null) });
}

// ── Chat-driven edit choreography ────────────────────────────────────────────
// A chat `edit_page` applies server-side and syncs into the bound editor as a
// remote Yjs change (no 'ai' origin locally), so the inline session's effects
// don't fire. This replays the same trail + caret hand-off over the newly
// written text, driving the identical presence animation from the chat side.
const DONE_DISSOLVE_MS = 650;
const DWELL_MS = 450;

/**
 * Animate the AI presence over the region a chat edit just wrote.
 *
 * `newText` is the tool's `replace` string. We locate it in the *current*
 * editor doc (the synced result already contains it) and trail+caret that
 * range. If the sync hasn't landed yet, we retry once shortly after.
 */
export function animateChatEdit(pageId: string, newText: string): void {
	if (!pageId || !newText) return;
	const view = getPageEditor(pageId);
	if (!view) return; // page isn't open in a pane — nothing to animate

	const run = (attempt: number): void => {
		const at = view.state.doc.toString().indexOf(newText);
		if (at < 0) {
			// The Yjs change may not have reached CodeMirror yet; retry once.
			if (attempt === 0) setTimeout(() => run(1), 150);
			return;
		}
		const to = at + newText.length;
		aiTrail(view, at, to);
		aiCaret(view, to, "active");
		setTimeout(() => {
			aiCaret(view, to, "done");
			setTimeout(() => aiPresenceClear(view), DONE_DISSOLVE_MS);
		}, DWELL_MS);
	};

	run(0);
}
