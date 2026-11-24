<!--
	LinkPreviewCard.svelte

	The page a post links to: its picture, its site and its title. Shown in
	place of the short link (t.co) the post's text carried, so a person sees
	where the link leads before opening it.

	Drawn as a box inside the card. On the Wall the card itself is the link to
	the bookmark, so the preview there is not a second link: `href` makes it one
	on the bookmark's page.
-->
<script lang="ts">
	import type { LinkPreview } from '$lib/bookmarks/shape';

	let { preview, href }: { preview: LinkPreview; href?: string } = $props();
	let pictureFailed = $state(false);
</script>

<svelte:element
	this={href ? 'a' : 'div'}
	class="preview"
	{href}
	target={href ? '_blank' : undefined}
	rel={href ? 'noopener noreferrer' : undefined}
>
	{#if preview.picture && !pictureFailed}
		<img src={preview.picture} alt="" loading="lazy" onerror={() => (pictureFailed = true)} />
	{/if}
	<span class="words">
		<span class="site">{preview.site}</span>
		{#if preview.title}<span class="title">{preview.title}</span>{/if}
	</span>
</svelte:element>

<style>
	.preview {
		display: flex;
		align-items: stretch;
		min-width: 0;
		overflow: hidden;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		color: var(--color-foreground);
		text-decoration: none;
	}

	a.preview:hover {
		border-color: var(--color-border-strong);
	}

	img {
		flex: none;
		width: 72px;
		object-fit: cover;
	}

	.words {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
		padding: 8px 12px;
	}

	.site {
		font-size: var(--text-small);
		color: var(--color-foreground-subtle);
	}

	.title {
		font-size: var(--text-small);
		line-height: 1.4;
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}
</style>
