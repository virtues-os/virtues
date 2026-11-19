<script lang="ts" module>
	export interface TabCardAction {
		id: string;
		label: string;
		icon: string;
		/** A toggle that is on, such as Unpin on a pinned tab. */
		on?: boolean;
		run: () => void;
	}
</script>

<script lang="ts">
	/**
	 * The card under a tab you rest the pointer on: the tab's whole title,
	 * which a fixed-width tab cuts off, and the tab's everyday actions as a
	 * row of icons. The tab bar decides when it shows and passes the actions;
	 * this component only draws and places it.
	 *
	 * It stays open while the pointer is on the tab or on the card, so you
	 * can move down onto an action.
	 */
	import { portal } from "$lib/actions/portal";
	import Icon from "$lib/components/Icon.svelte";

	let {
		anchor,
		title,
		actions,
		onenter,
		onleave,
	}: {
		anchor: HTMLElement;
		title: string;
		actions: TabCardAction[];
		onenter: () => void;
		onleave: () => void;
	} = $props();

	const GAP_PX = 6;
	const EDGE_PX = 8;

	let card = $state<HTMLElement | null>(null);
	let left = $state(0);
	let top = $state(0);

	// Hangs from the tab's left edge, as wide as the tab or wider.
	$effect(() => {
		if (!card) return;
		void title;
		const a = anchor.getBoundingClientRect();
		const c = card.getBoundingClientRect();
		left = Math.round(Math.max(EDGE_PX, Math.min(innerWidth - c.width - EDGE_PX, a.left)));
		top = Math.round(a.bottom + GAP_PX);
	});

	const minWidth = $derived(Math.max(220, Math.round(anchor.getBoundingClientRect().width)));
</script>

<div
	use:portal
	bind:this={card}
	class="tab-card"
	role="group"
	aria-label={title}
	style="left: {left}px; top: {top}px; min-width: {minWidth}px"
	onpointerenter={onenter}
	onpointerleave={onleave}
>
	<p class="tab-card-title">{title}</p>
	{#if actions.length}
		<div class="tab-card-actions">
			{#each actions as action (action.id)}
				<button
					type="button"
					class="tab-card-action"
					class:on={action.on}
					aria-label={action.label}
					aria-pressed={action.on}
					data-tip={action.label}
					onclick={(e) => {
						e.stopPropagation();
						action.run();
					}}
				>
					<Icon icon={action.icon} width="16" height="16" aria-hidden="true" />
				</button>
			{/each}
		</div>
	{/if}
</div>

<style>
	/* The context menu's card: the raised surface, a hairline and the float
	   shadow. */
	.tab-card {
		position: fixed;
		z-index: var(--z-context-menu);
		max-width: 320px;
		padding: 10px 8px 6px;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
		box-shadow: var(--shadow-float);
		color: var(--color-foreground);
		animation: tab-card-in var(--motion-enter) var(--ease-premium);
	}

	/* Two lines at most; a longer title ends in an ellipsis. */
	.tab-card-title {
		margin: 0 4px 8px;
		font-size: 13px;
		line-height: 1.35;
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
		overflow-wrap: anywhere;
	}

	.tab-card-actions {
		display: flex;
		justify-content: space-around;
		gap: 4px;
	}

	.tab-card-action {
		display: flex;
		flex: 1;
		align-items: center;
		justify-content: center;
		height: 34px;
		padding: 0;
		border: none;
		border-radius: var(--radius-md);
		background: transparent;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition:
			background-color var(--motion-hover-out) var(--ease-premium),
			color var(--motion-hover-out) var(--ease-premium);
	}

	.tab-card-action:hover {
		background: var(--hover-bg);
		color: var(--color-foreground);
		transition-duration: var(--motion-hover-in);
	}

	.tab-card-action:active {
		background: var(--press-bg);
		transition-duration: var(--motion-press);
	}

	.tab-card-action.on {
		color: var(--color-primary);
	}

	@keyframes tab-card-in {
		from {
			opacity: 0;
			translate: 0 -4px;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.tab-card {
			animation: none;
		}
	}
</style>
