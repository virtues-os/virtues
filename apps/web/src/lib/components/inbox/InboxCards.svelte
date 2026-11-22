<script lang="ts">
	/**
	 * The Inbox as cards, one column for each ring: what is for now, for
	 * today, for this week and for later.
	 */
	import { Card } from '$lib';
	import { RINGS, type InboxItem } from '$lib/inbox/items';
	import InboxMark from './InboxMark.svelte';

	let {
		items,
		picked = null,
		threadName,
		onPick,
	}: {
		items: InboxItem[];
		picked?: string | null;
		/** The name of the thread a thing sits in, or an empty string. */
		threadName: (it: InboxItem) => string;
		onPick: (key: string) => void;
	} = $props();
</script>

<div class="board">
	{#each RINGS as name, ring (name)}
		{@const cards = items.filter((it) => it.ring === ring)}
		<section class="col" aria-label={name}>
			<h3 class="group">{name}</h3>
			{#each cards as it (it.key)}
				<Card padding="none">
					<button type="button" class="card" class:on={it.key === picked} aria-pressed={it.key === picked} data-inbox-key={it.key} onclick={() => onPick(it.key)}>
						<span class="top"><InboxMark kind={it.kind} /><span class="aside">{it.meta}</span></span>
						<span class="title">{it.title}</span>
						{#if threadName(it)}<span class="aside">{threadName(it)}</span>{/if}
					</button>
				</Card>
			{:else}
				<p class="none">{ring === 0 ? 'Nothing needs you.' : 'Nothing here.'}</p>
			{/each}
		</section>
	{/each}
</div>

<style>
	.board {
		display: flex;
		align-items: flex-start;
		gap: 16px;
	}
	.col {
		display: flex;
		flex: 1 1 0;
		flex-direction: column;
		gap: 8px;
		min-width: 144px;
		max-width: 320px;
	}
	.group {
		margin: 0 0 4px;
		font-size: var(--text-small);
		font-weight: 400;
		line-height: 20px;
		color: var(--color-foreground-muted);
	}
	.card {
		display: flex;
		flex-direction: column;
		gap: 8px;
		width: 100%;
		padding: 12px;
		box-sizing: border-box;
		border: 0;
		/* The card's corner, less its hairline, so the wash fills it to the edge. */
		border-radius: calc(var(--radius-xl) - 1px);
		background: transparent;
		font: inherit;
		text-align: left;
		color: var(--color-foreground);
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.card:hover {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.card.on {
		background: var(--active-bg);
	}
	.top {
		display: flex;
		align-items: center;
		gap: 8px;
		min-height: 24px;
	}
	.title {
		font-size: var(--text-label);
		line-height: 20px;
	}
	.aside {
		font-size: var(--text-small);
		line-height: 20px;
		color: var(--color-foreground-muted);
	}
	.none {
		margin: 0;
		font-size: var(--text-small);
		line-height: 20px;
		color: var(--color-foreground-muted);
	}

	@media (prefers-reduced-motion: reduce) {
		.card {
			transition: none;
		}
	}
</style>
