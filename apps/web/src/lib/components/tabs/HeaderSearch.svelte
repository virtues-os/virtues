<script lang="ts">
	/**
	 * Search, in the right corner of the window header beside the status
	 * capsule. It opens the search palette (SearchModal), as ⌘K does. It
	 * searches everything, so it lives in the window's corner rather than on
	 * a page's toolbar, where it would read as a search of that page.
	 */
	import AtlasIcon from '$lib/components/sidebar/AtlasIcon.svelte';
	import { search } from '$lib/stores/search.svelte';
</script>

<button type="button" class="header-search" onclick={() => search.show()} aria-label="Search" data-tip="Search (⌘K)">
	<AtlasIcon name="search" size={15} bare />
</button>

<style>
	/* The capsule's resting shape and glass, so the corner reads as a pair.
	   Both fall back to the solid surface where the theme sets no glass. */
	.header-search {
		width: 30px;
		height: 30px;
		box-sizing: border-box;
		border: 1px solid var(--color-border);
		border-radius: 50%;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		color: var(--color-foreground-muted);
		background: var(--glass-bg, var(--color-surface));
		backdrop-filter: var(--glass-blur, none);
		-webkit-backdrop-filter: var(--glass-blur, none);
		cursor: pointer;
		transition:
			border-color var(--motion-hover-out) var(--ease-premium),
			color var(--motion-hover-out) var(--ease-premium);
	}

	.header-search:hover {
		border-color: var(--color-border-strong);
		color: var(--color-foreground);
		transition-duration: var(--motion-hover-in);
	}

	.header-search:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}

	@media (prefers-reduced-motion: reduce) {
		.header-search {
			transition: none;
		}
	}
</style>
