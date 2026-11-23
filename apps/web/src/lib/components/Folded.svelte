<script lang="ts">
	/**
	 * Long text, shown as its first few lines with "Show all" under them.
	 * Text that fits shows whole, with nothing under it. The cut fades out
	 * so it reads as more to come, not as text cut off by mistake.
	 */
	import type { Snippet } from 'svelte';
	import TextAction from '$lib/components/TextAction.svelte';

	let {
		lines = 6,
		lineHeight = 24,
		children,
	}: {
		/** How many lines show before "Show all". */
		lines?: number;
		/** The text's line height, in pixels. */
		lineHeight?: number;
		children: Snippet;
	} = $props();

	let open = $state(false);
	let full = $state(0);
	const max = $derived(lines * lineHeight);
	/** Folding saves at least two lines, or it is not worth a control. */
	const long = $derived(full > max + 2 * lineHeight);
</script>

<div class="folded">
	<div class="clip" class:cut={long && !open} style:max-height={long && !open ? `${max}px` : undefined}>
		<div bind:clientHeight={full}>{@render children()}</div>
	</div>
	{#if long}
		<span class="toggle"><TextAction quiet onclick={() => (open = !open)}>{open ? 'Show less' : 'Show all'}</TextAction></span>
	{/if}
</div>

<style>
	.folded {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.clip {
		overflow: hidden;
	}
	.clip.cut {
		mask-image: linear-gradient(to bottom, black calc(100% - 48px), transparent);
	}
	.toggle {
		font-size: var(--text-label);
	}
</style>
