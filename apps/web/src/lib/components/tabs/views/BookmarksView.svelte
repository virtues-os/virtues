<!--
	BookmarksView.svelte

	The Bookmarks room — everything you kept from the web, from every door:
	browser bookmarks synced off the Mac, GitHub stars, and URLs saved by hand.

	Server-paginated for the same reason the entity records feed is: a browser
	bookmark import is thousands of rows on day one, and the grid should hold a
	page, not a library.

	Deliberately plain for now. The bento wall, the Inbox/Library split, and
	filtering by colour all wait on the enrichment pass (docs/bookmarks-plan.md)
	— they read fields that nothing writes yet, and a facet over empty columns
	is furniture.
-->

<script lang="ts">
	import { Page } from '$lib';
	import Icon from '$lib/components/Icon.svelte';
	import UniversalDataGrid, {
		type Column,
	} from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import type { GridQuery, GridPage } from '$lib/components/datagrid/types';
	import { getBookmarksPage, saveBookmark, type BookmarkApi } from '$lib/bookmarks/api';

	let { active: _active }: { tab?: unknown; active?: boolean } = $props();

	let url = $state('');
	let saving = $state(false);
	let saveError = $state<string | null>(null);
	// Bumped after a save so the grid refetches — a new bookmark should appear
	// without a reload.
	let revision = $state(0);

	const serverExtra = $derived({ revision });

	async function fetchPage(q: GridQuery): Promise<GridPage<BookmarkApi>> {
		const page = await getBookmarksPage({
			offset: q.offset,
			limit: q.limit,
			search: q.search || undefined,
			dir: q.sort?.key === 'timestamp' && q.sort.dir === 'asc' ? 'asc' : 'desc',
		});
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

	function open(item: BookmarkApi) {
		window.open(item.url, '_blank', 'noopener,noreferrer');
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
		entityType="bookmarks"
		server={fetchPage}
		{serverExtra}
		pageSize={25}
		onItemClick={open}
		emptyIcon="ri:bookmark-line"
		emptyMessage="Nothing saved yet — paste a link above, or connect a browser."
		loadingMessage="Reading the shelf..."
		searchPlaceholder="Search bookmarks..."
	/>
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
</style>
