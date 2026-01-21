<script lang="ts">
	import WikiRailContents from "./WikiRailContents.svelte";
	import type { Snippet } from "svelte";

	interface Props {
		content: string;
		metadata?: Snippet;
	}

	let { content, metadata }: Props = $props();
</script>

<aside class="wiki-right-rail">
	<div class="rail-card">
		{#if metadata}
			<div class="rail-metadata">
				{@render metadata()}
			</div>
		{/if}
		
		<div class="rail-contents">
			<WikiRailContents {content} />
		</div>
	</div>
</aside>

<style>
	.wiki-right-rail {
		position: relative;
		flex-shrink: 0;
		width: 200px;
		padding: 2rem 1rem 2rem 0;
		height: 100%;
		overflow-y: auto;
	}

	.rail-card {
		position: sticky;
		top: 0;
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border);
		border-radius: 6px;
		overflow: hidden;
	}

	.rail-metadata {
		padding: 1rem;
		border-bottom: 1px solid var(--color-border);
	}

	.rail-contents {
		padding: 0.5rem 0;
	}

	/* Scrollbar styling */
	.wiki-right-rail::-webkit-scrollbar {
		width: 6px;
	}

	.wiki-right-rail::-webkit-scrollbar-track {
		background: transparent;
	}

	.wiki-right-rail::-webkit-scrollbar-thumb {
		background-color: var(--color-border);
		border-radius: 3px;
	}

	.wiki-right-rail::-webkit-scrollbar-thumb:hover {
		background-color: var(--color-border-strong);
	}

	/* Responsive */
	@media (max-width: 1100px) {
		.wiki-right-rail {
			display: none;
		}
	}
</style>
