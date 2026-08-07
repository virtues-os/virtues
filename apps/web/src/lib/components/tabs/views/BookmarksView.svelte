<!--
	BookmarksView.svelte

	The Bookmarks room — everything you kept from the web, from every door:
	browser bookmarks synced off the Mac, GitHub stars, and URLs saved by hand.

	Server-paginated for the same reason the entity records feed is: a browser
	bookmark import is thousands of rows on day one, and the grid should hold a
	page, not a library.

	Three views. Table and Cards come from the grid; the Wall is this room's
	own — tiles pack at their natural heights and each takes the form its
	content calls for, so a saved image is recognizable without reading.

	The tile form encodes what the box knows, which is why there is no separate
	"enrichment status" badge: a thing we have looked at is a picture or a
	paragraph, and a thing we have not is a spine. Three spines among the
	pictures reads as "still working" without a word of status copy.

	Filtering by colour still waits on the image pass — palette hexes are the
	one facet nothing writes yet, and a facet over an empty column is
	furniture.
-->

<script lang="ts">
	import { Page } from '$lib';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import UniversalDataGrid, {
		type Column,
	} from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import type { GridQuery, GridPage } from '$lib/components/datagrid/types';
	import type { FilterDef } from '$lib/components/datagrid/types';
	import {
		getBookmarksPage,
		saveBookmark,
		type BookmarkApi,
		type ShelfCounts,
	} from '$lib/bookmarks/api';

	let { active: _active }: { tab?: unknown; active?: boolean } = $props();

	let url = $state('');
	let saving = $state(false);
	let saveError = $state<string | null>(null);
	// Bumped after a save so the grid refetches — a new bookmark should appear
	// without a reload.
	let revision = $state(0);

	const serverExtra = $derived({ revision });

	// Shelf-wide, not page-wide: the status line answers what the box is still
	// working on, so it must not change when a filter narrows the view.
	let counts = $state<ShelfCounts | null>(null);

	async function fetchPage(q: GridQuery): Promise<GridPage<BookmarkApi>> {
		const one = (v: unknown) => (typeof v === 'string' && v ? v : undefined);
		const page = await getBookmarksPage({
			offset: q.offset,
			limit: q.limit,
			search: q.search || undefined,
			dir: q.sort?.key === 'timestamp' && q.sort.dir === 'asc' ? 'asc' : 'desc',
			platform: one(q.filters.platform),
			bookmark_type: one(q.filters.bookmark_type),
			medium: one(q.filters.medium),
			state: one(q.filters.state),
		});
		counts = page.counts;
		return { items: page.items, total: page.total };
	}

	async function save(e: SubmitEvent) {
		e.preventDefault();
		const candidate = url.trim();
		if (!candidate || saving) return;

		saving = true;
		saveError = null;
		try {
			// Accept a bare host the way a browser bar does; the box requires a
			// scheme, and making the user type one is a papercut on the one
			// interaction this room is built around.
			const withScheme = /^https?:\/\//i.test(candidate) ? candidate : `https://${candidate}`;
			await saveBookmark({ url: withScheme });
			url = '';
			revision += 1;
		} catch (err) {
			saveError = err instanceof Error ? err.message : 'Could not save that URL';
		} finally {
			saving = false;
		}
	}

	function hostOf(raw: string): string {
		try {
			return new URL(raw).hostname.replace(/^www\./, '');
		} catch {
			return raw;
		}
	}

	function formatWhen(iso: string): string {
		const d = new Date(iso);
		// Epoch-0 is the sentinel for "the source stored no date" (Safari
		// bookmarks carry none). Saying "Jan 1, 1970" would be a fabrication.
		if (d.getTime() <= 0) return '—';
		return d.toLocaleDateString('en-US', {
			month: 'short',
			day: 'numeric',
			year: 'numeric',
		});
	}

	const columns: Column<BookmarkApi>[] = [
		{
			key: 'title',
			label: 'Title',
			icon: 'ri:bookmark-line',
			// Enrichment fills titles later; until then a saved URL has none,
			// and its host is the most honest stand-in.
			getValue: (item) => item.title || hostOf(item.url),
			sortable: false,
		},
		{
			key: 'url',
			label: 'Link',
			icon: 'ri:links-line',
			width: '14rem',
			getValue: (item) => hostOf(item.url),
			sortable: false,
		},
		{
			key: 'source_platform',
			label: 'Source',
			icon: 'ri:import-line',
			width: '8rem',
			hideOnMobile: true,
			getValue: (item) => item.source_platform ?? '—',
			sortable: false,
		},
		{
			key: 'tags',
			label: 'Tags',
			icon: 'ri:price-tag-3-line',
			width: '12rem',
			hideOnMobile: true,
			getValue: (item) => (item.tags?.length ? item.tags.join(', ') : '—'),
			sortable: false,
		},
		{
			key: 'note',
			label: 'Note',
			icon: 'ri:quill-pen-line',
			width: '12rem',
			hideOnMobile: true,
			getValue: (item) => item.note ?? '—',
			sortable: false,
		},
		{
			key: 'timestamp',
			label: 'Saved',
			icon: 'ri:time-line',
			width: '7.5rem',
			getValue: (item) => formatWhen(item.timestamp),
		},
	];

	/**
	 * Which form a tile takes. Order is the argument: the user's own words
	 * outrank anything the box wrote about the page, and a picture outranks
	 * both because it is the fastest thing to recognize.
	 *
	 * `spine` is the residue — a save we have not read yet. It is the thinnest
	 * tile on the wall on purpose: little is known, so little is shown, and
	 * the shelf tells you its own state by texture instead of by badge.
	 */
	type TileForm = 'picture' | 'quote' | 'text' | 'spine';

	function tileForm(item: BookmarkApi): TileForm {
		if (item.thumbnail_url) return 'picture';
		if (item.note?.trim()) return 'quote';
		if (item.state === 'enriched' && (item.description || item.title)) return 'text';
		return 'spine';
	}

	/**
	 * Where a save came from, in the fewest honest words.
	 *
	 * An asset-backed bookmark's `url` is the in-app viewer route, so its host
	 * is `/drive/file_…` — true, and useless to read. Provenance for those
	 * lives in `source_platform` by design (docs/bookmarks-plan.md), so that
	 * is what gets shown.
	 */
	function originLabel(item: BookmarkApi): string {
		if (item.url.startsWith('/drive/')) return item.source_platform ?? 'Saved image';
		return hostOf(item.url);
	}

	/**
	 * The title, or nothing.
	 *
	 * A screenshot has no title until something reads it, and falling back to
	 * the URL would print a storage path at people. Better to show no title
	 * than to show plumbing.
	 */
	function displayTitle(item: BookmarkApi): string | null {
		if (item.title?.trim()) return item.title;
		if (item.url.startsWith('/drive/')) return null;
		return hostOf(item.url);
	}

	/** Only said when it changes what you are looking at. */
	function stateNote(item: BookmarkApi): string | null {
		if (item.state === 'held') return 'image not read yet';
		if (item.state === 'queued') return 'not read yet';
		if (item.state === 'failed') return "couldn't be read";
		return null;
	}

	// Hardcoded rather than derived: a server page cannot honestly enumerate
	// the values across the whole shelf, and these are the doors that exist.
	const platformFilter: FilterDef<BookmarkApi> = {
		id: 'platform',
		label: 'Source',
		kind: 'enum',
		options: [
			{ value: 'safari', label: 'Safari' },
			{ value: 'chrome', label: 'Chrome' },
			{ value: 'arc', label: 'Arc' },
			{ value: 'github', label: 'GitHub' },
			{ value: 'instagram', label: 'Instagram' },
			{ value: 'web', label: 'Saved by hand' },
		],
	};

	const mediumFilter: FilterDef<BookmarkApi> = {
		id: 'medium',
		label: 'Kind',
		kind: 'enum',
		options: [
			{ value: 'article', label: 'Article' },
			{ value: 'reference', label: 'Reference' },
			{ value: 'documentation', label: 'Documentation' },
			{ value: 'repository', label: 'Repository' },
			{ value: 'product', label: 'Product' },
			{ value: 'video', label: 'Video' },
		],
	};

	const stateFilter: FilterDef<BookmarkApi> = {
		id: 'state',
		label: 'Read',
		kind: 'enum',
		options: [
			{ value: 'enriched', label: 'Read' },
			{ value: 'queued', label: 'Not read yet' },
			{ value: 'held', label: 'Waiting on images' },
		],
	};

	const typeFilter: FilterDef<BookmarkApi> = {
		id: 'bookmark_type',
		label: 'Type',
		kind: 'enum',
		options: [
			{ value: 'bookmark', label: 'Bookmark' },
			{ value: 'reading_list', label: 'Reading list' },
			{ value: 'star', label: 'Star' },
			{ value: 'save', label: 'Saved link' },
			{ value: 'screenshot', label: 'Screenshot' },
		],
	};

	const filters = [platformFilter, mediumFilter, typeFilter, stateFilter];

	/**
	 * A bookmark is a thing in the library, so clicking one opens it here
	 * rather than throwing the person out to the web. The original is one
	 * click further on, which is the right order: your note and what we made
	 * of the save first, the save itself second.
	 */
	const detailRoute = (item: BookmarkApi) => `/bookmark/${item.id}`;

	function open(item: BookmarkApi) {
		windowShellStore.openTabFromRoute(detailRoute(item));
	}
