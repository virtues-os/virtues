<!--
  SourceRow — one connectable source inside "Connect your world".

  Icon + name + the WHY (why it's worth connecting), a Connect button or a
  connected checkmark, and the privacy receipt hung in the margin. Prominence
  ('anchor' | 'prominent' | 'quiet') tunes the emphasis so the richest sources
  lead and the long tail stays quiet.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { Button } from "$lib";
	import type { SourceCopy } from "./sources-copy";
	import type { SourceCatalogItem } from "$lib/api/client";

	interface Props {
		source: SourceCatalogItem;
		copy: SourceCopy;
		connected: boolean;
		busy?: boolean;
		onConnect: () => void;
	}

	let { source, copy, connected, busy = false, onConnect }: Props = $props();
</script>

<div class="row" class:anchor={copy.prominence === "anchor"} class:connected>
	<span class="icon"><Icon icon={source.icon ?? "ri:plug-line"} width={copy.prominence === "anchor" ? 24 : 20} /></span>
	<div class="text">
		<div class="name-line">
			<span class="name">{source.name}</span>
			{#if connected}
				<span class="badge"><Icon icon="ri:check-line" width="13" /> Connected</span>
			{/if}
		</div>
		<p class="why">{copy.why}</p>
	</div>
	<div class="cta">
		{#if connected}
			<button class="add-more" onclick={onConnect} disabled={busy}>Add another</button>
		{:else}
			<Button variant={copy.prominence === "anchor" ? "primary" : "secondary"} size="sm" onclick={onConnect} disabled={busy}>
				{busy ? "…" : "Connect"}
			</Button>
		{/if}
	</div>
</div>

<style>
	@reference "../../../../app.css";

	.row {
		position: relative;
		display: grid;
		grid-template-columns: auto 1fr auto;
		align-items: start;
		gap: 0.9rem;
		padding: 1.1rem 0;
	}
	.row + .row {
		border-top: 1px solid var(--color-border-subtle);
	}

	.icon {
		display: flex;
		height: 2.25rem;
		width: 2.25rem;
		flex-shrink: 0;
		align-items: center;
		justify-content: center;
		border-radius: 0.6rem;
		background: var(--color-surface-elevated);
		color: var(--color-foreground);
	}
	.anchor .icon {
		background: color-mix(in srgb, var(--color-primary) 12%, transparent);
		color: var(--color-primary);
	}

	.text {
		min-width: 0;
	}
	.name-line {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}
	.name {
		font-family: var(--font-serif);
		font-size: 1.05rem;
		color: var(--color-foreground);
	}
	.anchor .name {
		font-size: 1.2rem;
	}
	.why {
		margin: 0.25rem 0 0;
		font-size: 0.9rem;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.badge {
		display: inline-flex;
		align-items: center;
		gap: 0.15rem;
		font-size: 0.7rem;
		font-weight: 500;
		color: var(--color-success);
	}

	.cta {
		flex-shrink: 0;
		padding-top: 0.15rem;
	}
	.add-more {
		font-size: 0.8rem;
		color: var(--color-foreground-subtle);
		transition: color 0.15s ease;
	}
	.add-more:hover {
		color: var(--color-foreground);
	}
</style>
