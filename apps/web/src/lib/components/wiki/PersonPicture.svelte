<!--
	PersonPicture: a person's round picture, or their initials when they have
	none or it won't load. It sits beside the name, so it says nothing a reader
	of the page needs twice: hidden from assistive tech, empty alt.

	`picture` is the person's `picture` as stored. Usually that is a box path
	into the media store (`/api/drive/files/…/download`), and `backendUrl`
	routes it to the box in the Mac and phone apps' own copies; any other URL
	loads as written.
-->
<script lang="ts">
	import { backendUrl } from "$lib/config/backend";
	import { initials } from "$lib/utils/initials";

	interface Props {
		name: string;
		picture?: string | null;
		/** Diameter in pixels. */
		size?: number;
	}

	let { name, picture = null, size = 28 }: Props = $props();

	/** The picture that failed to load, so a broken one falls back to initials. */
	let failed = $state<string | null>(null);
	const src = $derived(picture && picture !== failed ? backendUrl(picture) : null);
</script>

<span class="person-picture" style:--size="{size}px" aria-hidden="true">
	{#if src}
		<img {src} alt="" loading="lazy" onerror={() => (failed = picture)} />
	{:else}
		{initials(name)}
	{/if}
</span>

<style>
	.person-picture {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		width: var(--size);
		height: var(--size);
		border-radius: 50%;
		overflow: hidden;
		box-sizing: border-box;
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border-subtle);
		color: var(--color-foreground-muted);
		font-family: var(--font-sans);
		font-size: calc(var(--size) * 0.38);
		font-weight: 500;
		letter-spacing: 0.02em;
		line-height: 1;
		user-select: none;
	}

	img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}
</style>
