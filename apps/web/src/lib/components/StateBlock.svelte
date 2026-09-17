<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import type { Snippet } from "svelte";

	/**
	 * The centered block a pane shows when it has nothing else to show.
	 *
	 * EmptyState, LoadingState and ErrorState each carried their own copy of
	 * the same `.state-wrap` / `.state-title` / `.state-message` rules —
	 * three definitions of one object, already disagreeing about gap (0.75rem
	 * in two, 0.5rem in the third). Nothing chose those differences; they are
	 * what happens when a shape is copied rather than named. The three remain
	 * as separate doors because a caller means three different things by them,
	 * but there is one block underneath, and it is this.
	 *
	 * `tone` colors the icon and nothing else. The title stays in the page's
	 * own ink in every tone: an error's words are not less readable than an
	 * empty list's, and coloring a whole block red to say "error" is the
	 * decoration design.md calls non-semantic.
	 */
	let {
		icon,
		iconSize = 32,
		spin = false,
		tone = "muted",
		title,
		message,
		class: className = "",
		actions,
	}: {
		icon?: string;
		iconSize?: number;
		/** The loading case; respects `prefers-reduced-motion`. */
		spin?: boolean;
		tone?: "muted" | "error";
		title?: string;
		message?: string;
		class?: string;
		actions?: Snippet;
	} = $props();
</script>

<div class="state-block {className}" class:is-error={tone === "error"} class:is-spinning={spin}>
	{#if icon}
		<Icon {icon} width={String(iconSize)} />
	{/if}
	{#if title}
		<p class="state-title">{title}</p>
	{/if}
	{#if message}
		<p class="state-message">{message}</p>
	{/if}
	{#if actions}
		<div class="state-actions">{@render actions()}</div>
	{/if}
</div>

<style>
	.state-block {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 12px;
		padding: 48px 32px;
		color: var(--color-foreground-muted);
		text-align: center;
	}

	.state-block :global(svg) {
		opacity: 0.5;
	}

	.is-error :global(svg) {
		color: var(--color-error);
		opacity: 1;
	}

	.is-spinning :global(svg) {
		opacity: 1;
		animation: state-spin 1s linear infinite;
	}

	.state-title {
		margin: 0;
		font-size: 15px;
		font-weight: 500;
		color: var(--color-foreground);
	}

	.state-message {
		margin: 0;
		font-size: 14px;
		color: var(--color-foreground-muted);
	}

	.state-actions {
		margin-top: 8px;
	}

	@keyframes state-spin {
		from {
			transform: rotate(0deg);
		}
		to {
			transform: rotate(360deg);
		}
	}

	/* Keep the information, drop the travel (design.md). A spinner that cannot
	   spin still has to read as "working", so it holds full opacity. */
	@media (prefers-reduced-motion: reduce) {
		.is-spinning :global(svg) {
			animation: none;
		}
	}
</style>
