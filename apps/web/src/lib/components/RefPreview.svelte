<script lang="ts">
	import { portal } from "$lib/actions/portal";
	// Preview: the middle reference density. A floating card shown on hover/focus
	// of any ref pill, or pinned open by a click on a citation. The body is the
	// shared RefCard (per-type content) unless the caller passes its own; this
	// component owns only the floating shell, positioning, and actions. Portalled
	// to <body> and fixed-positioned off the anchor rect so it escapes overflow /
	// clipping in chat + editor alike.
	import type { Snippet } from "svelte";
	import Icon from "$lib/components/Icon.svelte";
	import RefCard from "$lib/components/RefCard.svelte";

	let {
		anchor,
		type,
		label,
		url,
		mimeType,
		onOpen,
		onTurnInto,
		oncardenter,
		oncardleave,
		pinned = false,
		prefer = "above",
		width = 260,
		onclose,
		onkeydown,
		children,
	} = $props<{
		anchor: HTMLElement;
		type: string | null;
		label: string;
		url?: string;
		mimeType?: string;
		onOpen?: () => void;
		onTurnInto?: (density: "pill" | "preview" | "full") => void;
		oncardenter?: () => void;
		oncardleave?: () => void;
		/**
		 * Opened by a click, not a hover: a dialog that takes focus, closes on
		 * Escape or a click outside it, and points at its anchor.
		 */
		pinned?: boolean;
		prefer?: "above" | "below";
		width?: number;
		onclose?: (how: "escape" | "outside") => void;
		onkeydown?: (e: KeyboardEvent) => void;
		/** The card's whole body, in place of RefCard and its Open action. */
		children?: Snippet;
	}>();

	// Position: fixed card off the anchor rect, flipped above/below with clamping.
	let card = $state<HTMLElement | null>(null);
	let pos = $state<{ left: number; top: number; placement: "above" | "below"; notch: number }>({
		left: 0,
		top: 0,
		placement: "above",
		notch: 0,
	});

	function reposition() {
		if (!anchor || !card || !anchor.isConnected) return;
		const a = anchor.getBoundingClientRect();
		const c = card.getBoundingClientRect();
		const margin = pinned ? 10 : 8;
		const vw = window.innerWidth;
		const vh = window.innerHeight;

		const fitsAbove = a.top - margin - c.height >= 8;
		const fitsBelow = a.bottom + margin + c.height <= vh - 8;
		const above = prefer === "below" ? !fitsBelow && fitsAbove : fitsAbove;
		const top = above ? a.top - margin - c.height : a.bottom + margin;
		const center = a.left + a.width / 2;
		let left = pinned ? a.left - 16 : center - c.width / 2;
		left = Math.max(8, Math.min(left, vw - c.width - 8));
		// The pointer sits under the anchor's middle, kept off the card's corners.
		const notch = Math.max(14, Math.min(center - left, c.width - 14));

		pos = {
			left,
			top: Math.max(8, Math.min(top, vh - c.height - 8)),
			placement: above ? "above" : "below",
			notch,
		};
	}

	$effect(() => {
		// Re-run when the anchor changes; the observer below catches the card
		// changing size (a record that loads after the card opens).
		void anchor;
		reposition();
	});

	$effect(() => {
		if (!card) return;
		const ro = new ResizeObserver(() => reposition());
		ro.observe(card);
		return () => ro.disconnect();
	});

	// A pinned card that moves to another anchor slides there, so the eye can
	// follow it from one citation to the next. The first placement does not.
	let gliding = $state(false);
	let lastAnchor: HTMLElement | null = null;
	$effect(() => {
		const a = anchor;
		if (pinned && lastAnchor && a !== lastAnchor) {
			gliding = true;
			const t = setTimeout(() => (gliding = false), 320);
			lastAnchor = a;
			return () => clearTimeout(t);
		}
		lastAnchor = a;
	});

	$effect(() => {
		if (pinned && card) card.focus({ preventScroll: true });
	});

	function onWindowPointerDown(e: PointerEvent) {
		if (!pinned || !card) return;
		const t = e.target as Node | null;
		if (!t || card.contains(t) || anchor?.contains(t)) return;
		onclose?.("outside");
	}

	function onCardKeydown(e: KeyboardEvent) {
		if (pinned && e.key === "Escape") {
			e.preventDefault();
			onclose?.("escape");
			return;
		}
		onkeydown?.(e);
	}
