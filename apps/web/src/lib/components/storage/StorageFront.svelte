<!--
	StorageFront.svelte
	Storage's front page, at /storage. A person comes here to find something
	they kept, or to see what came in lately, so the page leads with a finder
	and one list of the newest things across Files, Bookmarks, App Media and
	Passwords. Each row shows the thing itself where your server has a picture
	of it, and its kind's icon where it does not.

	Recently added reads Files' top level only: the box has no endpoint for
	the newest files across every folder.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { STORAGE_MODE } from '$lib/sidebar/modes';
	import AtlasIcon from '$lib/components/sidebar/AtlasIcon.svelte';
	import {
		getDriveMedia,
		getDriveUsage,
		getLogins,
		listDriveFiles,
		type DriveUsage,
	} from '$lib/api/client';
	import { getBookmarksPage, type BookmarkApi } from '$lib/bookmarks/api';
	import { backendUrl } from '$lib/config/backend';
	import { getLocalDateSlug } from '$lib/utils/dateUtils';
	import { whenLabel } from '$lib/wiki/front';

	interface Kept {
		key: string;
		name: string;
		where: string;
		at: string;
		route: string;
		/** A picture of the thing itself, when there is one. */
		picture: string | null;
		glyph: string;
	}

	interface MediaFile {
		id: string;
		filename: string;
		mime_type: string | null;
		created_at: string;
	}

	const RECENT_MAX = 8;
	const today = getLocalDateSlug();

	/** Everything loaded, newest first; the page shows the first few. */
	let kept = $state<Kept[] | null>(null);
	const recent = $derived(kept?.slice(0, RECENT_MAX) ?? null);
	let usage = $state<DriveUsage | null>(null);

	function bookmarkRow(b: BookmarkApi): Kept {
		return {
			key: `bookmark:${b.id}`,
			name: b.title || host(b.url),
			where: `Bookmark · ${host(b.url)}`,
			at: b.timestamp,
			route: `/bookmark/${b.id}`,
			picture: b.thumbnail_url,
			glyph: 'bookmarks',
		};
	}

	const isImage = (mime: string | null) => (mime ?? '').startsWith('image/');
	const fileUrl = (id: string) => backendUrl(`/api/drive/files/${id}/download`);

	function host(url: string): string {
		try {
			return new URL(url).hostname.replace(/^www\./, '');
		} catch {
			return url;
		}
	}

	onMount(async () => {
		const [files, bookmarks, media, logins, use] = await Promise.all([
			listDriveFiles('').catch(() => []),
			getBookmarksPage({ offset: 0, limit: RECENT_MAX, dir: 'desc' }).catch(() => null),
			getDriveMedia<MediaFile[]>().catch(() => [] as MediaFile[]),
			getLogins().catch(() => []),
			getDriveUsage().catch(() => null),
		]);
		usage = use;

		const all: Kept[] = [
			...files
				.filter((f) => !f.is_folder && !f.deleted_at)
				.map((f) => ({
					key: `file:${f.id}`,
					name: f.filename,
					where: 'Files',
					at: f.created_at,
					route: '/storage/drive',
					picture: isImage(f.mime_type) ? fileUrl(f.id) : null,
					glyph: 'pages',
				})),
			...(bookmarks?.items ?? []).map(bookmarkRow),
			...media.map((m) => ({
				key: `media:${m.id}`,
				name: m.filename,
				where: 'App Media',
				at: m.created_at,
				route: '/storage/media',
				picture: isImage(m.mime_type) ? fileUrl(m.id) : null,
				glyph: 'media',
			})),
			...logins.map((l) => ({
				key: `login:${l.id}`,
				name: l.name || host(l.site),
				where: `Login · ${host(l.site)}`,
				at: l.updated_at,
				route: '/passwords',
				picture: null,
				glyph: 'passwords',
			})),
		];
		kept = all.sort((a, b) => b.at.localeCompare(a.at));
	});

	// --- Finding ---
	//
	// Files, App Media and logins are all loaded already and are matched by
	// name here. Bookmarks can run to thousands, so they are searched on the
	// box.

	let query = $state('');
	let foundBookmarks = $state<Kept[]>([]);
	let searchTimer: ReturnType<typeof setTimeout> | undefined;

	$effect(() => {
		const q = query.trim();
		clearTimeout(searchTimer);
		if (!q) {
			foundBookmarks = [];
			return;
		}
		searchTimer = setTimeout(async () => {
			const page = await getBookmarksPage({ offset: 0, limit: RECENT_MAX, search: q }).catch(() => null);
			if (query.trim() === q) foundBookmarks = (page?.items ?? []).map(bookmarkRow);
		}, 200);
	});

	const matches = $derived.by(() => {
		const q = query.trim().toLowerCase();
		if (!q || !kept) return [];
		const local = kept.filter((k) => !k.key.startsWith('bookmark:') && k.name.toLowerCase().includes(q));
		return [...local, ...foundBookmarks].slice(0, RECENT_MAX * 2);
	});

	function go(route: string, label: string) {
		windowShellStore.navigate(route, { label });
	}

	function open(k: Kept) {
		windowShellStore.openTabFromRoute(k.route);
	}

	// --- Space ---

	function bytes(n: number): string {
		if (n < 1024 ** 2) return `${Math.max(1, Math.round(n / 1024))} KB`;
		if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(0)} MB`;
		return `${(n / 1024 ** 3).toFixed(1)} GB`;
	}

	const space = $derived.by(() => {
		if (!usage || usage.quota_bytes <= 0) return null;
		const other = Math.max(0, usage.quota_bytes - usage.available_bytes - usage.total_bytes);
		const parts = [
			{ label: 'Files', size: usage.drive_bytes, tone: 'ink' },
			{ label: 'Source archives', size: usage.data_lake_bytes, tone: 'mid' },
			{ label: 'Everything else on the disk', size: other, tone: 'light' },
		].filter((p) => p.size > 0);
		return {
			used: bytes(usage.quota_bytes - usage.available_bytes),
			quota: bytes(usage.quota_bytes),
			parts: parts.map((p) => ({ ...p, pct: (p.size / usage!.quota_bytes) * 100, text: bytes(p.size) })),
		};
	});
</script>

<div class="front-wrap">
	<div class="front">
		<h1>Storage</h1>

		<form class="find" onsubmit={(e) => e.preventDefault()}>
			<label class="find-box">
				<AtlasIcon name="search" size={18} bare />
				<span class="sr-only">Find in storage</span>
				<input type="text" placeholder="Find a file, bookmark, or login" bind:value={query} autocomplete="off" />
			</label>
		</form>

		<nav class="doors" aria-label="Storage sections">
			{#each STORAGE_MODE.rows as row (row.id)}
				<button type="button" class="door" onclick={() => go(row.href, row.label)}>
					{#if row.glyph}<AtlasIcon name={row.glyph} size={16} bare />{/if}{row.label}
				</button>
			{/each}
		</nav>

		{#snippet item(k: Kept)}
			<button type="button" class="item" onclick={() => open(k)}>
				<span class="thumb" class:picture={!!k.picture}>
					{#if k.picture}
						<img src={k.picture} alt="" loading="lazy" />
					{:else}
						<AtlasIcon name={k.glyph} size={18} bare />
					{/if}
				</span>
				<span class="item-text">
					<span class="item-name">{k.name}</span>
					<span class="item-where">{k.where}</span>
				</span>
				<span class="item-when">{whenLabel(k.at.slice(0, 10), today)}</span>
			</button>
		{/snippet}

		<div class="cols">
			<section>
				<h2>{query.trim() ? 'Matches' : 'Recently added'}</h2>
				{#if recent && query.trim()}
					{#each matches as k (k.key)}{@render item(k)}{/each}
					{#if !matches.length}<p class="quiet">Nothing in storage matches “{query.trim()}”.</p>{/if}
				{:else if recent && recent.length}
					{#each recent as k (k.key)}{@render item(k)}{/each}
					<button type="button" class="more" onclick={() => go('/storage/drive', 'Files')}>All files →</button>
				{:else if recent}
					<p class="quiet">Files you add, pages you bookmark and logins you save show up here.</p>
				{/if}
			</section>

			<aside>
				{#if space}
					<h2>Space on your server</h2>
					<p class="used">{space.used} used of {space.quota}</p>
					<div class="bar" role="img" aria-label={space.parts.map((p) => `${p.label} ${p.text}`).join(', ')}>
						{#each space.parts as p (p.label)}
							<span class="seg {p.tone}" style:width="{p.pct}%"></span>
						{/each}
					</div>
					<ul class="legend">
						{#each space.parts as p (p.label)}
							<li><span class="sw {p.tone}"></span>{p.label}<span class="size">{p.text}</span></li>
						{/each}
					</ul>
				{/if}

				<h2 class="deleted-head">Recently deleted</h2>
				<p class="deleted">
					Deleted chats, pages, projects and files wait here for 30 days, so you can put them back.
				</p>
				<button type="button" class="more" onclick={() => go('/storage/trash', 'Recently deleted')}>
					Open recently deleted →
				</button>
			</aside>
		</div>
	</div>
</div>

<style>
	/* The same box model as the wiki's room: padding on the scroller, the
	   measure centred inside it. */
	.front-wrap {
		height: 100%;
		overflow-y: auto;
		padding: 2rem 1.25rem calc(1.5rem + env(safe-area-inset-bottom));
	}

	@media (min-width: 768px) {
		.front-wrap {
			padding: 3rem;
		}
	}

	.front {
		max-width: 60rem;
		margin: 0 auto;
		padding-bottom: 2rem;
	}

	h1 {
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: 2.625rem;
		line-height: 1.1;
		margin: 0 0 1.375rem;
		color: var(--color-foreground);
	}

	h2 {
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: 1.5625rem;
		margin: 0 0 0.5rem;
		color: var(--color-foreground);
	}

	button {
		font: inherit;
		color: inherit;
		background: none;
		border: none;
		padding: 0;
		text-align: left;
		cursor: pointer;
	}

	.find-box {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		height: 3rem;
		padding: 0 1rem;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-xl);
		color: var(--color-foreground-subtle);
		background: var(--color-surface);
	}

	.find-box:focus-within {
		border-color: var(--color-border-strong);
	}

	.find-box input {
		flex: 1;
		min-width: 0;
		border: none;
		outline: none;
		background: transparent;
		font: inherit;
		font-size: 1rem;
		color: var(--color-foreground);
	}

	.doors {
		display: flex;
		flex-wrap: wrap;
		gap: 0.625rem 1.375rem;
		margin-top: 1.125rem;
	}

	.door {
		display: inline-flex;
		align-items: center;
		gap: 0.4375rem;
		font-size: 0.9375rem;
		color: var(--color-foreground-muted);
	}

	.door:hover {
		color: var(--color-foreground);
	}

	.cols {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 17rem;
		gap: 4.5rem;
		margin-top: 3.25rem;
	}

	.item {
		display: flex;
		align-items: center;
		gap: 0.875rem;
		width: 100%;
		padding: 0.6875rem 0;
		border-bottom: 1px solid var(--color-border-subtle);
	}

	.thumb {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 2.75rem;
		height: 2.75rem;
		flex: none;
		border-radius: var(--radius-md);
		overflow: hidden;
		background: var(--color-background-hover);
		color: var(--color-foreground-muted);
	}

	.thumb.picture {
		border: 1px solid var(--color-border);
		background: var(--color-surface);
	}

	.thumb img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.item-text {
		display: block;
		flex: 1;
		min-width: 0;
	}

	.item-name {
		display: block;
		font-size: 0.9375rem;
		line-height: 1.3;
		color: var(--color-foreground);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.item:hover .item-name {
		color: var(--color-foreground-muted);
	}

	.item-where,
	.item-when {
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
	}

	.item-where {
		display: block;
		margin-top: 0.125rem;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.item-when {
		white-space: nowrap;
	}

	.more {
		display: inline-flex;
		margin-top: 0.875rem;
		font-size: 0.875rem;
		color: var(--color-foreground-muted);
	}

	.more:hover {
		color: var(--color-foreground);
	}

	.quiet {
		margin: 0.75rem 0 0;
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
	}

	.used {
		margin: 0.375rem 0 0.875rem;
		font-size: 0.875rem;
		color: var(--color-foreground-muted);
	}

	.bar {
		display: flex;
		height: 8px;
		border-radius: var(--radius-sm);
		overflow: hidden;
		background: var(--color-border-subtle);
	}

	.seg.ink,
	.sw.ink {
		background: var(--color-foreground);
	}

	.seg.mid,
	.sw.mid {
		background: var(--color-foreground-subtle);
	}

	.seg.light,
	.sw.light {
		background: var(--color-border-strong);
	}

	.legend {
		list-style: none;
		margin: 0.875rem 0 0;
		padding: 0;
	}

	.legend li {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		height: 2.125rem;
		font-size: 0.875rem;
		color: var(--color-foreground);
	}

	.sw {
		width: 10px;
		height: 10px;
		flex: none;
		border-radius: var(--radius-xs);
	}

	.size {
		margin-left: auto;
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
	}

	.deleted-head {
		margin-top: 2.75rem;
	}

	.deleted {
		margin: 0.375rem 0 0;
		font-family: var(--font-serif);
		font-size: 1.125rem;
		line-height: 1.42;
		color: var(--color-foreground);
	}

	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip: rect(0 0 0 0);
		white-space: nowrap;
	}

	@media (max-width: 767px) {
		.cols {
			grid-template-columns: minmax(0, 1fr);
			gap: 3rem;
		}
	}
</style>
