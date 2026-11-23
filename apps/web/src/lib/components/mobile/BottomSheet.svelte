<!--
	BottomSheet.svelte

	The phone's sheet: the screen dims and a panel rises from the bottom, so
	every choice lands where the thumb is. A drag down closes it, by distance
	or by a flick, and so does a tap on the dimmed screen or Escape. A light
	haptic tap marks it opening on the native app.
-->
<script lang="ts">
	import type { Snippet } from "svelte";
	import { onMount } from "svelte";
	import { fade, fly } from "svelte/transition";
	import { cubicOut } from "svelte/easing";
	import { tapHaptic } from "$lib/utils/haptics";
	import { portal } from "$lib/actions/portal";

	let {
		label,
		onClose,
		children,
	}: {
		/** What the sheet is, for a screen reader. */
		label: string;
		onClose: () => void;
		children: Snippet;
	} = $props();

	const CLOSE_PX = 96; // dragged this far, it closes
	const FLICK_PX_PER_MS = 0.5; // released this fast downward, it closes

	let sheetEl = $state<HTMLElement | null>(null);
	let dragY = $state(0);
	let dragging = $state(false);
	let startY = 0;
	let lastY = 0;
	let lastT = 0;
	let velocity = 0;

	onMount(() => {
		tapHaptic();
	});

	/** A drag starts from the grabber, or anywhere once the content is at its top. */
	function scrolledToTop(target: EventTarget | null): boolean {
		let el = target instanceof HTMLElement ? target : null;
		while (el && el !== sheetEl) {
			if (el.scrollHeight > el.clientHeight + 1 && el.scrollTop > 0) return false;
			el = el.parentElement;
		}
		return true;
	}

	function onPointerDown(e: PointerEvent) {
		if (!scrolledToTop(e.target)) return;
		dragging = true;
		startY = lastY = e.clientY;
		lastT = e.timeStamp;
		velocity = 0;
	}

	function onPointerMove(e: PointerEvent) {
		if (!dragging) return;
		const dy = e.clientY - startY;
		const dt = e.timeStamp - lastT;
		if (dt > 0) velocity = (e.clientY - lastY) / dt;
		lastY = e.clientY;
		lastT = e.timeStamp;
		// Up past the resting place gives a little, then stops.
		dragY = dy > 0 ? dy : dy / 6;
	}

	function onPointerUp() {
		if (!dragging) return;
		dragging = false;
		if (dragY > CLOSE_PX || velocity > FLICK_PX_PER_MS) onClose();
		else dragY = 0;
	}

	function onKey(e: KeyboardEvent) {
		if (e.key === "Escape") onClose();
	}

</script>

<svelte:window onkeydown={onKey} />

<!-- A sheet opened from inside a view would sit in that view's stacking
     context, under its chat bar and any transformed parent. On the body,
     fixed means the screen. -->
<div class="layer" use:portal>
<button class="scrim" aria-label="Close" onclick={onClose} transition:fade={{ duration: 180 }}></button>

<div
	bind:this={sheetEl}
	class="sheet"
	class:dragging
	role="dialog"
	aria-modal="true"
	aria-label={label}
	tabindex="-1"
	style:transform={dragY ? `translateY(${dragY}px)` : undefined}
	transition:fly={{ y: 480, duration: 280, easing: cubicOut, opacity: 1 }}
	onpointerdown={onPointerDown}
	onpointermove={onPointerMove}
	onpointerup={onPointerUp}
	onpointercancel={onPointerUp}
>
	<span class="grabber" aria-hidden="true"></span>
	<div class="content">
		{@render children()}
	</div>
</div>
</div>

<style>
	.layer {
		display: contents;
	}

	.scrim {
		position: fixed;
		inset: 0;
		border: 0;
		cursor: default;
		z-index: var(--z-context-menu);
		background: color-mix(in srgb, var(--wash-ink) 28%, transparent);
	}

	.sheet {
		position: fixed;
		left: 0;
		right: 0;
		bottom: 0;
		z-index: var(--z-context-menu);
		display: flex;
		flex-direction: column;
		max-height: calc(100dvh - max(48px, env(safe-area-inset-top)));
		padding: 8px 12px calc(16px + env(safe-area-inset-bottom));
		border-radius: var(--radius-xl) var(--radius-xl) 0 0;
		background: var(--color-page);
		/* design-ok: the sheet is lifted over the dimmed screen */
		box-shadow: 0 -10px 30px -12px color-mix(in srgb, var(--wash-ink) 30%, transparent);
		touch-action: none;
		transition: transform 0.28s cubic-bezier(0.32, 0.72, 0, 1);
	}
	.sheet.dragging {
		transition: none;
	}

	.grabber {
		flex: none;
		align-self: center;
		width: 36px;
		height: 4px;
		margin-bottom: 8px;
		border-radius: var(--radius-full);
		background: var(--color-border-strong);
	}

	.content {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		touch-action: pan-y;
	}

	@media (prefers-reduced-motion: reduce) {
		.sheet {
			transition: none;
		}
	}
</style>
