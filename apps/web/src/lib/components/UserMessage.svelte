<script lang="ts">
	import { slide } from 'svelte/transition';
	import Ref from './Ref.svelte';
	import { parseEntityRoute } from '$lib/utils/refRoutes';

	let { text }: { text: string } = $props();

	// The user's message is shown as they typed it — plain text, whitespace
	// preserved — not as markdown. Rendering it through the answer renderer
	// turned a leading `- ` into a list, four spaces into a code block, and
	// zeroed paragraph gaps; the echo stopped matching the composer (VIR-333).
	// The one construct we do recognize is the @-mention pill, which the
	// composer serializes as `[Name](/person/id)` so the model sees the link.
	// That, and only that, comes back as a chip; any other bracket-paren text
	// stays literal.
	type Segment = { kind: 'text'; text: string } | { kind: 'ref'; name: string; url: string };

	const MENTION = /\[([^\]\n]+)\]\(([^)\s]+)\)/g;

	function segment(src: string): Segment[] {
		const out: Segment[] = [];
		let last = 0;
		for (const m of src.matchAll(MENTION)) {
			const [whole, name, url] = m;
			const at = m.index ?? 0;
			if (parseEntityRoute(url) === null) continue;
			if (at > last) out.push({ kind: 'text', text: src.slice(last, at) });
			out.push({ kind: 'ref', name, url });
			last = at + whole.length;
		}
		if (last < src.length) out.push({ kind: 'text', text: src.slice(last) });
		return out;
	}

	const segments = $derived(segment(text));

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
	>{#each segments as seg, i (i)}{#if seg.kind === 'ref'}<Ref displayName={seg.name} url={seg.url} />{:else}{seg.text}{/if}{/each}</div>

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
	/* What was typed, as typed: newlines, indentation, runs of spaces. The
	   container is written without inner whitespace so pre-wrap has nothing
	   of ours to preserve. */
	.user-message-content {
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		line-height: 1.5;
	}
</style>
