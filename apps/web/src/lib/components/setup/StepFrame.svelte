<!--
	One step's page. Every step answers "what am I supposed to do?" the same
	way: a title that is a verb, at most one sentence under it, the thing
	itself, then ONE filled way forward and, where the step allows it, a
	quiet way past.

	The frame is the same on every step so the only thing that changes from
	one to the next is the work — the "Vince" complaint was never that a step
	was hard, it was not knowing what the step wanted.
-->
<script lang="ts">
	import type { Snippet } from "svelte";

	interface Props {
		/** "3 of 5", over the title. */
		eyebrow?: string;
		title?: string;
		/** One sentence. Longer than that belongs in the step itself or nowhere. */
		subtitle?: string;
		/** Full-bleed steps (the naming, the map) draw their own title. */
		bleed?: boolean;
		/** Title, sentence and way forward on the center line. The default
		 *  since 2026-09-25 (one alignment for every step); a step whose
		 *  work reads better from a left edge can still turn it off. */
		centered?: boolean;
		children: Snippet;
		actions?: Snippet;
	}

	let { eyebrow, title, subtitle, bleed = false, centered = true, children, actions }: Props = $props();
</script>

<section class="frame" class:bleed class:centered>
	{#if title}
		<header class="head">
			{#if eyebrow}<p class="eyebrow">{eyebrow}</p>{/if}
			<h1 class="title">{title}</h1>
			{#if subtitle}<p class="subtitle">{subtitle}</p>{/if}
		</header>
	{/if}

	<div class="body">
		{@render children()}
	</div>

	{#if actions}
		<footer class="actions">
			{@render actions()}
		</footer>
	{/if}
</section>

<style>
	.frame {
		width: 100%;
		max-width: 44rem;
		margin: 0 auto;
		padding: clamp(2.5rem, 8vh, 5.5rem) 16px 4rem;
		display: flex;
		flex-direction: column;
		min-height: 100%;
	}
	.frame.bleed {
		max-width: none;
		padding: 0;
	}

	.eyebrow {
		margin: 0 0 0.9rem;
		font-size: 12px;
		letter-spacing: 0.02em;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}
	.title {
		margin: 0;
		font-family: var(--font-serif, Georgia, serif);
		font-weight: 400;
		font-size: clamp(2rem, 4vw, 2.75rem);
		line-height: 1.08;
		letter-spacing: -0.015em;
		text-wrap: balance;
		color: var(--color-foreground);
	}
	.subtitle {
		margin: 0.9rem 0 0;
		max-width: 34rem;
		font-size: 1rem;
		line-height: 1.55;
		color: var(--color-foreground-muted);
	}

	.body {
		margin-top: 2.25rem;
	}
	.bleed .body {
		margin-top: 0;
		flex: 1;
		display: flex;
		flex-direction: column;
	}

	.actions {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 1.25rem;
		margin-top: 2.5rem;
	}
	.centered .head {
		text-align: center;
	}
	.centered .subtitle {
		margin-left: auto;
		margin-right: auto;
	}
	.centered .actions {
		justify-content: center;
	}

	/* The two controls every step shares, global to the room so no step
	   invents a third. One filled thing per screen, always the way forward. */
	:global(.setup-go) {
		display: inline-flex;
		align-items: center;
		gap: 0.55rem;
		font: inherit;
		font-size: 15px;
		padding: 0.75rem 1.5rem;
		border: none;
		border-radius: 999px;
		/* Primary, like the letter's close and the rail's tile: the way
		   forward is the same object on every screen of the walk. */
		background: var(--color-primary);
		color: var(--color-background);
		cursor: pointer;
		transition:
			opacity 0.15s ease,
			transform 0.15s ease;
	}
	:global(.setup-go:hover:not(:disabled)) {
		opacity: 0.86;
	}
	:global(.setup-go:active:not(:disabled)) {
		transform: translateY(1px);
	}
	:global(.setup-go:disabled) {
		opacity: 0.4;
		cursor: default;
	}
	:global(.setup-go:focus-visible),
	:global(.setup-past:focus-visible) {
		outline: 2px solid var(--color-primary);
		outline-offset: 3px;
	}
	:global(.setup-past) {
		padding: 0;
		border: none;
		background: none;
		font: inherit;
		font-size: 14px;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: color 0.15s ease;
	}
	:global(.setup-past:hover) {
		color: var(--color-foreground);
	}
</style>
