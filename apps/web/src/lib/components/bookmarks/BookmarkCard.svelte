<!--
	BookmarkCard.svelte

	One save on the Wall, laid out from the fields it has (lib/bookmarks/shape.ts).

	The saved thing is set in the serif: a post's text at the body size, a
	title at the lead size. Everything about it is in the sans at the small
	size: the author or site, the page's summary and facts, the footer. Your
	note is a filled block in the sans, so your words never read as the page's.

	A picture that fails to load is removed and the card lays out as if it had
	none, so a dead image never leaves a broken glyph or a gap.
-->
<script lang="ts">
	import type { BookmarkApi } from '$lib/bookmarks/api';
	import { footerOf, shapeOf } from '$lib/bookmarks/shape';
	import LinkPreviewCard from './LinkPreviewCard.svelte';

	let { bookmark, href, onopen }: { bookmark: BookmarkApi; href: string; onopen: (e: MouseEvent) => void } = $props();

	const shape = $derived(shapeOf(bookmark));
	let failed = $state<Record<string, boolean>>({});
	const shown = (key: string, src: string | null) => (src && !failed[key] ? src : null);
	const hide = (key: string) => () => (failed = { ...failed, [key]: true });
	const picture = $derived(shown('picture', shape.picture));
</script>

{#snippet pictureBlock()}
	{#if picture}
		<div class="frame">
			<img class="picture" src={picture} alt="" loading="lazy" onerror={hide('picture')} />
			{#if shape.duration}<span class="duration">{shape.duration}</span>{/if}
		</div>
	{/if}
{/snippet}

<a class="card" {href} onclick={onopen}>
	{#if shape.kind === 'post'}
		{#if shape.handle || shape.authorName}
			<span class="byline">
				{#if shown('avatar', shape.authorPicture)}
					<img class="avatar" src={shape.authorPicture} alt="" loading="lazy" onerror={hide('avatar')} />
				{/if}
				{#if shape.authorName}<span class="name">{shape.authorName}</span>{/if}
				{#if shape.handle}<span>{shape.handle}</span>{/if}
			</span>
		{/if}
		{#if shape.pictureFirst}{@render pictureBlock()}{/if}
		{#if shape.headline}<p class="text">{shape.headline}</p>{/if}
		{#if !shape.pictureFirst}{@render pictureBlock()}{/if}
		{#if shape.preview}<LinkPreviewCard preview={shape.preview} />{/if}
	{:else if shape.kind === 'page'}
		{@render pictureBlock()}
		{#if shape.site}
			<span class="byline">
				{#if shown('icon', shape.icon)}
					<img class="icon" src={shape.icon} alt="" loading="lazy" onerror={hide('icon')} />
				{/if}
				{shape.site}
			</span>
		{/if}
		<h3 class="title">{shape.headline}</h3>
		{#if shape.summary}<p class="summary">{shape.summary}</p>{/if}
		{#if shape.facts.length}<span class="facts">{shape.facts.join(' · ')}</span>{/if}
	{:else if shape.kind === 'picture'}
		{@render pictureBlock()}
	{:else}
		{#if shape.site}<span class="byline">{shape.site}</span>{/if}
		<p class="address">{shape.headline}</p>
	{/if}

	{#if shape.note}<p class="note">{shape.note}</p>{/if}
	<span class="foot">{footerOf(bookmark)}</span>
</a>

<style>
	.card {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 16px;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-xl);
		background: var(--color-surface);
		color: var(--color-foreground);
		text-decoration: none;
		transition: border-color var(--motion-hover-out) var(--ease-premium);
	}

	.card:hover {
		border-color: var(--color-border-strong);
	}

	.byline {
		display: flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
		font-size: var(--text-small);
		color: var(--color-foreground-subtle);
	}

	.byline .name {
		color: var(--color-foreground);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.avatar {
		flex: none;
		width: 24px;
		height: 24px;
		border-radius: 50%;
		object-fit: cover;
	}

	.icon {
		flex: none;
		width: 16px;
		height: 16px;
		border-radius: var(--radius-sm);
		object-fit: contain;
	}

	.foot,
	.facts {
		font-size: var(--text-small);
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}

	.frame {
		position: relative;
	}

	.picture {
		display: block;
		width: 100%;
		max-height: 420px;
		object-fit: cover;
		border-radius: var(--radius-md);
	}

	/* On the picture, so it is light ink on a dark chip in both themes. */
	.duration {
		position: absolute;
		right: 8px;
		bottom: 8px;
		padding: 2px 6px;
		border-radius: var(--radius-sm);
		background: rgb(0 0 0 / 0.75); /* design-ok: a chip on a photograph, the same in both themes */
		color: #fff; /* design-ok: ink on the dark chip above */
		font-size: var(--text-caption);
		font-variant-numeric: tabular-nums;
	}

	.text,
	.title,
	.address,
	.summary,
	.note {
		margin: 0;
	}

	.text {
		font-family: var(--font-serif);
		font-size: var(--text-body);
		line-height: 1.45;
		white-space: pre-line;
		overflow-wrap: anywhere;
		display: -webkit-box;
		-webkit-line-clamp: 6;
		line-clamp: 6;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	.title {
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: var(--text-lead);
		line-height: 1.25;
	}

	.address {
		font-size: var(--text-label);
		overflow-wrap: anywhere;
	}

	.summary {
		font-size: var(--text-small);
		line-height: 1.5;
		color: var(--color-foreground-muted);
		display: -webkit-box;
		-webkit-line-clamp: 3;
		line-clamp: 3;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	.note {
		padding: 8px 12px;
		border-radius: var(--radius-md);
		background: var(--fill-bg);
		font-size: var(--text-small);
		line-height: 1.5;
		white-space: pre-line;
	}

	@media (prefers-reduced-motion: reduce) {
		.card {
			transition: none;
		}
	}
</style>
