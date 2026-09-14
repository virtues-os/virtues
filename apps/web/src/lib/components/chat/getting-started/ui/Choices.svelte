<!--
	The row a step's controls sit in: always on its own line under the words
	they belong to, never butted against a sentence. One rhythm everywhere
	in the room.
-->
<script lang="ts">
	import type { Snippet } from "svelte";
	interface Props {
		children: Snippet;
		/** A second line of the same row, for a note or an error. */
		foot?: Snippet;
	}
	let { children, foot }: Props = $props();
</script>

<div class="choices">
	<div class="row">{@render children()}</div>
	{#if foot}<div class="foot">{@render foot()}</div>{/if}
</div>

<style>
	.choices {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		margin: 0.875rem 0 0.25rem;
	}
	.row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem;
	}
	.foot {
		font-size: 0.875rem;
		color: var(--color-foreground-muted);
	}
</style>
