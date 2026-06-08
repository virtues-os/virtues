<!--
	TemplateCard.svelte

	Restrained card for the Templates panel. Each template is meant to feel
	like an entry in a small pamphlet, not a SaaS marketing tile:

	  - Hero: an illustration provided by us (pencil sketch / line art),
	    served from `action.config.image_url`. When no image is set, falls
	    back to a calm warm-paper tone (no gradient).
	  - Title: serif, regular weight (no bold).
	  - Description: sans, regular weight, no italic.
	  - Meta: schedule + last run, small and quiet.

	No hover translate, no shadow lift — those are SaaS-y. Hover is a thin
	border-color change and that's it.
-->

<script lang="ts">
	import type { Action } from '$lib/api/client';
	import { describeSchedule, relativeTime } from '$lib/actions/palette';
	import { descriptionFor } from '$lib/actions/descriptions';

	let {
		action,
		onclick
	}: {
		action: Action;
		onclick?: (action: Action) => void;
	} = $props();

	const schedule = $derived(describeSchedule(action.cron_schedule));
	const description = $derived(descriptionFor(action));
	const lastRunAt = $derived(action.last_run?.started_at ?? null);
	const lastStatus = $derived(action.last_run?.status ?? null);
	const isFailing = $derived(lastStatus === 'error');

	// Custom illustration per template. Set in `action.config.image_url` —
	// path or absolute URL. We treat the asset as authored content (pencil
	// sketches / line art), not generated decoration. No image set ⇒ a calm
	// warm-paper block, no gradient.
	const imageUrl = $derived(
		typeof action.config?.image_url === 'string' ? (action.config.image_url as string) : null
	);

	function handleClick() {
		onclick?.(action);
	}
</script>

<button type="button" class="card" class:disabled={!action.enabled} onclick={handleClick}>
	<div class="hero" class:has-image={!!imageUrl}>
		{#if imageUrl}
			<img src={imageUrl} alt="" />
		{/if}
	</div>

	<div class="body">
		<h3 class="name">{action.name}</h3>
		{#if description}
			<p class="excerpt">{description}</p>
		{/if}
		<div class="meta">
			<span>{schedule}</span>
			{#if lastRunAt}
				<span class="sep">·</span>
				<span>{relativeTime(lastRunAt)}</span>
			{/if}
			{#if isFailing}
				<span class="fail" title="Last run failed">last run failed</span>
			{/if}
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
	.card.disabled {
		opacity: 0.55;
	}

	/* ── Hero ───────────────────────────────────────────────────────────── */

	.hero {
		/* Cinematic 21:9 strip — keeps the card from feeling top-heavy and
		   reads more like a book-banner than a screenshot tile. */
		aspect-ratio: 21 / 9;
		width: 100%;
		/* Theme-aware. Dark themes get a darker tone, light themes a lighter
		   one — same surface token used elsewhere in the app. */
		background: var(--color-surface-elevated, #f3f4f6);
		overflow: hidden;
	}
	.hero img {
		display: block;
		width: 100%;
		height: 100%;
		object-fit: contain;
		/* Illustrations are authored as black-line transparent PNGs (same
		   convention as the day-page illustration). Dark themes set
		   `--illustration-filter: invert(1)` in themes.css to flip the lines
		   to white. Light themes leave it `none`. */
		filter: var(--illustration-filter, none);
	}

	/* ── Body ───────────────────────────────────────────────────────────── */

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
		color: var(--color-foreground, #111827);
		display: -webkit-box;
		line-clamp: 2;
		-webkit-line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	.excerpt {
		margin: 0;
		font-size: 0.8125rem;
		font-weight: 400;
		line-height: 1.45;
		color: var(--color-foreground-muted, #6b7280);
		display: -webkit-box;
		line-clamp: 3;
		-webkit-line-clamp: 3;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	.meta {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		margin-top: 0.125rem;
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.sep {
		opacity: 0.45;
	}
	.fail {
		margin-left: 0.375rem;
		color: #b91c1c;
	}
</style>
