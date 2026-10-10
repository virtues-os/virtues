/**
 * A widget's size handle (an image's width, an applet's height) as a slider
 * the keyboard moves, as well as the pointer that drags it: the arrows by a
 * step, Page Up and Page Down by a page, Home and End to either end. The
 * value is written once per key, as a drag writes it once when it ends
 * (`dragToResize`). On touch a handle takes the pointer only while its
 * widget is selected (the widgets' own CSS): a thumb that begins a scroll
 * over it scrolls.
 */

export interface SizeRange {
	min: number;
	max: number;
	/** What an arrow moves it by. */
	step: number;
	/** What Page Up and Page Down move it by. */
	page: number;
}

/** The size a key sets the handle to, from `now`; null for a key the handle does not take. */
export function sizeByKey(key: string, now: number, r: SizeRange): number | null {
	const clamp = (v: number) => Math.round(Math.min(r.max, Math.max(r.min, v)));
	switch (key) {
		case 'ArrowRight':
		case 'ArrowUp':
			return clamp(now + r.step);
		case 'ArrowLeft':
		case 'ArrowDown':
			return clamp(now - r.step);
		case 'PageUp':
			return clamp(now + r.page);
		case 'PageDown':
			return clamp(now - r.page);
		case 'Home':
			return r.min;
		case 'End':
			return r.max;
		default:
			return null;
	}
}

/** How far a pointer moves on a size handle before the drag is read as a resize or not, in CSS pixels. */
export const DRAG_SLOP = 6;

/**
 * Whether a drag on a size handle that has moved (`dx`, `dy`) resizes: null
 * until it has moved `DRAG_SLOP`, then whether it moved mostly along the
 * handle's axis. One that moved mostly across it is a scroll a thumb began
 * on the handle.
 */
export function dragResizes(dx: number, dy: number, axis: 'x' | 'y'): boolean | null {
	if (Math.hypot(dx, dy) < DRAG_SLOP) return null;
	const along = Math.abs(axis === 'x' ? dx : dy);
	const across = Math.abs(axis === 'x' ? dy : dx);
	return along > across;
}

export interface ResizeDrag extends Pick<SizeRange, 'min' | 'max'> {
	axis: 'x' | 'y';
	/** The size the drag starts from. */
	start: number;
	/** Each size the drag passes through, to draw. */
	onSize: (size: number) => void;
	/** The drag is over: the size to write, or null when it resized nothing or was cancelled. */
	onEnd: (size: number | null) => void;
}

/**
 * Follow a drag on a size handle from its pointerdown, `down`. It resizes
 * once it has moved `DRAG_SLOP` mostly along the handle's axis; one that
 * moves mostly across it resizes nothing and lets go. Only a drag that
 * ends with the pointer lifted writes a size, once; a cancelled one (the
 * browser took the gesture) writes none.
 */
export function dragToResize(handle: HTMLElement, down: PointerEvent, o: ResizeDrag): void {
	down.preventDefault();
	down.stopPropagation();
	handle.setPointerCapture?.(down.pointerId);
	const x0 = down.clientX;
	const y0 = down.clientY;
	let resizes: boolean | null = null;
	let size: number | null = null;
	const stop = (write: boolean) => {
		handle.removeEventListener('pointermove', move);
		handle.removeEventListener('pointerup', up);
		handle.removeEventListener('pointercancel', cancel);
		if (handle.hasPointerCapture?.(down.pointerId)) handle.releasePointerCapture(down.pointerId);
		o.onEnd(write ? size : null);
	};
	const move = (ev: PointerEvent) => {
		const dx = ev.clientX - x0;
		const dy = ev.clientY - y0;
		resizes ??= dragResizes(dx, dy, o.axis);
		if (resizes === false) return stop(false);
		if (!resizes) return;
		size = Math.round(Math.min(o.max, Math.max(o.min, o.start + (o.axis === 'x' ? dx : dy))));
		o.onSize(size);
	};
	const up = () => stop(true);
	const cancel = () => stop(false);
	handle.addEventListener('pointermove', move);
	handle.addEventListener('pointerup', up);
	handle.addEventListener('pointercancel', cancel);
}
