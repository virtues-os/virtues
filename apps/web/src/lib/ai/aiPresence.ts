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
 *
 * A block page's editor (`lib/document`) draws its own presence
 * (`lib/document/presence.ts`); a chat edit to one flashes the blocks it wrote,
 * by id, in the editor `lib/document/registry.ts` holds for the page.
 */

import type { EditorView } from "@codemirror/view";
import type { Editor } from "@tiptap/core";
import contractJson from "$contract";
import { getTreeEditor } from "$lib/document/registry";
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

/** Wait before looking again for what a chat edit wrote, which may not have synced yet. */
const RETRY_MS = 150;

/** The attribute a block's id is kept in (`contract.json`). */
const ID_ATTR: string = (contractJson as { id: { attr: string } }).id.attr;

/** The ids in `ids` that a block of the editor's page carries. */
function blocksPresent(editor: Editor, ids: string[]): string[] {
	const want = new Set(ids);
	const found: string[] = [];
	editor.state.doc.descendants((node) => {
		const id = node.attrs[ID_ATTR];
		if (typeof id === "string" && want.has(id)) found.push(id);
		return found.length < want.size;
	});
	return found;
}

/**
 * Animate the AI presence over the region a chat edit just wrote.
 *
 * On a block page open in an editor, `blockIds` (the edit's `blocks`) flash
 * there. Otherwise `newText` is the tool's `replace` string: we locate it in
 * the *current* CodeMirror doc (the synced result already contains it) and
 * trail+caret that range. Either way, if the sync hasn't landed yet, we retry
 * once shortly after.
 */
export function animateChatEdit(pageId: string, newText: string, blockIds: string[] = []): void {
	if (!pageId) return;
	const tree = getTreeEditor(pageId);
	if (tree) {
		if (!blockIds.length) return;
		const flash = (attempt: number): void => {
			if (tree.isDestroyed) return;
			const present = blocksPresent(tree, blockIds);
			if (present.length < blockIds.length && attempt === 0) {
				setTimeout(() => flash(1), RETRY_MS);
				return;
			}
			// Loaded here, not at the top: the chat reaches this module, and
			// the block editor's code is already loaded when one is open.
			void import("$lib/document/presence").then(({ flashBlocks }) => {
				if (!tree.isDestroyed) flashBlocks(tree, present);
			});
		};
		flash(0);
		return;
	}

	if (!newText) return;
	const view = getPageEditor(pageId);
	if (!view) return; // page isn't open in a pane — nothing to animate

	const run = (attempt: number): void => {
		const at = view.state.doc.toString().indexOf(newText);
		if (at < 0) {
			// The Yjs change may not have reached CodeMirror yet; retry once.
			if (attempt === 0) setTimeout(() => run(1), RETRY_MS);
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
