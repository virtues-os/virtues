<!--
	hello_world — Card.svelte

	Demonstrates the view-runtime override path. Renders in TemplatesPanel
	in place of the generic TemplateCard. The matching action is declared
	at `actions/hello_world/manifest.toml` with:
	    runtime = "view"
	    config  = { view = { name = "hello_world" } }
-->

<script lang="ts">
	import type { Action } from '$lib/api/client';

	let {
		action,
		onclick
	}: {
		action: Action;
		onclick?: (action: Action) => void;
	} = $props();
</script>

<button type="button" class="card" onclick={() => onclick?.(action)}>
	<div class="hero">
		<div class="hero-glyph">·</div>
	</div>
	<div class="body">
		<h3 class="name">{action.name}</h3>
		<p class="excerpt">A small view applet — pure frontend, no backend invocation.</p>
		<div class="meta">
			<span>view runtime</span>
		</div>
	</div>
</button>

<style>
	.card {
		display: flex;
		flex-direction: column;
		overflow: hidden;
		border-radius: 10px;
		border: 1px solid var(--color-border, #e5e7eb);
		background: var(--color-surface, #fff);
		color: var(--color-foreground, inherit);
		text-align: left;
		font: inherit;
		cursor: pointer;
		transition: border-color 120ms ease;
	}
	.card:hover {
		border-color: var(--color-foreground-subtle, #9ca3af);
	}
	.card:focus-visible {
		outline: 2px solid var(--color-primary, #4338ca);
		outline-offset: 2px;
	}

	.hero {
		aspect-ratio: 21 / 9;
		width: 100%;
		background: var(--color-surface-elevated, #f3f4f6);
		display: grid;
		place-items: center;
		filter: var(--illustration-filter, none);
	}
	.hero-glyph {
		font-family: var(--font-serif, Georgia, 'Times New Roman', serif);
		font-size: 3rem;
		color: var(--color-foreground-muted, #6b7280);
		opacity: 0.5;
	}

	.body {
		display: flex;
		flex-direction: column;
		gap: 0.375rem;
		padding: 0.875rem 1rem 1rem;
	}
	.name {
		margin: 0;
		font-family: var(--font-serif, Georgia, 'Times New Roman', serif);
		font-size: 1.0625rem;
		font-weight: 400;
		line-height: 1.3;
	}
	.excerpt {
		margin: 0;
		font-size: 0.8125rem;
		line-height: 1.45;
		color: var(--color-foreground-muted, #6b7280);
	}
	.meta {
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle, #9ca3af);
		margin-top: 0.125rem;
	}
</style>
