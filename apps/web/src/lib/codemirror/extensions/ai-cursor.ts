/**
 * Live AI cursor — rendering layer.
 *
 * Two coordinated pieces:
 *  - The caret: a single overlay DOM node in `view.scrollDOM`, positioned via
 *    `coordsAtPos` and moved with a CSS transform transition so it GLIDES
 *    between positions (decoupled from CodeMirror's decoration diffing, which
 *    would tear down a per-position widget on every token).
 *  - The trail + telegraph: real CodeMirror decorations held in StateFields, so
 *    their positions are remapped through every document change (the user
 *    typing, remote peers, and the AI's own inserts) automatically.
 *
 * The session orchestrator (aiCursorSession.ts) drives this purely through the
 * exported StateEffects — it never touches the DOM.
 */

import { StateEffect, StateField } from '@codemirror/state';
import {
	Decoration,
	type DecorationSet,
	EditorView,
	ViewPlugin,
	type ViewUpdate,
} from '@codemirror/view';

export type AiCaretPhase = 'active' | 'done';
export interface AiCaretState {
	pos: number;
	phase: AiCaretPhase;
}

// ── Effects (the session's only API into this layer) ───────────────────────
export const setAiCaret = StateEffect.define<AiCaretState | null>();
export const addAiTrail = StateEffect.define<{ from: number; to: number }>();
export const setAiTelegraph = StateEffect.define<{ from: number; to: number } | null>();
export const clearAiSession = StateEffect.define<null>();

// ── Caret position state ────────────────────────────────────────────────────
const aiCaretField = StateField.define<AiCaretState | null>({
	create: () => null,
	update(value, tr) {
		if (value) {
			value = { ...value, pos: tr.changes.mapPos(value.pos, 1) };
		}
		for (const effect of tr.effects) {
			if (effect.is(setAiCaret)) value = effect.value;
			else if (effect.is(clearAiSession)) value = null;
		}
		return value;
	},
});

// ── Trail decorations (AI-authored ranges, fade via CSS) ─────────────────────
const trailMark = Decoration.mark({ class: 'cm-ai-trail' });

const aiTrailField = StateField.define<DecorationSet>({
	create: () => Decoration.none,
	update(deco, tr) {
		deco = deco.map(tr.changes);
		for (const effect of tr.effects) {
			if (effect.is(addAiTrail)) {
				if (effect.value.to > effect.value.from) {
					deco = deco.update({
						add: [trailMark.range(effect.value.from, effect.value.to)],
					});
				}
			} else if (effect.is(clearAiSession)) {
				deco = Decoration.none;
			}
		}
		return deco;
	},
	provide: (f) => EditorView.decorations.from(f),
});

// ── Telegraph decoration (the region the AI is about to change) ──────────────
const telegraphMark = Decoration.mark({ class: 'cm-ai-telegraph' });

const aiTelegraphField = StateField.define<DecorationSet>({
	create: () => Decoration.none,
	update(deco, tr) {
		deco = deco.map(tr.changes);
		for (const effect of tr.effects) {
			if (effect.is(setAiTelegraph)) {
				deco =
					effect.value && effect.value.to > effect.value.from
						? Decoration.set([telegraphMark.range(effect.value.from, effect.value.to)])
						: Decoration.none;
			} else if (effect.is(clearAiSession)) {
				deco = Decoration.none;
			}
		}
		return deco;
	},
	provide: (f) => EditorView.decorations.from(f),
});

// ── Caret overlay (glides via CSS transform) ─────────────────────────────────
const aiCaretOverlay = ViewPlugin.fromClass(
	class {
		dom: HTMLElement | null = null;

		constructor(view: EditorView) {
			this.render(view);
		}

		update(update: ViewUpdate) {
			const changed =
				update.startState.field(aiCaretField) !== update.state.field(aiCaretField);
			if (changed || update.docChanged || update.viewportChanged || update.geometryChanged) {
				this.render(update.view);
			}
		}

		render(view: EditorView) {
			const state = view.state.field(aiCaretField);
			if (!state) {
				this.remove();
				return;
			}
			const pos = Math.min(state.pos, view.state.doc.length);
			const coords = view.coordsAtPos(pos);
			if (!coords) {
				this.remove();
				return;
			}

			if (!this.dom) {
				this.dom = document.createElement('div');
				this.dom.className = 'cm-ai-caret';
				const bar = document.createElement('span');
				bar.className = 'cm-ai-caret-bar';
				const label = document.createElement('span');
				label.className = 'cm-ai-caret-label';
				label.textContent = 'Virtues';
				this.dom.append(bar, label);
				view.scrollDOM.appendChild(this.dom);
			}

			this.dom.classList.toggle('cm-ai-caret--done', state.phase === 'done');

			const rect = view.scrollDOM.getBoundingClientRect();
			const x = coords.left - rect.left + view.scrollDOM.scrollLeft;
			const y = coords.top - rect.top + view.scrollDOM.scrollTop;
			this.dom.style.transform = `translate(${x}px, ${y}px)`;
			this.dom.style.height = `${coords.bottom - coords.top}px`;
		}

		remove() {
			if (this.dom) {
				this.dom.remove();
				this.dom = null;
			}
		}

		destroy() {
			this.remove();
		}
	},
);

/** The complete AI-cursor extension (caret + trail + telegraph). */
export const aiCursor = [aiCaretField, aiTrailField, aiTelegraphField, aiCaretOverlay];
