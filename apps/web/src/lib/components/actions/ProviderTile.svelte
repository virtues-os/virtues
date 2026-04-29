<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import Badge from '$lib/components/Badge.svelte';
	import type { SourceCatalogItem } from '$lib/api/client';

	let {
		source,
		onConnect
	}: {
		source: SourceCatalogItem;
		onConnect: (source: SourceCatalogItem) => void;
	} = $props();
</script>

<button
	type="button"
	class="tile"
	onclick={() => onConnect(source)}
>
	<div class="icon-wrap">
		<Icon icon={source.icon ?? 'ri:plug-line'} width="20" />
	</div>
	<div class="body">
		<div class="title-row">
			<span class="title">{source.name}</span>
			{#if source.credential_count > 0}
				<Badge variant="success">
					{source.credential_count} connected
				</Badge>
			{/if}
		</div>
		{#if source.description}
			<div class="desc">{source.description}</div>
		{/if}
	</div>
	<div class="action">
		<Icon icon="ri:add-line" width="16" />
	</div>
</button>

<style>
	.tile {
		display: grid;
		grid-template-columns: 36px 1fr auto;
		align-items: flex-start;
		gap: 0.75rem;
		padding: 0.875rem 1rem;
		border-radius: 10px;
		border: 1px solid var(--color-border, #e5e7eb);
		background: var(--color-surface, #fff);
		text-align: left;
		cursor: pointer;
		font: inherit;
		color: inherit;
		transition: border-color 120ms, background 120ms;
	}
	.tile:hover:not(:disabled) {
		border-color: var(--color-foreground-muted, #6b7280);
		background: var(--color-surface-elevated, #f9fafb);
	}

	.icon-wrap {
		display: grid;
		place-items: center;
		width: 36px;
		height: 36px;
		border-radius: 999px;
		background: var(--color-surface-elevated, #f3f4f6);
		color: var(--color-foreground-muted, #6b7280);
	}

	.body {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		min-width: 0;
	}

	.title-row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}
	.title {
		font-size: 0.9375rem;
		font-weight: 600;
	}

	.desc {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle, #9ca3af);
		line-height: 1.35;
	}

	.action {
		color: var(--color-foreground-muted, #6b7280);
		align-self: center;
	}
</style>
