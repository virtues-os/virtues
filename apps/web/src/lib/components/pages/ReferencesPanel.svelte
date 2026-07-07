<script lang="ts">
	/**
	 * ReferencesPanel — a collapsible right-hand panel listing every page that
	 * links to the active page (inbound references / backlinks). Each row shows
	 * the source title and a one-line context snippet; clicking opens it.
	 *
	 * Kept deliberately quiet: it protects the writing canvas by living off to
	 * the side, only present when summoned.
	 */
	import Icon from "$lib/components/Icon.svelte";
	import { type Backlink } from "$lib/api/client";

	interface Props {
		backlinks: Backlink[];
		loading: boolean;
		onOpen: (pageId: string, title: string) => void;
		onClose: () => void;
	}

	let { backlinks, loading, onOpen, onClose }: Props = $props();
</script>

<aside class="references-panel">
	<header class="references-header">
		<span class="references-title">
			References
			{#if backlinks.length > 0}
				<span class="references-count">{backlinks.length}</span>
			{/if}
		</span>
		<button class="references-close" onclick={onClose} title="Close references">
			<Icon icon="ri:close-line" width="16" />
		</button>
	</header>

	<div class="references-body">
		{#if loading}
			<div class="references-state">
				<Icon icon="ri:loader-4-line" width="15" class="spin" />
				<span>Finding references…</span>
			</div>
		{:else if backlinks.length === 0}
			<div class="references-empty">
				<Icon icon="ri:links-line" width="18" />
				<p>No references yet.</p>
				<span
					>When another page links here with <span class="mono">@</span>, it
					shows up in this list.</span
				>
			</div>
		{:else}
			<ul class="references-list">
				{#each backlinks as ref (ref.id)}
					<li>
						<button
							class="reference-item"
							onclick={() => onOpen(ref.id, ref.title || "Untitled")}
						>
							<span class="reference-item-head">
								<Icon
									icon={ref.icon || "ri:file-text-line"}
									width="14"
									class="reference-icon"
								/>
								<span class="reference-item-title"
									>{ref.title || "Untitled"}</span
								>
							</span>
							<span class="reference-snippet">{ref.snippet}</span>
						</button>
					</li>
				{/each}
			</ul>
		{/if}
	</div>
</aside>

<style>
	.references-panel {
		display: flex;
		flex-direction: column;
		width: 300px;
		flex-shrink: 0;
		height: 100%;
		border-left: 1px solid var(--color-border);
		background: var(--color-surface);
		overflow: hidden;
	}

	.references-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0.75rem 0.875rem 0.75rem 1rem;
		border-bottom: 1px solid var(--color-border-subtle, var(--color-border));
		flex-shrink: 0;
	}

	.references-title {
		display: inline-flex;
		align-items: center;
		gap: 0.4375rem;
		font-size: 0.8125rem;
		font-weight: 600;
		letter-spacing: 0.02em;
		color: var(--color-foreground);
	}

	.references-count {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		min-width: 1.125rem;
		height: 1.125rem;
		padding: 0 0.3125rem;
		border-radius: 999px;
		background: var(--color-surface-elevated);
		color: var(--color-foreground-muted);
		font-size: 0.6875rem;
		font-weight: 500;
	}

	.references-close {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 26px;
		height: 26px;
		border: none;
		background: transparent;
		color: var(--color-foreground-muted);
		border-radius: 6px;
		cursor: pointer;
		transition:
			color 0.15s ease,
			background-color 0.15s ease;
	}

	.references-close:hover {
		color: var(--color-foreground);
		background: var(--color-surface-elevated);
	}

	.references-body {
		flex: 1;
		overflow-y: auto;
		padding: 0.5rem;
	}

	.references-state {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 1rem 0.75rem;
		color: var(--color-foreground-muted);
		font-size: 0.8125rem;
	}

	.references-empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.375rem;
		padding: 2.5rem 1.25rem;
		text-align: center;
		color: var(--color-foreground-subtle);
	}

	.references-empty p {
		margin: 0.25rem 0 0;
		font-size: 0.875rem;
		color: var(--color-foreground-muted);
	}

	.references-empty span {
		font-size: 0.75rem;
		line-height: 1.5;
	}

	.mono {
		font-family: var(--font-mono, monospace);
	}

	.references-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
	}

	.reference-item {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		width: 100%;
		padding: 0.5625rem 0.625rem;
		border: none;
		background: transparent;
		text-align: left;
		border-radius: 0.5rem;
		cursor: pointer;
		transition: background-color 0.12s ease;
	}

	.reference-item:hover {
		background: var(--color-surface-elevated);
	}

	.reference-item-head {
		display: flex;
		align-items: center;
		gap: 0.4375rem;
		min-width: 0;
	}

	.reference-item :global(.reference-icon) {
		color: var(--color-foreground-subtle);
		flex-shrink: 0;
	}

	.reference-item-title {
		min-width: 0;
		font-size: 0.8125rem;
		font-weight: 500;
		color: var(--color-foreground);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.reference-snippet {
		font-size: 0.75rem;
		line-height: 1.45;
		color: var(--color-foreground-muted);
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}
</style>
