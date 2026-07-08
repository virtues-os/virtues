<script lang="ts">
	// Embed: the block reference density. A persistent card rendered in document
	// flow when a ref sits alone on a line. Same body as the hover Preview
	// (RefCard); a block shell instead of a floating one. Click → open.
	import RefCard from "$lib/components/RefCard.svelte";
	import Icon from "$lib/components/Icon.svelte";

	let { type, label, url, mimeType, onOpen } = $props<{
		type: string | null;
		label: string;
		url?: string;
		mimeType?: string;
		onOpen?: (openInTab: boolean) => void;
	}>();

	function handleClick(e: MouseEvent) {
		e.preventDefault();
		onOpen?.(e.metaKey || e.ctrlKey);
	}
</script>

<button class="ref-embed" onclick={handleClick} title="Open {label}">
	<RefCard {type} {label} {url} {mimeType} />
	<span class="ref-embed-open"><Icon icon="ri:arrow-right-up-line" width="14" /></span>
</button>

<style>
	.ref-embed {
		position: relative;
		display: block;
		width: 100%;
		margin: 6px 0;
		padding: 0;
		text-align: left;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 10px;
		overflow: hidden;
		cursor: pointer;
		font-family: inherit;
		transition:
			border-color 0.12s ease,
			background-color 0.12s ease;
	}
	.ref-embed:hover {
		border-color: color-mix(in srgb, var(--color-primary) 40%, var(--color-border));
		background: var(--ref-pill-bg);
	}

	.ref-embed-open {
		position: absolute;
		top: 8px;
		right: 8px;
		display: inline-flex;
		color: var(--color-foreground-subtle);
		opacity: 0;
		transition: opacity 0.12s ease;
	}
	.ref-embed:hover .ref-embed-open {
		opacity: 1;
	}
</style>
