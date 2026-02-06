<script lang="ts">
	import { slide } from 'svelte/transition';
	import CitedMarkdown from './CitedMarkdown.svelte';

	let { text }: { text: string } = $props();

	let isExpanded = $state(false);
	let lineCount = $state(0);
	let textContainer: HTMLDivElement | undefined = $state();
	const LINE_THRESHOLD = 8;

	// Calculate line count after mount
	$effect(() => {
		if (textContainer) {
			const lineHeight = parseFloat(getComputedStyle(textContainer).lineHeight);
			const height = textContainer.scrollHeight;
			lineCount = Math.round(height / lineHeight);
		}
	});

	const shouldTruncate = $derived(lineCount > LINE_THRESHOLD);
	const maxHeight = $derived(!isExpanded && shouldTruncate ? `${LINE_THRESHOLD * 1.5}rem` : 'none');
</script>

<div class="user-message-container">
	<div
		bind:this={textContainer}
		class="text-base text-primary user-message-content"
		style="max-height: {maxHeight}; overflow: hidden;"
	>
		<CitedMarkdown content={text} />
	</div>

	{#if shouldTruncate}
		<button
			onclick={() => (isExpanded = !isExpanded)}
			class="block text-sm text-primary bg-surface-elevated rounded px-2 py-1 mt-2 transition-colors hover:bg-primary/15 cursor-pointer"
			transition:slide={{ duration: 200 }}
		>
			{isExpanded ? '↑ Show less' : '↓ Show more'}
		</button>
	{/if}
</div>

<style>
	/* Preserve whitespace in user messages while allowing markdown */
	.user-message-content :global(.markdown) {
		white-space: pre-wrap;
	}

	/* Ensure paragraphs don't add extra spacing */
	.user-message-content :global(p) {
		margin: 0;
	}
</style>
