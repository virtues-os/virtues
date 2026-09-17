<script lang="ts">
	import type { Snippet } from "svelte";

	/**
	 * A page's title, its one sentence, and the controls that belong to the
	 * whole page.
	 *
	 * The title was set `font-serif font-medium` — a 500 weight on JJannon,
	 * which ships exactly one cut. A weight request inside that family resolves
	 * to the regular and returns silently, so the declaration did nothing at
	 * best; where a browser synthesizes instead, it returned a smeared faux
	 * bold. design-grammar.md §4 says the serif is never bold, and it is worth
	 * knowing that this is not only taste: there is no bold to set. Hierarchy
	 * comes from size, case, ink and space.
	 *
	 * The sizes are the scale's, not Tailwind's steps: 36 for a page title
	 * (`text-3xl` was 30), then 26 and 22. `--font-serif-ui` is deliberately
	 * NOT used here — it is for serif beside an icon or in a fixed-height row,
	 * and applying its metric correction to a page title would move the first
	 * baseline of every page.
	 */
	let {
		title,
		description,
		level = 1,
		class: className = "",
		children,
		actions,
	}: {
		title?: string;
		description?: string;
		level?: 1 | 2 | 3;
		class?: string;
		children?: Snippet;
		actions?: Snippet;
	} = $props();
</script>

<div class="page-heading {className}" data-level={level}>
	<div class="page-heading-row">
		<div class="page-heading-titles">
			{#if title}
				{#if level === 1}
					<h1 class="page-title">{title}</h1>
				{:else if level === 2}
					<h2 class="page-title">{title}</h2>
				{:else}
					<h3 class="page-title">{title}</h3>
				{/if}
			{:else if children}
				{@render children()}
			{/if}
			{#if description}
				<p class="page-heading-description">{description}</p>
			{/if}
		</div>
		{#if actions}
			<div class="page-heading-actions">{@render actions()}</div>
		{/if}
	</div>
</div>

<style>
	.page-heading[data-level="1"] {
		margin-bottom: 24px;
	}
	.page-heading[data-level="2"] {
		margin-bottom: 16px;
	}
	.page-heading[data-level="3"] {
		margin-bottom: 12px;
	}

	.page-heading-row {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 16px;
	}

	.page-heading-titles {
		min-width: 0;
		flex: 1;
	}

	.page-title {
		margin: 0;
		font-family: var(--font-serif);
		font-weight: 400;
		line-height: 1.15;
		color: var(--color-foreground);
	}

	[data-level="1"] .page-title {
		font-size: 36px;
	}
	[data-level="2"] .page-title {
		font-size: 26px;
	}
	[data-level="3"] .page-title {
		font-size: 22px;
	}

	/* The page's one sentence (design-grammar.md §1), in the sans. */
	.page-heading-description {
		margin: 10px 0 0;
		font-family: var(--font-sans);
		font-size: 15px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.page-heading-actions {
		flex-shrink: 0;
	}
</style>
