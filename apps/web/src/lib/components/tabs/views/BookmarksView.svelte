<!--
	BookmarksView.svelte

	The Bookmarks room: everything you saved, from every door.

	People come here to find one thing they remember, to recognize things by
	sight while scrolling, and to open the original. So search is the widest
	control, every card shows the saved thing itself (lib/bookmarks/shape.ts),
	and saves are grouped by when you saved them: people remember a save by
	when, and the groups do the work pages of 25 used to.

	Wall and List show the same saves in the same groups. The List is for
	scanning, one line per save with no pictures. The Wall places each
	card in the shortest column, left to right, so date order holds across a
	row. More load as you scroll.

	The filters are the doors a save came through, with no counts on them:
	design.md bans counts, because someone in recovery sees every page.
-->

<script lang="ts">
	import { Button, Page, EmptyState, LoadingState } from '$lib';
	import Chip from '$lib/components/Chip.svelte';
	import SearchField from '$lib/components/SearchField.svelte';
	import Segmented from '$lib/components/Segmented.svelte';
	import BookmarkCard from '$lib/components/bookmarks/BookmarkCard.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { getAssistantProfile, updateUiPreferences } from '$lib/api/client';
	import {
		getBookmarksPage,
		saveBookmark,
		type BookmarkApi,
		type BookmarkDoor,
	} from '$lib/bookmarks/api';
	import {
		DOOR_FILTERS,
		estimateHeight,
		footerOf,
		groupBySave,
		intoColumns,
		shapeOf,
	} from '$lib/bookmarks/shape';

	let { active: _active }: { tab?: unknown; active?: boolean } = $props();

	const PAGE_SIZE = 40;
	const GAP = 16;

	type View = 'wall' | 'list';
	const VIEWS: readonly { value: View; label: string }[] = [
		{ value: 'wall', label: 'Wall' },
		{ value: 'list', label: 'List' },
	];

	let view = $state<View>('wall');
	let door = $state<BookmarkDoor | null>(null);
	let search = $state('');
	let query = $state('');

	let items = $state<BookmarkApi[]>([]);
	let total = $state(0);
	let loading = $state(true);
	let loadingMore = $state(false);
	let error = $state<string | null>(null);
	const hasMore = $derived(items.length < total);

	let url = $state('');
	let saving = $state(false);
	let saveError = $state<string | null>(null);

	let width = $state(0);
	const columns = $derived(width >= 960 ? 3 : width >= 600 ? 2 : 1);
	const columnWidth = $derived(width ? (width - GAP * (columns - 1)) / columns : 320);
	const groups = $derived(groupBySave(items));

	// Wall or List is remembered per person, on the profile.
	$effect(() => {
		getAssistantProfile<{ ui_preferences?: Record<string, unknown> }>()
			.then((p) => {
				const saved = p?.ui_preferences?.bookmarks_view;
				if (saved === 'wall' || saved === 'list') view = saved;
			})
			// Unreadable: the Wall, which is the default anyway.
			.catch(() => {});
	});

	function pickView(next: View) {
		view = next;
		updateUiPreferences({ bookmarks_view: next }).catch(() => {});
	}

	// Typing settles for a moment before the server is asked.
	$effect(() => {
		const next = search.trim();
		const t = setTimeout(() => (query = next), 250);
		return () => clearTimeout(t);
	});

	// A new search or filter starts the list again. A response that arrives
	// after a newer request was sent is dropped.
	let generation = 0;
	$effect(() => {
		void query;
		void door;
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
			const page = await getBookmarksPage({
				offset: reset ? 0 : items.length,
				limit: PAGE_SIZE,
				search: query || undefined,
				door: door ?? undefined,
			});
			if (mine !== generation) return;
			// Offsets shift when a save arrives mid-scroll, so a page can repeat
			// a row already shown. Each row appears once.
			const have = new Set(reset ? [] : items.map((b) => b.id));
			const fresh = page.items.filter((b) => !have.has(b.id));
			items = reset ? page.items : [...items, ...fresh];
			total = page.total;
		} catch (e) {
			if (mine !== generation) return;
			error = e instanceof Error ? e.message : "Your server couldn't list your bookmarks. Try again.";
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
				if (entries.some((e) => e.isIntersecting) && hasMore && !loading && !loadingMore && !error) {
					load(false);
				}
			},
			{ rootMargin: '600px 0px' }
		);
		io.observe(node);
		return { destroy: () => io.disconnect() };
	}

	async function save(e: SubmitEvent) {
		e.preventDefault();
		const candidate = url.trim();
		if (!candidate || saving) return;
		saving = true;
		saveError = null;
		try {
			// Accept a bare host the way a browser bar does.
			const withScheme = /^https?:\/\//i.test(candidate) ? candidate : `https://${candidate}`;
			await saveBookmark({ url: withScheme });
			url = '';
			load(true);
		} catch (err) {
			saveError = err instanceof Error ? err.message : "Your server couldn't save that link. Try again.";
		} finally {
			saving = false;
		}
	}

	const routeOf = (b: BookmarkApi) => `/bookmark/${b.id}`;

	/** A plain click opens the bookmark in a tab here; a modified click is the browser's. */
	function open(b: BookmarkApi, e: MouseEvent) {
		if (e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return;
		e.preventDefault();
		windowShellStore.openTabFromRoute(routeOf(b));
	}
</script>

<Page
	title="Bookmarks"
	description="Everything you saved, from every app you connected."
	maxWidth="full"
>
	{#snippet actions()}
		<form class="save-row" onsubmit={save}>
			<input
				class="save-input"
				type="text"
				bind:value={url}
				placeholder="Save a link…"
				aria-label="Link to save"
				disabled={saving}
			/>
			<Button variant="secondary" type="submit" loading={saving} disabled={!url.trim()}>Save</Button>
		</form>
	{/snippet}

	{#if saveError}
		<p class="save-error" role="alert">{saveError}</p>
	{/if}

	<div class="controls">
		<div class="search">
			<SearchField bind:value={search} placeholder="Search by what you remember" />
		</div>
		<Segmented label="View" value={view} options={VIEWS} onchange={pickView} />
	</div>
	<div class="doors" role="group" aria-label="Where it came from">
		{#each DOOR_FILTERS as f (f.label)}
			<Chip selected={door === f.door} onclick={() => (door = f.door)}>{f.label}</Chip>
		{/each}
	</div>

	<div class="shelf" bind:clientWidth={width}>
		{#if loading}
			<LoadingState message="Opening your bookmarks…" />
		{:else if error}
			<EmptyState icon="mynaui:danger-circle" message={error}>
				{#snippet actions()}<Button variant="secondary" onclick={() => load(true)}>Try again</Button>{/snippet}
			</EmptyState>
		{:else if items.length === 0}
			<EmptyState
				icon="mynaui:bookmark"
				message={query
					? `Nothing you saved matches "${query}". Try a word from the page, the post or your note.`
					: door
						? 'Nothing has come through this door yet.'
						: 'Nothing saved yet. Paste a link above, or connect a browser in Sources.'}
			/>
		{:else}
			{#each groups as group, gi (group.label + gi)}
				<section class="group">
					<h2 class="group-title">{group.label}</h2>
					{#if view === 'wall'}
						<div class="wall" style:--columns={columns}>
							{#each intoColumns(group.items, columns, (b) => estimateHeight(b, columnWidth)) as column, ci (ci)}
								<div class="column">
									{#each column as b (b.id)}
										<BookmarkCard bookmark={b} href={routeOf(b)} onopen={(e) => open(b, e)} />
									{/each}
								</div>
							{/each}
						</div>
					{:else}
						<ul class="list">
							{#each group.items as b (b.id)}
								{@const s = shapeOf(b)}
								<li>
									<a class="row" href={routeOf(b)} onclick={(e) => open(b, e)}>
										<span class="row-main">
											<span class="row-title" class:serif={s.kind !== 'address'}>{s.headline ?? 'Saved picture'}</span>
											<span class="row-meta">{s.handle ?? s.site ?? ''}</span>
										</span>
										<span class="row-foot">{footerOf(b)}</span>
									</a>
								</li>
							{/each}
						</ul>
					{/if}
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
	.save-row {
		display: flex;
		gap: 8px;
	}

	.save-input {
		width: 200px;
		height: var(--control-h);
		box-sizing: border-box;
		padding: 0 12px;
		font: inherit;
		font-size: var(--text-label);
		color: var(--color-foreground);
		background: var(--color-surface);
		border: 1px solid var(--color-border-field);
		border-radius: var(--radius-lg);
	}

	.save-input::placeholder {
		color: var(--color-foreground-subtle);
	}

	.save-error {
		margin: 0 0 12px;
		font-size: var(--text-small);
		color: var(--color-error);
	}

	.controls {
		display: flex;
		align-items: center;
		gap: 16px;
	}

	.search {
		flex: 1;
		max-width: 640px;
	}

	.controls > :global(:last-child) {
		margin-left: auto;
	}

	.doors {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
		margin-top: 16px;
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
		gap: 16px;
		align-items: start;
	}

	.column {
		display: flex;
		flex-direction: column;
		gap: 16px;
		min-width: 0;
	}

	.list {
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.row {
		display: flex;
		align-items: center;
		gap: 12px;
		min-height: 52px;
		padding: 4px 8px;
		border-radius: var(--radius-md);
		color: var(--color-foreground);
		text-decoration: none;
	}

	.row:hover {
		background: var(--hover-bg);
	}

	.row-main {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}

	.row-title {
		font-size: var(--text-label);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.row-title.serif {
		font-family: var(--font-serif-ui);
		font-size: var(--text-body);
	}

	.row-meta,
	.row-foot {
		font-size: var(--text-small);
		color: var(--color-foreground-subtle);
		white-space: nowrap;
	}

	.more {
		min-height: 48px;
	}
</style>