</script>

<Page
	title="Bookmarks"
	description="Everything you kept from the web — browser bookmarks, starred repositories, and links you saved by hand."
	maxWidth="wide"
>
	{#snippet actions()}
		<form class="save-row" onsubmit={save}>
			<input
				class="save-input"
				type="text"
				bind:value={url}
				placeholder="Save a link…"
				aria-label="URL to save"
				disabled={saving}
			/>
			<button class="save-button" type="submit" disabled={saving || !url.trim()}>
				<Icon icon="ri:add-line" width="16" />
				{saving ? 'Saving…' : 'Save'}
			</button>
		</form>
	{/snippet}

	{#if saveError}
		<p class="save-error" role="alert">{saveError}</p>
	{/if}

	<UniversalDataGrid
		items={[]}
		{columns}
		{filters}
		entityType="bookmarks"
		server={fetchPage}
		{serverExtra}
		defaultViewMode="wall"
		pageSize={25}
		onItemClick={open}
		rowHref={detailRoute}
		emptyIcon="ri:bookmark-line"
		emptyMessage="Nothing saved yet — paste a link above, or connect a browser."
		loadingMessage="Reading the shelf..."
		searchPlaceholder="Search bookmarks..."
	>
		{#snippet wallTile(item)}
			{@const form = tileForm(item)}
			{@const note = stateNote(item)}
			{@const title = displayTitle(item)}
			<article class="tile" class:spine={form === 'spine'}>
				{#if form === 'picture'}
					<!-- A dead og:image should cost nothing: hide the element and let
					     the tile read as the text one it would otherwise have been,
					     rather than leaving a broken-image glyph on the wall. -->
					<img
						class="tile-image"
						src={item.thumbnail_url}
						alt=""
						loading="lazy"
						onerror={(e) => ((e.currentTarget as HTMLImageElement).hidden = true)}
					/>
					{#if title}<h3 class="tile-title">{title}</h3>{/if}
				{:else if form === 'quote'}
					<!-- Her words, given the weight they earn. -->
					<blockquote class="tile-note">{item.note}</blockquote>
					{#if title}<h3 class="tile-title">{title}</h3>{/if}
				{:else if form === 'text'}
					{#if title}<h3 class="tile-title tile-title-lead">{title}</h3>{/if}
					{#if item.description}
						<p class="tile-desc">{item.description}</p>
					{/if}
				{:else if title}
					<h3 class="tile-title">{title}</h3>
				{/if}

				<footer class="tile-foot">
					<span class="tile-host">{originLabel(item)}</span>
					{#if item.medium}<span class="tile-kind">{item.medium}</span>{/if}
					{#if note}<span class="tile-state">{note}</span>{/if}
				</footer>
			</article>
		{/snippet}
	</UniversalDataGrid>

	{#if counts && (counts.queued > 0 || counts.held > 0)}
		<!-- Two numbers, not one: only the first is a backlog that drains. -->
		<p class="shelf-status">
			{#if counts.queued > 0}
				<span>{counts.queued} still to read</span>
			{/if}
			{#if counts.held > 0}
				<span>{counts.held} waiting on the image pass</span>
			{/if}
		</p>
	{/if}
</Page>

<style>
	@reference "../../../../app.css";

	.save-row {
		display: flex;
		gap: 0.5rem;
	}

	.save-input {
		flex: 1;
		min-width: 0;
		padding: 0.4rem 0.7rem;
		font: inherit;
		font-size: 0.8125rem;
		color: var(--color-foreground);
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border);
		border-radius: 0.375rem;
	}

	.save-input::placeholder {
		color: var(--color-foreground-subtle);
	}

	.save-input:focus {
		outline: none;
		border-color: var(--color-foreground-muted);
	}

	.save-button {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		white-space: nowrap;
		padding: 0.4rem 0.9rem;
		font: inherit;
		font-size: 0.8125rem;
		color: var(--color-foreground);
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border);
		border-radius: 0.375rem;
		cursor: pointer;
		transition: all 0.12s ease;
	}

	.save-button:hover:not(:disabled) {
		border-color: var(--color-foreground-muted);
	}

	.save-button:disabled {
		opacity: 0.5;
		cursor: default;
	}

	.save-error {
		margin: 0 0 0.75rem;
		font-size: 0.75rem;
		color: var(--color-danger, #dc2626);
	}

	/* ── Wall tiles ───────────────────────────────────────────────────────
	   One hairline, one surface, no shadows. The variation between tiles
	   should come from their content, so the frame stays identical. */
	.tile {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		padding: 0.875rem;
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border-subtle, var(--color-border));
		border-radius: 0.375rem;
		transition:
			border-color 0.16s ease,
			transform 0.16s cubic-bezier(0.2, 0.9, 0.3, 1.2);
	}

	.tile:hover {
		border-color: var(--color-foreground-muted);
		transform: translateY(-2px);
	}

	/* The spine: a save we have not read. Thin, quiet, and marked down the
	   left edge like a book turned outward on a shelf. */
	.tile.spine {
		gap: 0.35rem;
		padding: 0.5rem 0.75rem;
		background: transparent;
		border-color: transparent;
		border-left: 2px solid var(--color-border);
		border-radius: 0;
	}

	.tile.spine:hover {
		border-left-color: var(--color-foreground-muted);
	}

	/* Capped, and cropped rather than letterboxed. A square logo at full column
	   width takes over the wall and starves everything below it of attention —
	   the tile is a way to recognize a save, not a place to display artwork. */
	.tile-image {
		display: block;
		width: 100%;
		max-height: 15rem;
		object-fit: cover;
		border-radius: 0.25rem;
		background: var(--color-surface);
	}

	.tile-title {
		margin: 0;
		font-family: var(--font-serif);
		font-size: 0.9375rem;
		font-weight: 400;
		line-height: 1.3;
		color: var(--color-foreground);
	}

	.tile-title-lead {
		font-size: 1.0625rem;
	}

	.tile.spine .tile-title {
		font-family: var(--font-sans);
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}

	.tile-note {
		margin: 0;
		font-family: var(--font-serif);
		font-size: 1.0625rem;
		line-height: 1.4;
		color: var(--color-foreground);
	}

	.tile-desc {
		margin: 0;
		font-size: 0.8125rem;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.tile-foot {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
	}

	.tile-kind::before,
	.tile-state::before {
		content: '·';
		margin-right: 0.5rem;
	}

	.tile-state {
		font-style: italic;
	}

	.shelf-status {
		display: flex;
		gap: 1rem;
		margin: 0.75rem 0 0;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}

	@media (prefers-reduced-motion: reduce) {
		.tile {
			transition: border-color 0.16s ease;
		}
		.tile:hover {
			transform: none;
		}
	}
</style>