</script>

<svelte:window on:scroll={reposition} on:resize={reposition} on:pointerdown|capture={onWindowPointerDown} />

<!-- A dialog when pinned, a tooltip when hovered; the role is chosen at run time. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions, a11y_no_noninteractive_tabindex -->
<div
	bind:this={card}
	use:portal
	class="ref-preview {pos.placement}"
	class:pinned
	class:gliding
	role={pinned ? "dialog" : "tooltip"}
	aria-label={pinned ? label : undefined}
	tabindex={pinned ? -1 : undefined}
	style="left: {pos.left}px; top: {pos.top}px; width: {width}px; --notch: {pos.notch}px;"
	onmouseenter={() => oncardenter?.()}
	onmouseleave={() => oncardleave?.()}
	onkeydown={onCardKeydown}
>
	{#if pinned}<span class="ref-preview-notch" aria-hidden="true"></span>{/if}
	{#if children}
		{@render children()}
	{:else}
		<RefCard {type} {label} {url} {mimeType} />

		<div class="ref-preview-actions">
			<button class="ref-preview-open" onclick={() => onOpen?.()}>
				<Icon icon="ri:arrow-right-up-line" width="13" /> Open
			</button>
			{#if onTurnInto}
				<button class="ref-preview-turn" title="Turn into…" onclick={() => onTurnInto?.("full")}>
					<Icon icon="ri:layout-line" width="13" /> Turn into…
				</button>
			{/if}
		</div>
	{/if}
</div>

<style>
	.ref-preview {
		position: fixed;
		z-index: var(--z-popover);
		display: flex;
		flex-direction: column;
		max-width: calc(100vw - 16px);
		box-sizing: border-box;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-xl);
		box-shadow:
			0 4px 6px -1px rgba(0, 0, 0, 0.1),
			0 10px 20px -5px rgba(0, 0, 0, 0.15);
		animation: ref-preview-in var(--motion-enter) var(--ease-premium);
	}

	/* The hover card clips its media to its corners. The pinned card can't
	   clip, or it would cut off its own pointer, so its body clips instead. */
	.ref-preview:not(.pinned) {
		overflow: hidden;
	}

	.ref-preview.pinned {
		outline: none;
	}

	.ref-preview.gliding {
		transition:
			left var(--motion-layout) var(--ease-premium),
			top var(--motion-layout) var(--ease-premium);
	}

	/* A small square turned 45°, carrying the card's border on its two outer
	   sides, under the anchor's middle. */
	.ref-preview-notch {
		position: absolute;
		left: calc(var(--notch) - 6px);
		width: 12px;
		height: 12px;
		background: var(--color-surface);
		transform: rotate(45deg);
		pointer-events: none;
	}
	.ref-preview.below .ref-preview-notch {
		top: -7px;
		border-top: 1px solid var(--color-border);
		border-left: 1px solid var(--color-border);
		border-top-left-radius: var(--radius-xs);
	}
	.ref-preview.above .ref-preview-notch {
		bottom: -7px;
		border-bottom: 1px solid var(--color-border);
		border-right: 1px solid var(--color-border);
		border-bottom-right-radius: var(--radius-xs);
	}

	/* RefCard is boxless (padding normally supplied by the embed shell); the
	   floating popover supplies its own. */
	.ref-preview :global(.ref-card) {
		padding: 11px 12px;
	}

	@keyframes ref-preview-in {
		from {
			opacity: 0;
			transform: translateY(3px);
		}
		to {
			opacity: 1;
			transform: translateY(0);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.ref-preview {
			animation: none;
		}
		.ref-preview.gliding {
			transition: none;
		}
	}

	.ref-preview-actions {
		display: flex;
		gap: 6px;
		padding: 0 12px 10px;
	}
	.ref-preview-actions button {
		display: inline-flex;
		align-items: center;
		gap: 3px;
		padding: 3px 8px;
		font-size: 0.6875rem;
		border-radius: var(--radius-md);
		border: 1px solid var(--color-border);
		background: transparent;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: background-color 0.12s ease;
	}
	.ref-preview-actions button:hover {
		background: var(--ref-pill-bg);
		color: var(--color-primary);
	}
	.ref-preview-open {
		color: var(--color-primary) !important;
	}
</style>
