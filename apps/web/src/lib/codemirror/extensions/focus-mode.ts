/**
 * Focus / Typewriter mode
 *
 * One toggle, two calming behaviors:
 *  - Focus: dim every line except the one the cursor is on, so the current
 *    thought stands out (extends the active-line idea used in live-preview.ts).
 *  - Typewriter: keep the caret line vertically centered as you write.
 *
 * Toggled live via a Compartment so it can be reconfigured without rebuilding
 * the editor. The extension is pure — the owning component flips it from the
 * pageDisplay store.
 */

import { Compartment, type Extension, type Range } from '@codemirror/state';
import { Decoration, type DecorationSet, EditorView, ViewPlugin, type ViewUpdate } from '@codemirror/view';

/** Reconfigurable slot the editor host toggles. */
export const focusModeCompartment = new Compartment();

const dimDecoration = Decoration.line({ attributes: { class: 'cm-focus-dim' } });

/** Dim all visible lines except the active one. */
const focusDimmer = ViewPlugin.fromClass(
	class {
		decorations: DecorationSet;

		constructor(view: EditorView) {
			this.decorations = this.build(view);
		}

		update(update: ViewUpdate) {
			if (update.docChanged || update.selectionSet || update.viewportChanged) {
				this.decorations = this.build(update.view);
			}
		}

		build(view: EditorView): DecorationSet {
			const { doc, selection } = view.state;
			const activeLine = doc.lineAt(selection.main.head).number;
			const ranges: Range<Decoration>[] = [];
			const startLine = doc.lineAt(view.viewport.from).number;
			const endLine = doc.lineAt(view.viewport.to).number;
			for (let n = startLine; n <= endLine; n++) {
				if (n === activeLine) continue;
				ranges.push(dimDecoration.range(doc.line(n).from));
			}
			return Decoration.set(ranges, true);
		}
	},
	{ decorations: (v) => v.decorations },
);

/** Keep the caret line vertically centered. */
const typewriterScroller = EditorView.updateListener.of((update) => {
	if (!update.selectionSet && !update.docChanged) return;
	const head = update.state.selection.main.head;
	// Dispatch async so we don't re-enter the update cycle; the scroll effect
	// carries no doc/selection change, so it won't loop.
	requestAnimationFrame(() => {
		if (update.view.dom.isConnected) {
			update.view.dispatch({ effects: EditorView.scrollIntoView(head, { y: 'center' }) });
		}
	});
});

/** Build the extension set for the given enabled state. */
export function focusMode(enabled: boolean): Extension {
	return enabled ? [focusDimmer, typewriterScroller] : [];
}
