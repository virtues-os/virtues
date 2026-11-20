<script lang="ts">
	/**
	 * One site in the Sites door: a preview of the site, its name, and one
	 * line. The line says when it last refreshed, or what it needs from you.
	 *
	 * The preview is the site's own view, drawn at desktop size and scaled to
	 * the card, so a card shows the real page with today's data. It loads when
	 * the card comes into view, because each one opens a frame and runs the
	 * view's queries. A site with no view shows its description in the
	 * reading face.
	 */
	import type { Applet } from '$lib/api/client';
	import FaceFrame from '$lib/components/applets/FaceFrame.svelte';
	import { problemOf } from '$lib/applets/standing';
	import { whenInWords } from '$lib/utils/dateUtils';

	let { site }: { site: Applet } = $props();

	// The size the view is drawn at before it is scaled to the card.
	const DRAWN_WIDTH = 1120;
	const DRAWN_HEIGHT = 700;

	let shot = $state<HTMLDivElement | null>(null);
	let width = $state(0);
	let seen = $state(false);

	$effect(() => {
		if (!shot || seen) return;
		const watch = new IntersectionObserver(
			(entries) => {
				if (entries.some((e) => e.isIntersecting)) seen = true;
			},
			{ rootMargin: '200px' }
		);
		watch.observe(shot);
		return () => watch.disconnect();
	});

	const trouble = $derived(problemOf(site));
	const line = $derived.by(() => {
		if (trouble) return `Needs you - ${trouble}`;
		if (!site.enabled) return 'Off';
		const ran = site.last_run?.status === 'success' ? site.last_run.started_at : null;
		return ran ? `Refreshed ${whenInWords(ran)}` : `Made ${whenInWords(site.created_at)}`;
	});
</script>

<div class="site-card">
	<div class="shot" bind:this={shot} bind:clientWidth={width} aria-hidden="true">
		{#if site.has_face}
			{#if seen && width > 0}
				<div
					class="drawn"
					style:width="{DRAWN_WIDTH}px"
					style:height="{DRAWN_HEIGHT}px"
					style:transform="scale({width / DRAWN_WIDTH})"
				>
					<FaceFrame appletId={site.id} height="{DRAWN_HEIGHT}px" />
				</div>
			{/if}
		{:else if site.description}
			<p class="words">{site.description}</p>
		{/if}
	</div>
	<span class="name">{site.name}</span>
	<span class="line" class:needs={!!trouble}>{line}</span>
</div>

<style>
	.site-card {
		display: flex;
		flex-direction: column;
		gap: 4px;
		width: 100%;
		min-width: 0;
		text-align: left;
	}
	.shot {
		position: relative;
		width: 100%;
		aspect-ratio: 8 / 5;
		overflow: hidden;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
		background: var(--color-surface);
		margin-bottom: 4px;
	}
	.drawn {
		position: absolute;
		top: 0;
		left: 0;
		transform-origin: top left;
		/* A preview is a picture of the site. Opening the card opens the site. */
		pointer-events: none;
	}
	.drawn :global(iframe.face-frame) {
		border: none;
		border-radius: 0;
	}
	.words {
		margin: 0;
		padding: 16px;
		font-family: var(--font-serif);
		font-size: var(--text-small);
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}
	.name {
		font-size: var(--text-label);
		font-weight: 500;
		color: var(--color-foreground);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.line {
		font-size: var(--text-small);
		color: var(--color-foreground-subtle);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.line.needs {
		color: var(--color-foreground-muted);
	}
</style>
