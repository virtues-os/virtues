<script lang="ts">
	/**
	 * The app's one tooltip. Mounted once; any element opts in with
	 * `data-tip="New chat"`, and `data-tip-key="mod+n"` adds its shortcut,
	 * formatted for this platform (⌘N on a Mac).
	 *
	 * It waits 500ms the first time, so passing over a row on the way to
	 * somewhere else shows nothing. Once one has shown, the next appears at
	 * once for 1.5s, so running along a toolbar reads each button without
	 * waiting again, as macOS does. Keyboard focus shows it too, at once.
	 *
	 * It sits below its element, or beside it with `data-tip-side="right"`,
	 * and moves to stay on screen. The element keeps its own aria-label: the
	 * tip repeats it for the eye, so screen readers are not told twice.
	 *
	 * TODO(2026-10-21): migrate the remaining `title=` tooltips onto
	 * `data-tip`. The shell (sidebar, window header, tab bar) is moved.
	 */
	import { onMount, tick } from 'svelte';
	import { shortcuts } from '$lib/shortcuts/registry.svelte';

	const DELAY_MS = 500;
	const WARM_MS = 1500;
	const GAP_PX = 6;
	const EDGE_PX = 8;

	let tip = $state<{ text: string; key: string | null; side: 'bottom' | 'right' } | null>(null);
	let left = $state(0);
	let top = $state(0);
	let el: HTMLDivElement | undefined = $state();

	let anchor: HTMLElement | null = null;
	let timer: ReturnType<typeof setTimeout> | undefined;
	let lastHidden = 0;

	function tipTarget(node: EventTarget | null): HTMLElement | null {
		return node instanceof Element ? node.closest<HTMLElement>('[data-tip]') : null;
	}

	async function show(target: HTMLElement) {
		const text = target.dataset.tip?.trim();
		if (!text) return;
		const key = target.dataset.tipKey;
		anchor = target;
		tip = {
			text,
			key: key ? shortcuts.format(key) : null,
			side: target.dataset.tipSide === 'right' ? 'right' : 'bottom',
		};
		await tick();
		place();
	}

	function place() {
		if (!anchor || !el || !tip) return;
		const a = anchor.getBoundingClientRect();
		const t = el.getBoundingClientRect();
		let x: number;
		let y: number;
		if (tip.side === 'right') {
			x = a.right + GAP_PX;
			y = a.top + a.height / 2 - t.height / 2;
		} else {
			x = a.left + a.width / 2 - t.width / 2;
			y = a.bottom + GAP_PX;
			if (y + t.height > innerHeight - EDGE_PX) y = a.top - GAP_PX - t.height;
		}
		left = Math.round(Math.max(EDGE_PX, Math.min(innerWidth - t.width - EDGE_PX, x)));
		top = Math.round(Math.max(EDGE_PX, y));
	}

	function hide() {
		clearTimeout(timer);
		timer = undefined;
		if (!tip && !anchor) return;
		if (tip) lastHidden = Date.now();
		tip = null;
		anchor = null;
	}

	/**
	 * Focus leaving, from a listener. Removing a focused element fires
	 * `focusout` (and can blur the window) synchronously, while Svelte is
	 * tearing that element down, and a state write then throws
	 * state_unsafe_mutation. So the hide waits for the teardown to finish.
	 * Tabbing fires `focusout` and then `focusin` on the next element, which
	 * may arm a tip of its own; the wait hides only the tip that was there.
	 */
	function hideLater() {
		if (!tip && !anchor && !timer) return;
		const was = anchor;
		const pending = timer;
		queueMicrotask(() => {
			if (anchor === was && timer === pending) hide();
		});
	}

	function arm(target: HTMLElement, immediate: boolean) {
		clearTimeout(timer);
		if (immediate || Date.now() - lastHidden < WARM_MS) void show(target);
		else timer = setTimeout(() => void show(target), DELAY_MS);
	}

	onMount(() => {
		const over = (e: PointerEvent) => {
			if (e.pointerType === 'touch') return;
			const target = tipTarget(e.target);
			if (target === anchor) return;
			if (!target) {
				if (anchor || timer) hide();
				return;
			}
			if (anchor) hide();
			arm(target, false);
		};
		const focusIn = (e: FocusEvent) => {
			const target = tipTarget(e.target);
			if (target && target.matches(':focus-visible')) arm(target, true);
		};
		const key = (e: KeyboardEvent) => {
			if (e.key === 'Escape') hide();
		};
		document.addEventListener('pointerover', over);
		document.addEventListener('pointerdown', hide, true);
		document.addEventListener('focusin', focusIn);
		document.addEventListener('focusout', hideLater);
		document.addEventListener('keydown', key, true);
		document.addEventListener('scroll', hide, true);
		window.addEventListener('blur', hideLater);
		return () => {
			clearTimeout(timer);
			document.removeEventListener('pointerover', over);
			document.removeEventListener('pointerdown', hide, true);
			document.removeEventListener('focusin', focusIn);
			document.removeEventListener('focusout', hideLater);
			document.removeEventListener('keydown', key, true);
			document.removeEventListener('scroll', hide, true);
			window.removeEventListener('blur', hideLater);
		};
	});
</script>

{#if tip}
	<div bind:this={el} class="tooltip" role="tooltip" aria-hidden="true" style="left: {left}px; top: {top}px">
		<span>{tip.text}</span>
		{#if tip.key}<kbd>{tip.key}</kbd>{/if}
	</div>
{/if}

<style>
	/* A hairline card on the raised surface, like the menus. No shadow. */
	.tooltip {
		position: fixed;
		z-index: var(--z-tooltip);
		display: flex;
		align-items: baseline;
		gap: 10px;
		max-width: 280px;
		padding: 4px 8px;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background: var(--color-surface-elevated);
		color: var(--color-foreground);
		font-size: 12px;
		line-height: 1.4;
		pointer-events: none;
		animation: tooltip-in 90ms var(--ease-premium);
	}

	kbd {
		font-family: inherit;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}

	@keyframes tooltip-in {
		from {
			opacity: 0;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.tooltip {
			animation: none;
		}
	}
</style>
