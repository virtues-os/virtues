<!--
	PhotosView.svelte

	Photo favorites: the photos you favorited on your iPhone, beside Bookmarks.

	People come here to find one photo they remember, or to look. So the page
	is the pictures and one search, which matches what is in a photo, your
	note, and the month it was taken. Photos are grouped by month, placed left
	to right so date order reads across a row, and more load as you scroll.
	There are no counts: design.md bans them, because someone in recovery sees
	every page.
-->

<script lang="ts">
	import { Button, Page, EmptyState, LoadingState } from '$lib';
	import SearchField from '$lib/components/SearchField.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { intoColumns } from '$lib/bookmarks/shape';
	import { getPhotos, type PhotoApi } from '$lib/photos/api';
	import { durationOf, groupByMonth, pictureSrc, ratioOf, titleOf } from '$lib/photos/shape';

	let { active: _active }: { tab?: unknown; active?: boolean } = $props();

	const PAGE_SIZE = 60;

	let search = $state('');
	let query = $state('');
	let items = $state<PhotoApi[]>([]);
	let hasMore = $state(false);
	let loading = $state(true);
	let loadingMore = $state(false);
	let error = $state<string | null>(null);
	let failed = $state<Record<string, boolean>>({});

	let width = $state(0);
	const columns = $derived(width >= 1100 ? 5 : width >= 800 ? 4 : width >= 520 ? 3 : 2);
	const groups = $derived(groupByMonth(items));

	// Typing settles for a moment before the server is asked.
	$effect(() => {
		const next = search.trim();
		const t = setTimeout(() => (query = next), 250);
		return () => clearTimeout(t);
	});

	// A new search starts the list again. A response that arrives after a
	// newer request was sent is dropped.
	let generation = 0;
	$effect(() => {
		void query;
		load(true);
	});

	async function load(reset: boolean) {
		const mine = reset ? ++generation : generation;
		if (reset) {
			loading = true;
			error = null;
		} else {
			loadingMore = true;
		}
		try {
			const page = await getPhotos({
				offset: reset ? 0 : items.length,
				limit: PAGE_SIZE,
				search: query || undefined,
			});
			if (mine !== generation) return;
			// A photo that arrives mid-scroll shifts the offsets, so a page can
			// repeat one already shown. Each appears once.
			const have = new Set(reset ? [] : items.map((p) => p.id));
			const fresh = page.items.filter((p) => !have.has(p.id));
			items = reset ? page.items : [...items, ...fresh];
			hasMore = page.has_more && (reset || fresh.length > 0);
		} catch (e) {
			if (mine !== generation) return;
			error = e instanceof Error ? e.message : "Your server couldn't list your photos. Try again.";
		} finally {
			if (mine === generation) {
				loading = false;
				loadingMore = false;
			}
		}
	}

	// Load the next page when the end of the list comes into view.
	function moreOnScroll(node: HTMLElement) {
		const io = new IntersectionObserver(
			(entries) => {
				if (entries.some((e) => e.isIntersecting) && hasMore && !loading && !loadingMore && !error) load(false);
			},
			{ rootMargin: '800px 0px' }
		);
		io.observe(node);
		return { destroy: () => io.disconnect() };
	}

	const routeOf = (p: PhotoApi) => `/photo/${p.id}`;

	/** A plain click opens the photo in a tab here; a modified click is the browser's. */
	function open(p: PhotoApi, e: MouseEvent) {
		if (e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return;
		e.preventDefault();
		windowShellStore.openTabFromRoute(routeOf(p), { label: titleOf(p) });
	}
</script>

<Page title="Photo favorites" description="The photos you favorited on your iPhone." maxWidth="full">
	<div class="search">
		<SearchField bind:value={search} placeholder="Search by what you remember" />
	</div>

	<div class="shelf" bind:clientWidth={width}>
		{#if loading}
			<LoadingState message="Opening your photos…" />
		{:else if error}
			<EmptyState icon="mynaui:danger-circle" message={error}>
				{#snippet actions()}<Button variant="secondary" onclick={() => load(true)}>Try again</Button>{/snippet}
			</EmptyState>
		{:else if items.length === 0}
			<EmptyState
				icon="mynaui:image"
				message={query
					? `No photo matches "${query}". Try something in it, your note, or the month.`
					: 'No photos yet. Turn on Photo favorites in the iPhone app, then favorite a photo in Photos.'}
			/>
		{:else}
			{#each groups as group, gi (group.label + gi)}
				<section>
					<h2 class="group-title">{group.label}</h2>
					<div class="wall" style:--columns={columns}>
						{#each intoColumns(group.items, columns, ratioOf) as column, ci (ci)}
							<div class="column">
								{#each column as p (p.id)}
									{@const src = pictureSrc(p)}
									{@const length = durationOf(p)}
									<a
										class="photo"
										href={routeOf(p)}
										style:aspect-ratio={`1 / ${ratioOf(p)}`}
										onclick={(e) => open(p, e)}
									>
										{#if src && !failed[p.id]}
											<img
												{src}
												alt={p.description ?? titleOf(p)}
												loading="lazy"
												decoding="async"
												onerror={() => (failed = { ...failed, [p.id]: true })}
											/>
										{:else}
											<span class="missing">{titleOf(p)}</span>
										{/if}
										{#if length}<span class="duration">{length}</span>{/if}
									</a>
								{/each}
							</div>
						{/each}
					</div>
				</section>
			{/each}
			{#if hasMore}
				<div class="more" use:moreOnScroll>
					{#if loadingMore}<LoadingState />{/if}
				</div>
			{/if}
		{/if}
	</div>
</Page>

<style>
	.search {
		max-width: 640px;
	}

	.shelf {
		margin-top: 8px;
	}

	.group-title {
		margin: 32px 0 16px;
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: var(--text-heading);
		color: var(--color-foreground);
	}

	.wall {
		display: grid;
		grid-template-columns: repeat(var(--columns), minmax(0, 1fr));
		gap: 8px;
		align-items: start;
	}

	.column {
		display: flex;
		flex-direction: column;
		gap: 8px;
		min-width: 0;
	}

	.photo {
		position: relative;
		display: block;
		overflow: hidden;
		border-radius: var(--radius-md);
		background: var(--fill-bg);
		color: var(--color-foreground-subtle);
		text-decoration: none;
	}

	.photo img {
		display: block;
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.missing {
		display: block;
		padding: 12px;
		font-size: var(--text-small);
	}

	/* On the picture, so it is light ink on a dark chip in both themes. */
	.duration {
		position: absolute;
		right: 8px;
		bottom: 8px;
		padding: 4px 8px;
		border-radius: var(--radius-sm);
		background: rgb(0 0 0 / 0.75); /* design-ok: a chip on a photograph, the same in both themes */
		color: #fff; /* design-ok: ink on the dark chip above */
		font-size: var(--text-small);
		font-variant-numeric: tabular-nums;
	}

	.more {
		min-height: 48px;
	}
</style>
