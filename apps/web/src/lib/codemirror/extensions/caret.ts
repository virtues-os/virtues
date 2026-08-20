/**
 * The caret.
 *
 * CodeMirror's default is the browser's native caret: one hairline pixel, a
 * square 1.2s blink, and no memory of where it just was. We replace it with
 * `drawSelection()`'s rendered caret so it becomes an element we own — then
 * give it the three properties that make a caret feel like a place rather than
 * a marker:
 *
 *  1. PRESENCE. The bar is sized off the FONT SIZE at the caret, not off the
 *     line box — so it grows with the type (visibly taller and thicker inside
 *     an H1) without inheriting our generous 1.7 prose line-height, which
 *     would make it tower over body copy. A caret the same weight at every
 *     size reads as system chrome; one that tracks the type reads as part of
 *     the page.
 *
 *  2. CONTINUITY. It glides between positions (105ms) instead of teleporting,
 *     so arrow-key motion is legible as motion. The easing is front-loaded —
 *     it leaves fast and settles — because a symmetric curve at this duration
 *     reads as lag, not travel.
 *
 *  3. RESTRAINT — the rule that makes the other two survive contact:
 *     - The glide is KEYBOARD-ONLY. A click is a teleport, always. Animating
 *       toward the place someone just pointed at is the single fastest way to
 *       make an editor feel slow, and it's the detail most imitations miss.
 *     - Solid while you work, blinking only after 500ms of stillness. A caret
 *       that blinks under your own typing is noise.
 *     - The blink is a soft asymmetric fade (long on, quick dip) rather than a
 *       hard on/off, so peripheral vision reads it as breathing, not flicker.
 *
 * Note this is the same trick `ai-cursor.ts` already plays for the ASSISTANT's
 * caret — a positioned bar with an eased transition. This module finally gives
 * the human writing the page the same treatment.
 *
 * Enabling `drawSelection()` also revives `.cm-selectionBackground` /
 * `.cm-selectionLayer` in `theme.ts`, which until now styled elements that were
 * never constructed (the local selection was falling through to the global
 * `::selection` rule in `app.css`). The rendered selection matches it, so the
 * visible result is unchanged — the rules are simply no longer dead.
 */

import type { Extension } from '@codemirror/state';
import { drawSelection, EditorView, ViewPlugin, type ViewUpdate } from '@codemirror/view';

/** How long the caret stays solid after the last keystroke or move. */
const BLINK_DELAY_MS = 500;

/**
 * Bar height as a multiple of the FONT SIZE at the caret — roughly ascender to
 * descender plus a little. Deliberately not a multiple of the line box: this
 * editor runs a 1.7 line-height for prose, and a caret scaled to that reads as
 * a cursor from a different, larger document.
 */
const CARET_HEIGHT_RATIO = 1.25;

/**
 * Width also follows the caret's own height, so the bar keeps its proportions
 * from body copy up to an H1 instead of staying a hairline at display sizes.
 */
function caretWidthFor(caretHeight: number): number {
	return caretHeight * 0.055 + 1.1;
}

/**
 * Records what moved the caret, so the CSS can suppress the glide for pointer
 * input. `keydown` fires before the selection it causes, and `mousedown`
 * before the click's selection, so the attribute is always correct by the time
 * the transition would run.
 */
const caretInputSource = EditorView.domEventHandlers({
	mousedown(_event, view) {
		view.dom.dataset.caretInput = 'pointer';
		return false;
	},
	touchstart(_event, view) {
		view.dom.dataset.caretInput = 'pointer';
		return false;
	},
	keydown(_event, view) {
		view.dom.dataset.caretInput = 'keyboard';
		return false;
	},
});

/**
 * Drives the two things CSS can't compute on its own: the size-relative bar
 * width, and the solid → blinking handoff.
 */
