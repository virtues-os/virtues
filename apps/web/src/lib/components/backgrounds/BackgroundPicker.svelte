<script lang="ts">
	/**
	 * Settings › Appearance › Background: Plain or a style, then a
	 * collection in it, then one picture. A choice that stops above a
	 * picture changes picture each day (lib/backgrounds/resolve.ts). Each
	 * card shows its pictures in the current appearance. The lake is not a
	 * choice: it is what home shows until one is made (Background.svelte).
	 *
	 * A style the server does not have yet follows the installed ones, with
	 * its size. Picking it downloads it to the server, and it becomes the
	 * setting once it is installed (download in lib/backgrounds/controls.ts).
	 */
	import { onMount } from 'svelte';
	import { background } from '$lib/stores/background.svelte';
	import { parseSetting, type Picture, type StyleManifest } from '$lib/backgrounds/resolve';
	import { download, megabytes, thumbnail } from '$lib/backgrounds/controls';
	import type { AvailableStyle } from '$lib/backgrounds/resolve';

	onMount(() => {
		background.start();
		void background.ensureStyles();
	});

	const setting = $derived(parseSetting(background.setting));
	const styles = $derived(background.styles ?? []);
	const style = $derived(setting.kind === 'path' ? (styles.find((s) => s.style === setting.style) ?? null) : null);
	const collection = $derived(
		style && setting.kind === 'path' && setting.collection ? (style.collections.find((c) => c.id === setting.collection) ?? null) : null,
	);

	const thumb = (s: StyleManifest, p: Picture) => thumbnail(s.style, p);

	/** Up to four pictures for a card that stands for many. */
	function mosaic(s: StyleManifest, pictures: Picture[]): string[] {
		return pictures.slice(0, 4).map((p) => thumb(s, p));
	}

	const firsts = (s: StyleManifest) => s.collections.map((c) => c.pictures[0]).filter(Boolean);

	/** What a style not on the server says under its name. */
	function downloadNote(a: AvailableStyle): string {
		const p = background.installing[a.style];
		if (p) return `Downloading, ${Math.floor((100 * p.received_bytes) / Math.max(1, p.total_bytes))}%`;
		if (background.failed[a.style]) return background.failed[a.style];
		return `${megabytes(a.bytes)} to download`;
	}

	function startDownload(style: string) {
		if (background.installing[style]) return;
		download(style).catch((error) => console.warn('Background download did not start:', error));
	}
</script>

{#snippet card(label: string, picked: boolean, onclick: () => void, images: string[], kind: 'one' | 'many' | 'plain' = 'one', note = '')}
	<button type="button" role="radio" class="card" class:picked aria-checked={picked} {onclick}>
		<span class="frame" class:plain={kind === 'plain'} class:many={kind === 'many'}>
			{#each kind === 'many' ? images : images.slice(0, 1) as src (src)}
				<img {src} alt="" loading="lazy" decoding="async" />
			{/each}
		</span>
		<span class="name">{label}</span>
		{#if note}<span class="note">{note}</span>{/if}
	</button>
{/snippet}

<div class="picker">
	<div class="cards" role="radiogroup" aria-label="Style">
		{@render card('Plain', setting.kind === 'plain', () => background.set('plain'), [], 'plain')}
		{#each styles as s (s.style)}
			{@render card(s.title, setting.kind === 'path' && setting.style === s.style, () => background.set(s.style), [thumb(s, firsts(s)[0])])}
		{/each}
		{#each background.available as a (a.style)}
			{@const preview = a.preview[background.appearance] ?? a.preview.light}
			{@render card(a.title, false, () => startDownload(a.style), preview ? [preview] : [], 'one', downloadNote(a))}
		{/each}
	</div>

	{#if style}
		<h3 class="step">Collection</h3>
		<div class="cards" role="radiogroup" aria-label="Collection">
			{@render card('Changes daily', !collection, () => background.set(style.style), mosaic(style, firsts(style)), 'many')}
			{#each style.collections as c (c.id)}
				{@render card(c.title, collection?.id === c.id, () => background.set(`${style.style}/${c.id}`), [thumb(style, c.pictures[0])])}
			{/each}
		</div>
	{/if}

	{#if style && collection}
		<h3 class="step">Picture</h3>
		<div class="cards" role="radiogroup" aria-label="Picture">
			{@render card(
				'Changes daily',
				setting.kind === 'path' && !setting.picture,
				() => background.set(`${style.style}/${collection.id}`),
				mosaic(style, collection.pictures),
				'many',
			)}
			{#each collection.pictures as p (p.id)}
				{@render card(
					p[background.appearance].title,
					setting.kind === 'path' && setting.picture === p.id,
					() => background.set(`${style.style}/${collection.id}/${p.id}`),
					[thumb(style, p)],
				)}
			{/each}
		</div>
	{/if}
</div>

<style>
	.picker {
		display: flex;
		flex-direction: column;
		max-width: 40rem;
	}

	.cards {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
		gap: 16px;
	}

	.step {
		margin: 24px 0 8px;
		font-size: var(--text-label);
		font-weight: inherit;
		color: var(--color-foreground-muted);
	}

	.card {
		display: flex;
		flex-direction: column;
		gap: 8px;
		min-width: 0;
		padding: 0;
		color: var(--color-foreground-muted);
		text-align: left;
		background: none;
		border: none;
		cursor: pointer;
	}

	.frame {
		display: block;
		aspect-ratio: 16 / 9;
		overflow: hidden;
		border-radius: var(--radius-xl);
		/* The selection is the accent edge, as on the theme cards. */
		outline: 1px solid var(--color-border);
		outline-offset: -1px;
		transition: outline-color var(--motion-hover-out) var(--ease-premium);
	}

	.frame.plain {
		background: var(--desk);
	}

	.frame.many {
		display: grid;
		grid-template-columns: 1fr 1fr;
		grid-template-rows: 1fr 1fr;
		gap: 1px;
	}

	.frame img {
		display: block;
		width: 100%;
		height: 100%;
		min-height: 0;
		object-fit: cover;
	}

	.card:hover .frame {
		outline-color: var(--color-foreground-subtle);
		transition-duration: var(--motion-hover-in);
	}

	.card.picked .frame {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

	.name {
		overflow: hidden;
		font-size: var(--text-label);
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.card.picked .name {
		color: var(--color-foreground);
	}

	.note {
		margin-top: -4px;
		font-size: var(--text-caption);
		line-height: 1.3;
		color: var(--color-foreground-muted);
	}

	@media (prefers-reduced-motion: reduce) {
		.frame {
			transition: none;
		}
	}
</style>
