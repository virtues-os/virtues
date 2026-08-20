/**
 * The block-widget height contract.
 *
 * This module is the rule, written once, in the place that enforces it.
 *
 * CodeMirror measures every widget with `getBoundingClientRect` and stores the
 * result in its heightmap. Everything below the widget is positioned from that
 * number. So a widget whose PAINTED height differs from its MEASURED height
 * desynchronizes the map from the real layout, and `posAtCoords` — which walks
 * down accumulating heights — never finds a line that matches, walks off the
 * end, and returns position 0.
 *
 * The symptom is never "the widget looks wrong." It is: ArrowUp anywhere below
 * a table or an image teleports the caret to the top of the document, and
 * clicks near the widget land somewhere else. Two separate days have been lost
 * to this. Hence two rules, both mechanical:
 *
 * ── RULE 1: no vertical MARGIN on a widget's root element ────────────────────
 * `getBoundingClientRect` excludes margins, so margin is height the heightmap
 * cannot see. 16px was enough to break it. Spacing must live in a box that is
 * measured:
 *   - transparent wrapper  → use `padding` (see `.cm-table-wrapper`)
 *   - a visible surface    → transparent top/bottom `border` plus
 *                            `background-clip: padding-box`
 * The same rule already governs `.cm-line` headings, for the same reason.
 *
 * ── RULE 2: height must be known before paint, or announced after ───────────
 * Anything that grows AFTER the measurement invalidates it: images and video
 * loading, web fonts swapping, and — the quiet one — `<iconify-icon>`, which is
 * an unresolved custom element with no box until its SVG arrives, then suddenly
 * has one. Two ways to comply:
 *   - reserve the box up front → `createWidgetIcon()` below, and explicit
 *     width/height on media
 *   - tell CodeMirror when it changes → `remeasureOnResize()` below, wired in
 *     `toDOM` and undone in `destroy`
 *
 * A widget that does neither is a latent caret bug, not a styling nit.
 */

import type { EditorView } from '@codemirror/view';

type MeasuredEl = HTMLElement & { _cmResizeObs?: ResizeObserver };

/**
 * Tell CodeMirror to re-measure whenever `el` changes size.
 *
 * `requestMeasure` is batched, so firing on every observer tick is cheap. The
 * observer is parked on the element itself so `destroy` can disconnect it even
 * when CodeMirror recycles the widget instance.
 */
export function remeasureOnResize(view: EditorView, el: HTMLElement) {
	const observer = new ResizeObserver(() => view.requestMeasure());
	observer.observe(el);
	(el as MeasuredEl)._cmResizeObs = observer;
}

/** Undo `remeasureOnResize`. Safe to call on an element that never had one. */
export function disconnectRemeasure(dom: HTMLElement) {
	(dom as MeasuredEl)._cmResizeObs?.disconnect();
}

/**
 * An `<iconify-icon>` whose box exists before the icon does.
 *
 * The `width` attribute alone is not enough: until the SVG resolves, the custom
 * element is undefined and lays out as an inline element with no dimensions, so
 * the row it sits in measures short and then grows. Setting the size as inline
 * style reserves the space immediately, and the icon simply appears inside a
 * box that was already the right shape.
 */
export function createWidgetIcon(icon: string, size: number): HTMLElement {
	const el = document.createElement('iconify-icon');
	el.setAttribute('icon', icon);
	el.setAttribute('width', String(size));
	el.style.display = 'inline-block';
	el.style.width = `${size}px`;
	el.style.height = `${size}px`;
	el.style.flexShrink = '0';
	return el;
}