const caretBehavior = ViewPlugin.fromClass(
	class {
		private layer: HTMLElement | null = null;
		private blinkTimer: ReturnType<typeof setTimeout> | null = null;

		constructor(private readonly view: EditorView) {
			this.refresh();
		}

		update(update: ViewUpdate) {
			// Geometry changes matter as much as selection ones: a widget
			// resizing above the caret moves it without any selection event.
			if (
				update.selectionSet ||
				update.docChanged ||
				update.geometryChanged ||
				update.focusChanged
			) {
				this.refresh();
			}
		}

		destroy() {
			this.clearTimer();
		}

		private clearTimer() {
			if (this.blinkTimer !== null) {
				clearTimeout(this.blinkTimer);
				this.blinkTimer = null;
			}
		}

		/** `.cm-cursorLayer` is created by drawSelection, after this plugin. */
		private cursorLayer(): HTMLElement | null {
			if (!this.layer?.isConnected) {
				this.layer = this.view.dom.querySelector('.cm-cursorLayer');
			}
			return this.layer;
		}

		private refresh() {
			this.measureCaret();
			this.restartBlink();
		}

		/**
		 * Size the bar against the type at the caret.
		 *
		 * CodeMirror sets the cursor's `height` inline, to the LINE BOX — which
		 * carries our 1.7 prose leading and is far taller than the glyphs. We
		 * leave that inline height alone (overriding it would fight the
		 * measured geometry) and correct it with a scaleY computed from the
		 * font size, so the painted bar hugs the text while the element the
		 * browser positions stays exactly what CodeMirror measured.
		 *
		 * `transform-origin: center` then keeps the bar centered on the line
		 * box, which is where the text sits.
		 *
		 * Read in a measure phase: the cursor is positioned by drawSelection in
		 * this same update, and reading layout inline forces a reflow.
		 */
		private measureCaret() {
			this.view.requestMeasure({
				read: (view) => {
					const cursor = view.dom.querySelector<HTMLElement>('.cm-cursor-primary');
					if (!cursor) return null;

					// The element at the caret, so headings and code spans each
					// report their own size rather than the editor's base.
					const { node } = view.domAtPos(view.state.selection.main.head);
					const element = node.nodeType === Node.TEXT_NODE ? node.parentElement : (node as HTMLElement);
					const fontSize = element ? parseFloat(getComputedStyle(element).fontSize) : 0;

					return { lineBox: cursor.offsetHeight, fontSize };
				},
				write: (measurement, view) => {
					if (!measurement) return;
					const { lineBox, fontSize } = measurement;
					if (!lineBox || !fontSize) return;

					const height = fontSize * CARET_HEIGHT_RATIO;
					view.dom.style.setProperty('--cm-caret-scale-y', (height / lineBox).toFixed(3));
					view.dom.style.setProperty(
						'--cm-caret-width',
						`${caretWidthFor(height).toFixed(2)}px`
					);
				},
			});
		}

		/**
		 * Solid now, blinking once the writer stops. Toggling the class is what
		 * restarts the CSS animation, so every move resets the phase and the
		 * caret is never caught mid-fade at the moment it arrives somewhere.
		 */
		private restartBlink() {
			const layer = this.cursorLayer();
			if (!layer) return;

			this.clearTimer();
			layer.classList.remove('cm-caret-blinking');

			// Nothing to blink: no focus, or the caret is one end of a range and
			// the selection highlight is carrying the signal instead.
			if (!this.view.hasFocus || !this.view.state.selection.main.empty) return;

			this.blinkTimer = setTimeout(() => {
				this.blinkTimer = null;
				if (this.view.hasFocus && this.view.state.selection.main.empty) {
					layer.classList.add('cm-caret-blinking');
				}
			}, BLINK_DELAY_MS);
		}
	}
);

/**
 * The rendered caret. `cursorBlinkRate: 0` disables CodeMirror's own blink so
 * the timing above is the only one running — otherwise two animations fight
 * over the same element's opacity.
 */
export const smoothCaret: Extension = [
	drawSelection({ cursorBlinkRate: 0 }),
	caretInputSource,
	caretBehavior,
];
