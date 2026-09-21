// @vitest-environment happy-dom
import { EditorView, type WidgetType } from '@codemirror/view';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { createTestView, destroyTestView } from '../test-utils';
import { __imageDimensionCache, mediaWidgets } from './media-widgets';

// The context-menu and link-editor stores are Svelte-rune modules (`$state`),
// which the vitest config (no SvelteKit plugin) cannot compile. Nothing here
// opens a menu, so both are stubbed at the module boundary.
vi.mock('../../stores/contextMenu.svelte', () => ({ contextMenu: { show: vi.fn() } }));
vi.mock('../../stores/linkEditor.svelte', () => ({ linkEditor: { show: vi.fn() } }));

// happy-dom ships a ResizeObserver, but `remeasureOnResize` must not be the
// reason these tests fail on a build that does not.
if (typeof globalThis.ResizeObserver === 'undefined') {
	globalThis.ResizeObserver = class {
		observe() {}
		unobserve() {}
		disconnect() {}
	} as unknown as typeof ResizeObserver;
}

const views: EditorView[] = [];
afterEach(() => {
	for (const v of views) destroyTestView(v);
	views.length = 0;
});
beforeEach(() => __imageDimensionCache.clear());

const mount = (doc: string) => {
	const v = createTestView(doc, { extensions: [mediaWidgets] });
	views.push(v);
	return v;
};

/** Every widget the view's decoration facet currently provides. */
function widgets(view: EditorView): WidgetType[] {
	const out: WidgetType[] = [];
	for (const provided of view.state.facet(EditorView.decorations)) {
		const set = typeof provided === 'function' ? provided(view) : provided;
		const it = set.iter();
		while (it.value) {
			const w = it.value.spec.widget as WidgetType | undefined;
			if (w) out.push(w);
			it.next();
		}
	}
	return out;
}

const img = (view: EditorView) => view.contentDOM.querySelector<HTMLImageElement>('img.cm-image');

/** Simulate a decode: happy-dom never sets natural dimensions itself. */
function decode(el: HTMLImageElement, w: number, h: number) {
	Object.defineProperty(el, 'naturalWidth', { value: w, configurable: true });
	Object.defineProperty(el, 'naturalHeight', { value: h, configurable: true });
	el.dispatchEvent(new Event('load'));
}

const URL_A = 'https://example.com/pictures/a.png';
const URL_B = 'https://example.com/pictures/b.png';

describe('image widget: dimension cache', () => {
	it('an unknown URL reserves nothing and estimates -1', () => {
		const view = mount(`![photo](${URL_A})`);
		const el = img(view);
		expect(el).not.toBeNull();
		expect(el!.hasAttribute('width')).toBe(false);
		expect(el!.hasAttribute('height')).toBe(false);
		expect(el!.style.aspectRatio).toBe('');
		const [widget] = widgets(view);
		expect(widget).toBeDefined();
		expect(widget.estimatedHeight).toBe(-1);
	});

	it('after one decode, a second widget for the same URL reserves the box', () => {
		const first = mount(`![photo](${URL_A})`);
		decode(img(first)!, 800, 600);
		expect(__imageDimensionCache.get(URL_A)).toEqual({ w: 800, h: 600 });

		const second = mount(`![photo](${URL_A})`);
		const el = img(second)!;
		expect(el.getAttribute('width')).toBe('800');
		expect(el.getAttribute('height')).toBe('600');
		const [widget] = widgets(second);
		expect(widget.estimatedHeight).toBeGreaterThan(0);
		// Natural height plus the wrapper's padding.
		expect(widget.estimatedHeight).toBe(600 + 16);
	});

	it('the cache is per URL', () => {
		const first = mount(`![photo](${URL_A})`);
		decode(img(first)!, 800, 600);

		const other = mount(`![photo](${URL_B})`);
		expect(img(other)!.hasAttribute('width')).toBe(false);
		expect(widgets(other)[0].estimatedHeight).toBe(-1);
	});

	it('a |width suffix keeps the author width and scales the estimate', () => {
		const first = mount(`![photo](${URL_A})`);
		decode(img(first)!, 800, 600);

		const sized = mount(`![photo|320](${URL_A})`);
		const el = img(sized)!;
		expect(el.style.width).toBe('320px');
		expect(el.getAttribute('width')).toBe('800');
		expect(el.getAttribute('height')).toBe('600');
		expect(widgets(sized)[0].estimatedHeight).toBe(240 + 16);
	});

	it('a |width wider than the image does not inflate the estimate', () => {
		const first = mount(`![photo](${URL_A})`);
		decode(img(first)!, 800, 600);
		const sized = mount(`![photo|2000](${URL_A})`);
		expect(widgets(sized)[0].estimatedHeight).toBe(600 + 16);
	});

	it('a zero-size decode (a broken image) is not cached', () => {
		const view = mount(`![photo](${URL_A})`);
		decode(img(view)!, 0, 0);
		expect(__imageDimensionCache.has(URL_A)).toBe(false);
	});

	it('a width change is a different widget (eq stays correct)', () => {
		const view = mount(`![photo](${URL_A})`);
		const before = img(view)!;
		view.dispatch({ changes: { from: 7, insert: '|320' } });
		const after = img(view)!;
		expect(after).not.toBe(before);
		expect(after.style.width).toBe('320px');
	});
});
