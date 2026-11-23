<script lang="ts">
	/**
	 * The line between two side-by-side panes, dragged to set the width of
	 * the one before it. It has no width of its own: it sits on the line and
	 * its grab area reaches 4px into each pane. A pill shows on hover and
	 * takes the accent while held, as the tab split's divider and the
	 * sidebar's seam do.
	 *
	 * Dragging uses pointer capture, so the drag follows the pointer anywhere,
	 * works with touch, and ends however it is interrupted. The arrow keys
	 * move it 16px (64px with Shift), and Home or a double-click puts it back
	 * where the page wants it.
	 *
	 * The width is the host's to keep. `onchange` hands it a width already
	 * held between `min` and `max`.
	 */
	let {
		width,
		min,
		max,
		label,
		onchange,
		onreset,
	}: {
		/** The pane before the divider, in pixels, as it is drawn now. */
		width: number;
		min: number;
		max: number;
		/** What dragging it does, for a screen reader. */
		label: string;
		onchange: (width: number) => void;
		/** Back to where the page wants it. */
		onreset: () => void;
	} = $props();

	let held = $state(false);
	let from = { x: 0, width: 0 };

	const clamp = (w: number) => Math.round(Math.max(min, Math.min(Math.max(min, max), w)));

	function down(e: PointerEvent) {
		// The primary button only: a right-click is not a drag.
		if (e.button !== 0) return;
		e.preventDefault();
		held = true;
		from = { x: e.clientX, width };
		try {
			(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
		} catch {
			// A synthetic or already-captured pointer: moves still arrive while
			// the pointer is over the divider.
		}
	}
	function move(e: PointerEvent) {
		if (!held) return;
		// No button held means the release went somewhere the divider never
		// heard about, such as another app after ⌘-Tab.
		if (e.buttons === 0) {
			held = false;
			return;
		}
		onchange(clamp(from.width + e.clientX - from.x));
	}
	function up(e: PointerEvent) {
		held = false;
		try {
			(e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
		} catch {
			// Capture can already be gone if the pointer left the window.
		}
	}
	function key(e: KeyboardEvent) {
		const step = e.shiftKey ? 64 : 16;
		if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
			e.preventDefault();
			onchange(clamp(width + (e.key === 'ArrowLeft' ? -step : step)));
		} else if (e.key === 'Home') {
			e.preventDefault();
			onreset();
		}
	}
</script>

<!-- A focusable separator with a value is a widget: the arrow keys move it. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
	class="divider"
	class:held
	role="separator"
	aria-orientation="vertical"
	aria-label={label}
	aria-valuenow={Math.round(width)}
	aria-valuemin={min}
	aria-valuemax={max}
	tabindex="0"
	onpointerdown={down}
	onpointermove={move}
	onpointerup={up}
	onpointercancel={() => (held = false)}
	onlostpointercapture={() => (held = false)}
	ondblclick={onreset}
	onkeydown={key}
>
	<span class="grip" aria-hidden="true"></span>
</div>

<style>
	.divider {
		position: relative;
		z-index: 5;
		flex: none;
		align-self: stretch;
		width: 0;
		cursor: col-resize;
		/* A touch drag moves the divider instead of scrolling the page. */
		touch-action: none;
	}
	.divider::after {
		position: absolute;
		inset: 0 -4px;
		content: '';
	}
	/* The grip: a pill on the line, hidden until the pointer finds it. */
	.grip {
		position: absolute;
		top: 50%;
		left: 50%;
		z-index: 1;
		width: 4px;
		height: 32px;
		transform: translate(-50%, -50%);
		border-radius: var(--radius-full);
		background: var(--color-foreground-subtle);
		opacity: 0;
		pointer-events: none;
		transition:
			opacity var(--motion-state) var(--ease-premium),
			height var(--motion-state) var(--ease-premium);
	}
	.divider:hover .grip,
	.divider:focus-visible .grip {
		height: 40px;
		opacity: 1;
	}
	/* Held: the accent says you have it, and it follows the pointer. */
	.held .grip {
		height: 40px;
		background: var(--color-primary);
		opacity: 1;
		transition: none;
	}

	@media (prefers-reduced-motion: reduce) {
		.grip {
			transition: none;
		}
	}
</style>
