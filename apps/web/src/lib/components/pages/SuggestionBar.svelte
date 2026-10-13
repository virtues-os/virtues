<!--
	SuggestionBar: Accept and Reject for the proposal the caret is in
	(lib/document/suggestions.ts), under it; ⌘⌥Enter and ⌘⌥Backspace do the
	same from the keyboard. With more than one proposal on the page, the same
	for all of them.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { FloatingContent, type VirtualAnchor } from "$lib/floating";

	interface Props {
		position: { x: number; y: number };
		/** How many proposals the page holds. */
		count: number;
		onAccept: () => void;
		onReject: () => void;
		onAcceptAll: () => void;
		onRejectAll: () => void;
	}

	let { position, count, onAccept, onReject, onAcceptAll, onRejectAll }: Props = $props();

	const virtualAnchor = $derived<VirtualAnchor>({ x: position.x, y: position.y, width: 0, height: 0 });
</script>

<FloatingContent
	anchor={virtualAnchor}
	options={{ placement: "bottom-start", offset: 8, flip: true, shift: true, padding: 8 }}
	class="suggestion-bar-container"
>
	<div class="suggestion-bar" role="toolbar" aria-label="Suggested edit" tabindex="-1" data-print="hide" onmousedown={(e) => e.preventDefault()}>
		<button type="button" class="sb-btn accept" onclick={onAccept}>
			<Icon icon="ri:check-line" width="14" />
			<span>Accept</span>
		</button>
		<button type="button" class="sb-btn" onclick={onReject}>
			<Icon icon="ri:close-line" width="14" />
			<span>Reject</span>
		</button>
		{#if count > 1}
			<span class="sep"></span>
			<button type="button" class="sb-btn" onclick={onAcceptAll}>Accept all</button>
			<button type="button" class="sb-btn" onclick={onRejectAll}>Reject all</button>
		{/if}
	</div>
</FloatingContent>

<style>
	:global(.suggestion-bar-container) {
		--z-floating: 102;
		padding: 0;
		background: transparent;
		border: none;
		box-shadow: none;
	}

	.suggestion-bar {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 4px;
		border: 1px solid var(--color-border-subtle, var(--color-border));
		border-radius: 12px;
		background: var(--color-surface);
	}

	.sb-btn {
		display: flex;
		align-items: center;
		gap: 4px;
		height: 28px;
		padding: 0 8px;
		border: none;
		border-radius: 6px;
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: 0.8125rem;
		cursor: pointer;
	}

	.sb-btn:hover {
		background: var(--hover-bg);
		color: var(--color-foreground);
	}

	.sb-btn.accept {
		color: var(--color-primary);
	}

	/* A thumb's 44pt (design-grammar §6): the target is the button itself. */
	@media (pointer: coarse) {
		.sb-btn {
			height: 44px;
			padding: 0 12px;
		}
	}

	.sep {
		width: 1px;
		align-self: stretch;
		margin: 4px;
		background: var(--color-border-subtle, var(--color-border));
	}
</style>
