// @vitest-environment happy-dom
import { EditorView } from '@codemirror/view';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { createTestView, destroyTestView, lines } from '../test-utils';
import { dragJustEnded, isMouseSelecting, mouseFreeze } from './mouse-freeze';

let view: EditorView | undefined;
afterEach(() => {
	if (view) destroyTestView(view);
	view = undefined;
	vi.useRealTimers();
});

const mount = (doc = 'one line\n\nanother line', extra: Parameters<typeof createTestView>[1] = {}) =>
	createTestView(doc, { ...extra, extensions: [mouseFreeze, ...(extra.extensions ?? [])] });

/** A primary, left-button pointer event unless the init says otherwise. */
function pointer(type: string, target: EventTarget, init: PointerEventInit = {}) {
	target.dispatchEvent(
		new PointerEvent(type, {
			bubbles: true,
			cancelable: true,
			composed: true,
			button: 0,
			isPrimary: true,
			pointerId: 1,
			pointerType: 'touch',
			...init,
		}),
	);
}

describe('mouse freeze: pointer events', () => {
	it('starts out released', () => {
		view = mount();
		expect(isMouseSelecting(view.state)).toBe(false);
	});

	it('pointerdown on a line freezes, pointerup on the editor releases', () => {
		view = mount();
		pointer('pointerdown', lines(view)[0]);
		expect(isMouseSelecting(view.state)).toBe(true);
		pointer('pointerup', lines(view)[0]);
		expect(isMouseSelecting(view.state)).toBe(false);
	});

	it('releases when pointerup lands on the window, outside the editor', () => {
		view = mount();
		pointer('pointerdown', view.contentDOM);
		expect(isMouseSelecting(view.state)).toBe(true);
		// A drag that ends past the editor's edge: the up never touches view.dom.
		pointer('pointerup', window);
		expect(isMouseSelecting(view.state)).toBe(false);
	});

	it('releases even when a widget stops pointerup propagation', () => {
		view = mount();
		const line = lines(view)[0];
		line.addEventListener('pointerup', (e) => e.stopPropagation());
		pointer('pointerdown', line);
		expect(isMouseSelecting(view.state)).toBe(true);
		pointer('pointerup', line);
		expect(isMouseSelecting(view.state)).toBe(false);
	});

	it('pointercancel releases only after the tail', () => {
		vi.useFakeTimers();
		view = mount();
		pointer('pointerdown', view.contentDOM);
		expect(isMouseSelecting(view.state)).toBe(true);
		// The touch became a scroll.
		pointer('pointercancel', window);
		expect(isMouseSelecting(view.state)).toBe(true);
		vi.advanceTimersByTime(99);
		expect(isMouseSelecting(view.state)).toBe(true);
		vi.advanceTimersByTime(1);
		expect(isMouseSelecting(view.state)).toBe(false);
	});

	it('a new press during the cancel tail keeps the freeze', () => {
		vi.useFakeTimers();
		view = mount();
		pointer('pointerdown', view.contentDOM);
		pointer('pointercancel', window);
		vi.advanceTimersByTime(50);
		pointer('pointerdown', view.contentDOM);
		vi.advanceTimersByTime(200);
		expect(isMouseSelecting(view.state)).toBe(true);
		pointer('pointerup', window);
		expect(isMouseSelecting(view.state)).toBe(false);
	});

	it('ignores a non-primary pointer', () => {
		view = mount();
		pointer('pointerdown', view.contentDOM, { isPrimary: false, pointerId: 2 });
		expect(isMouseSelecting(view.state)).toBe(false);
		// And its up does not release a freeze it never engaged.
		pointer('pointerup', window, { isPrimary: false, pointerId: 2 });
		expect(isMouseSelecting(view.state)).toBe(false);
	});

	it('ignores a non-left button', () => {
		view = mount();
		pointer('pointerdown', view.contentDOM, { button: 2, pointerType: 'mouse' });
		expect(isMouseSelecting(view.state)).toBe(false);
	});

	it('ignores a press outside contentDOM (scrollbar, gutters, panels)', () => {
		view = mount();
		pointer('pointerdown', view.scrollDOM);
		expect(isMouseSelecting(view.state)).toBe(false);
		pointer('pointerdown', view.dom);
		expect(isMouseSelecting(view.state)).toBe(false);
	});

	it('a consumer sees dragJustEnded on the release transaction', () => {
		const seen: boolean[] = [];
		view = mount('text', {
			extensions: [EditorView.updateListener.of((u) => seen.push(dragJustEnded(u)))],
		});
		pointer('pointerdown', view.contentDOM);
		expect(seen.at(-1)).toBe(false);
		pointer('pointerup', window);
		expect(seen.at(-1)).toBe(true);
	});

	it('survives the view being destroyed mid-drag', () => {
		vi.useFakeTimers();
		const v = mount();
		pointer('pointerdown', v.contentDOM);
		expect(isMouseSelecting(v.state)).toBe(true);
		destroyTestView(v);
		expect(() => pointer('pointerup', window)).not.toThrow();

		const w = mount();
		pointer('pointerdown', w.contentDOM);
		pointer('pointercancel', window);
		destroyTestView(w);
		expect(() => vi.advanceTimersByTime(500)).not.toThrow();
	});
});
