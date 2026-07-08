<script lang="ts">
	// Preview: the middle reference density. A floating card shown on hover/focus
	// of any ref pill. The body is the shared RefCard (per-type content); this
	// component owns only the floating shell, positioning, and actions. Portalled
	// to <body> and fixed-positioned off the anchor rect so it escapes overflow /
	// clipping in chat + editor alike.
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
	} = $props<{
		anchor: HTMLElement;
		type: string | null;
		label: string;
		url?: string;
		mimeType?: string;
		onOpen: () => void;
		onTurnInto?: (density: "pill" | "preview" | "full") => void;
		oncardenter?: () => void;
		oncardleave?: () => void;
	}>();

	function portal(node: HTMLElement) {
		document.body.appendChild(node);
		return { destroy: () => node.remove() };
	}

	// Position: fixed card off the anchor rect, flipped above/below with clamping.
	let card = $state<HTMLElement | null>(null);
	let pos = $state<{ left: number; top: number; placement: "above" | "below" }>({
		left: 0,
		top: 0,
		placement: "above",
	});

	function reposition() {
		if (!anchor || !card) return;
		const a = anchor.getBoundingClientRect();
		const c = card.getBoundingClientRect();
		const margin = 8;
		const vw = window.innerWidth;
		const vh = window.innerHeight;

		const above = a.top - margin - c.height >= 8;
		const top = above ? a.top - margin - c.height : a.bottom + margin;
		let left = a.left + a.width / 2 - c.width / 2;
		left = Math.max(8, Math.min(left, vw - c.width - 8));

		pos = { left, top: Math.max(8, Math.min(top, vh - c.height - 8)), placement: above ? "above" : "below" };
	}

	$effect(() => {
		// Re-run once the card (and any async summary) changes measurable size.
		reposition();
	});
</script>

<svelte:window on:scroll={reposition} on:resize={reposition} />

<div
	bind:this={card}
	use:portal
	class="ref-preview {pos.placement}"
	role="tooltip"
	style="left: {pos.left}px; top: {pos.top}px;"
	onmouseenter={() => oncardenter?.()}
	onmouseleave={() => oncardleave?.()}
>
	<RefCard {type} {label} {url} {mimeType} />

	<div class="ref-preview-actions">
		<button class="ref-preview-open" onclick={onOpen}>
			<Icon icon="ri:arrow-right-up-line" width="13" /> Open
		</button>
		{#if onTurnInto}
			<button class="ref-preview-turn" title="Turn into…" onclick={() => onTurnInto?.("full")}>
				<Icon icon="ri:layout-line" width="13" /> Turn into…
			</button>
		{/if}
	</div>
</div>

<style>
	.ref-preview {
		position: fixed;
		z-index: 60;
		display: flex;
		flex-direction: column;
		width: 260px;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 10px;
		box-shadow:
			0 4px 6px -1px rgba(0, 0, 0, 0.1),
			0 10px 20px -5px rgba(0, 0, 0, 0.15);
		overflow: hidden;
		animation: ref-preview-in 0.12s ease-out;
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
		border-radius: 6px;
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
