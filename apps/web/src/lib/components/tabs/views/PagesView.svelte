<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import type { PageSummary } from "$lib/api/client";
	import { spaceStore } from "$lib/stores/space.svelte";
	import { pagesStore } from "$lib/stores/pages.svelte";
	import { contextMenu } from "$lib/stores/contextMenu.svelte";
	import type { ContextMenuItem } from "$lib/stores/contextMenu.svelte";
	import { getWorkspaceMenuItems } from "$lib/utils/contextMenuItems";
	import { Page } from "$lib";
	import { onMount } from "svelte";
	import Icon from "$lib/components/Icon.svelte";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// View mode types
	type ViewMode = "list" | "table" | "gallery";
	type SortBy = "updated" | "created" | "title";
	type SortDir = "asc" | "desc";

	// Persisted state keys
	const VIEW_MODE_KEY = "virtues-pages-view-mode";
	const SORT_KEY = "virtues-pages-sort";

	// Load persisted preferences
	function loadPreferences() {
		try {
			const savedMode = localStorage.getItem(VIEW_MODE_KEY);
			if (savedMode && ["list", "table", "gallery"].includes(savedMode)) {
				viewMode = savedMode as ViewMode;
			}
			const savedSort = localStorage.getItem(SORT_KEY);
			if (savedSort) {
				const parsed = JSON.parse(savedSort);
				if (parsed.by) sortBy = parsed.by;
				if (parsed.dir) sortDir = parsed.dir;
			}
		} catch {}
	}

	function savePreferences() {
		localStorage.setItem(VIEW_MODE_KEY, viewMode);
		localStorage.setItem(
			SORT_KEY,
			JSON.stringify({ by: sortBy, dir: sortDir }),
		);
	}

	// State
	let viewMode = $state<ViewMode>("list");
	let sortBy = $state<SortBy>("updated");
	let sortDir = $state<SortDir>("desc");
	let creating = $state(false);
	let searchQuery = $state("");

	// Use store state
	const pages = $derived(pagesStore.pages);
	const loading = $derived(pagesStore.pagesLoading);
	const error = $derived(pagesStore.pagesError);

	// Filter and sort pages
	const filteredPages = $derived(() => {
		let result = [...pages];

		// Filter by search
		if (searchQuery.trim()) {
			const q = searchQuery.toLowerCase();
			result = result.filter(
				(p) =>
					p.title.toLowerCase().includes(q) ||
					p.tags?.toLowerCase().includes(q),
			);
		}

		// Sort
		result.sort((a, b) => {
			let cmp = 0;
			if (sortBy === "updated") {
				cmp =
					new Date(b.updated_at).getTime() -
					new Date(a.updated_at).getTime();
			} else if (sortBy === "created") {
				cmp =
					new Date(b.created_at).getTime() -
					new Date(a.created_at).getTime();
			} else if (sortBy === "title") {
				cmp = a.title.localeCompare(b.title);
			}
			return sortDir === "desc" ? cmp : -cmp;
		});

		return result;
	});

	onMount(async () => {
		loadPreferences();
		await pagesStore.loadPages();
	});

	// Save preferences when they change
	$effect(() => {
		viewMode;
		sortBy;
		sortDir;
		savePreferences();
	});

	function formatDate(dateStr: string): string {
		const date = new Date(dateStr);
		const now = new Date();
		const diffMs = now.getTime() - date.getTime();
		const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

		if (diffDays === 0) {
			return date.toLocaleTimeString("en-US", {
				hour: "numeric",
				minute: "2-digit",
			});
		} else if (diffDays === 1) {
			return "Yesterday";
		} else if (diffDays < 7) {
			return date.toLocaleDateString("en-US", { weekday: "long" });
		} else {
			return date.toLocaleDateString("en-US", {
				month: "short",
				day: "numeric",
				year:
					now.getFullYear() !== date.getFullYear()
						? "numeric"
						: undefined,
			});
		}
	}

	function formatFullDate(dateStr: string): string {
		return new Date(dateStr).toLocaleDateString("en-US", {
			month: "short",
			day: "numeric",
			year: "numeric",
			hour: "numeric",
			minute: "2-digit",
		});
	}

	function parseTags(tagsJson: string | null): string[] {
		if (!tagsJson) return [];
		try {
			return JSON.parse(tagsJson);
		} catch {
			return [];
		}
	}

	function getPageIcon(page: { icon: string | null }): string {
		return page.icon || "ri:file-text-line";
	}

	function handlePageClick(pageId: string, title: string, e: MouseEvent) {
		pagesStore.markAsRecent(pageId);
		const forceNew = e.metaKey || e.ctrlKey;
		spaceStore.openTabFromRoute(`/page/${pageId}`, {
			forceNew,
			label: title,
			preferEmptyPane: true,
		});
	}

	function handleContextMenu(e: MouseEvent, page: PageSummary) {
		e.preventDefault();

		const items: ContextMenuItem[] = [
			{
				id: "open-new-tab",
				label: "Open in New Tab",
				icon: "ri:external-link-line",
				action: () => {
					spaceStore.openTabFromRoute(`/page/${page.id}`, {
						forceNew: true,
						label: page.title,
						preferEmptyPane: true,
					});
				},
			},
			...getWorkspaceMenuItems(`/page/${page.id}`),
			{
				id: "delete",
				label: "Delete",
				icon: "ri:delete-bin-line",
				variant: "destructive",
				dividerBefore: true,
				action: async () => {
					if (confirm("Delete this page?")) {
						await pagesStore.removePage(page.id);
					}
				},
			},
		];

		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}

	function toggleSort(column: SortBy) {
		if (sortBy === column) {
			sortDir = sortDir === "desc" ? "asc" : "desc";
		} else {
			sortBy = column;
			sortDir = column === "title" ? "asc" : "desc";
		}
	}

	async function createNewPage() {
		if (creating) return;
		creating = true;

		try {
			// Use store method - handles API call, cache invalidation, sidebar refresh
			const page = await pagesStore.createNewPage("Untitled");
			pagesStore.addPage(page);
			pagesStore.markAsRecent(page.id);

			spaceStore.openTabFromRoute(`/page/${page.id}`, {
				label: page.title,
				preferEmptyPane: true,
			});
		} catch (err) {
			console.error("Failed to create page:", err);
		} finally {
			creating = false;
		}
	}
</script>

<Page>
	<div class="pages-container">
		<!-- Header -->
		<div class="header">
			<div>
				<h1 class="title">Pages</h1>
				<p class="subtitle">
					{pages.length} page{pages.length !== 1 ? "s" : ""}
				</p>
			</div>
			<button
				onclick={createNewPage}
				disabled={creating}
				class="new-page-btn"
			>
				<Icon icon="ri:add-line" width="18" />
				New Page
			</button>
		</div>

		<!-- Toolbar: Search + View Mode -->
		<div class="toolbar">
			<div class="search-container">
				<Icon icon="ri:search-line" class="search-icon" width="18" />
				<input
					type="text"
					bind:value={searchQuery}
					placeholder="Search pages..."
					class="search-input"
				/>
				{#if searchQuery}
					<button
						onclick={() => (searchQuery = "")}
						class="search-clear"
					>
						<Icon icon="ri:close-line" width="18" />
					</button>
				{/if}
			</div>

			<div class="view-toggles">
				<button
					onclick={() => (viewMode = "list")}
					class="view-toggle"
					class:active={viewMode === "list"}
					title="List view"
				>
					<Icon icon="ri:list-check" width="18" />
				</button>
				<button
					onclick={() => (viewMode = "table")}
					class="view-toggle"
					class:active={viewMode === "table"}
					title="Table view"
				>
					<Icon icon="ri:table-line" width="18" />
				</button>
				<button
					onclick={() => (viewMode = "gallery")}
					class="view-toggle"
					class:active={viewMode === "gallery"}
					title="Gallery view"
				>
					<Icon icon="ri:layout-grid-line" width="18" />
				</button>
			</div>
		</div>

		<!-- Content -->
		{#if loading}
			<div class="flex items-center justify-center h-full">
				<Icon icon="ri:loader-4-line" width="20" class="spin" />
			</div>
		{:else if error}
			<div class="error-state">{error}</div>
		{:else if pages.length === 0}
			<div class="empty-state">
				<p>No pages yet</p>
				<button
					onclick={createNewPage}
					disabled={creating}
					class="create-link"
				>
					Create your first page
				</button>
			</div>
		{:else if filteredPages().length === 0}
			<div class="empty-state">
				<Icon icon="ri:search-line" width="48" class="empty-icon" />
				<p>No pages matching "{searchQuery}"</p>
			</div>
		{:else if viewMode === "list"}
			<!-- List View -->
			<ul class="list-view">
				{#each filteredPages() as page (page.id)}
					{@const tags = parseTags(page.tags)}
					<li>
						<button
							onclick={(e) =>
								handlePageClick(page.id, page.title, e)}
							oncontextmenu={(e) => handleContextMenu(e, page)}
							class="list-item"
						>
							<Icon
								icon={getPageIcon(page)}
								class="item-icon"
								width="18"
							/>
							<span class="item-title">{page.title}</span>
							{#if tags.length > 0}
								<div class="item-tags">
									{#each tags.slice(0, 3) as tag}
										<span class="tag">{tag}</span>
									{/each}
									{#if tags.length > 3}
										<span class="tag-more"
											>+{tags.length - 3}</span
										>
									{/if}
								</div>
							{/if}
							<span class="item-date"
								>{formatDate(page.updated_at)}</span
							>
						</button>
					</li>
				{/each}
			</ul>
		{:else if viewMode === "table"}
			<!-- Table View -->
			<div class="table-container">
				<table class="table-view">
					<thead>
						<tr>
							<th class="th-icon"></th>
							<th
								class="th-title sortable"
								onclick={() => toggleSort("title")}
							>
								Title
								{#if sortBy === "title"}
									<Icon
										icon={sortDir === "asc"
											? "ri:arrow-up-s-line"
											: "ri:arrow-down-s-line"}
										width="14"
									/>
								{/if}
							</th>
							<th class="th-tags">Tags</th>
							<th
								class="th-date sortable"
								onclick={() => toggleSort("updated")}
							>
								Updated
								{#if sortBy === "updated"}
									<Icon
										icon={sortDir === "asc"
											? "ri:arrow-up-s-line"
											: "ri:arrow-down-s-line"}
										width="14"
									/>
								{/if}
							</th>
							<th
								class="th-date sortable"
								onclick={() => toggleSort("created")}
							>
								Created
								{#if sortBy === "created"}
									<Icon
										icon={sortDir === "asc"
											? "ri:arrow-up-s-line"
											: "ri:arrow-down-s-line"}
										width="14"
									/>
								{/if}
							</th>
						</tr>
					</thead>
					<tbody>
						{#each filteredPages() as page (page.id)}
							{@const tags = parseTags(page.tags)}
							<tr
								onclick={(e) =>
									handlePageClick(page.id, page.title, e)}
								oncontextmenu={(e) =>
									handleContextMenu(e, page)}
								class="table-row"
							>
								<td class="td-icon">
									<Icon
										icon={getPageIcon(page)}
										class="item-icon"
										width="16"
									/>
								</td>
								<td class="td-title">{page.title}</td>
								<td class="td-tags">
									{#if tags.length > 0}
										<div class="item-tags">
											{#each tags.slice(0, 2) as tag}
												<span class="tag">{tag}</span>
											{/each}
											{#if tags.length > 2}
												<span class="tag-more"
													>+{tags.length - 2}</span
												>
											{/if}
										</div>
									{/if}
								</td>
								<td class="td-date"
									>{formatDate(page.updated_at)}</td
								>
								<td class="td-date"
									>{formatDate(page.created_at)}</td
								>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		{:else if viewMode === "gallery"}
			<!-- Gallery View -->
			<div class="gallery-view">
				{#each filteredPages() as page (page.id)}
					{@const tags = parseTags(page.tags)}
					<button
						onclick={(e) => handlePageClick(page.id, page.title, e)}
						oncontextmenu={(e) => handleContextMenu(e, page)}
						class="gallery-card"
					>
						<div
							class="card-cover"
							style={page.cover_url
								? `background-image: url(${page.cover_url})`
								: ""}
						>
							{#if !page.cover_url}
								<Icon
									icon={getPageIcon(page)}
									width="32"
									class="cover-placeholder-icon"
								/>
							{/if}
						</div>
						<div class="card-content">
							<div class="card-title-row">
								<Icon
									icon={getPageIcon(page)}
									width="16"
									class="card-icon"
								/>
								<span class="card-title">{page.title}</span>
							</div>
							{#if tags.length > 0}
								<div class="card-tags">
									{#each tags.slice(0, 3) as tag}
										<span class="tag">{tag}</span>
									{/each}
								</div>
							{/if}
							<div class="card-date">
								{formatDate(page.updated_at)}
							</div>
						</div>
					</button>
				{/each}
			</div>
		{/if}
	</div>
</Page>

<style>
	.pages-container {
		max-width: 1000px;
		padding: 0 1rem;
	}

	/* Header */
	.header {
		display: flex;
		justify-content: space-between;
		align-items: flex-start;
		margin-bottom: 1.5rem;
	}

	.title {
		font-size: 1.875rem;
		font-family: var(--font-serif);
		font-weight: 500;
		color: var(--color-foreground);
		margin-bottom: 0.5rem;
	}

	.subtitle {
		color: var(--color-foreground-muted);
		font-size: 0.875rem;
	}

	.new-page-btn {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.5rem 1rem;
		background: var(--color-foreground);
		color: var(--color-background);
		border: none;
		border-radius: 0.5rem;
		font-size: 0.875rem;
		font-weight: 500;
		cursor: pointer;
		transition: opacity 150ms;
	}

	.new-page-btn:hover {
		opacity: 0.9;
	}

	.new-page-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	/* Toolbar */
	.toolbar {
		display: flex;
		gap: 1rem;
		margin-bottom: 1.5rem;
		align-items: center;
	}

	.search-container {
		flex: 1;
		position: relative;
	}

	.search-container :global(.search-icon) {
		position: absolute;
		left: 0.75rem;
		top: 50%;
		transform: translateY(-50%);
		color: var(--color-foreground-subtle);
	}

	.search-input {
		width: 100%;
		padding: 0.625rem 2.5rem 0.625rem 2.5rem;
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border);
		border-radius: 0.5rem;
		color: var(--color-foreground);
		font-size: 0.875rem;
	}

	.search-input::placeholder {
		color: var(--color-foreground-subtle);
	}

	.search-input:focus {
		outline: none;
		border-color: var(--color-primary);
		box-shadow: 0 0 0 2px
			color-mix(in srgb, var(--color-primary) 20%, transparent);
	}

	.search-clear {
		position: absolute;
		right: 0.75rem;
		top: 50%;
		transform: translateY(-50%);
		background: none;
		border: none;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		padding: 0;
	}

	.search-clear:hover {
		color: var(--color-foreground);
	}

	.view-toggles {
		display: flex;
		gap: 2px;
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border);
		border-radius: 0.5rem;
		padding: 2px;
	}

	.view-toggle {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 32px;
		height: 32px;
		border: none;
		background: transparent;
		color: var(--color-foreground-muted);
		border-radius: 0.375rem;
		cursor: pointer;
		transition: all 150ms;
	}

	.view-toggle:hover {
		color: var(--color-foreground);
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
	}

	.view-toggle.active {
		background: var(--color-background);
		color: var(--color-foreground);
		box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05);
	}

	/* Empty/Error States */
	.empty-state {
		text-align: center;
		padding: 3rem 1rem;
		color: var(--color-foreground-muted);
	}

	.empty-state :global(.empty-icon) {
		color: var(--color-foreground-subtle);
		margin-bottom: 1rem;
	}

	.create-link {
		background: none;
		border: none;
		color: var(--color-primary);
		cursor: pointer;
		font-size: inherit;
	}

	.create-link:hover {
		text-decoration: underline;
	}

	.error-state {
		padding: 1rem;
		background: color-mix(in srgb, var(--color-error) 10%, transparent);
		border: 1px solid var(--color-error);
		border-radius: 0.5rem;
		color: var(--color-error);
	}

	/* Tags */
	.item-tags,
	.card-tags {
		display: flex;
		gap: 0.25rem;
		flex-wrap: wrap;
	}

	.tag {
		padding: 0.125rem 0.5rem;
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
		border-radius: 9999px;
		font-size: 0.6875rem;
		color: var(--color-foreground-muted);
	}

	.tag-more {
		padding: 0.125rem 0.375rem;
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
	}

	/* List View */
	.list-view {
		list-style: none;
		padding: 0;
		margin: 0;
	}

	.list-item {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		width: 100%;
		padding: 0.75rem;
		margin: 0 -0.75rem;
		background: none;
		border: none;
		border-radius: 0.5rem;
		cursor: pointer;
		text-align: left;
		transition: background 150ms;
	}

	.list-item:hover {
		background: var(--color-surface-elevated);
	}

	.list-item :global(.item-icon) {
		color: var(--color-foreground-subtle);
		flex-shrink: 0;
	}

	.list-item:hover :global(.item-icon) {
		color: var(--color-foreground);
	}

	.list-item :global(.item-icon.pinned) {
		color: var(--color-primary);
	}

	.item-title {
		flex: 1;
		color: var(--color-foreground);
		font-size: 0.9375rem;
	}

	.list-item:hover .item-title {
		color: var(--color-primary);
	}

	.item-date {
		color: var(--color-foreground-subtle);
		font-size: 0.8125rem;
		flex-shrink: 0;
	}

	/* Table View */
	.table-container {
		overflow-x: auto;
	}

	.table-view {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.875rem;
	}

	.table-view th {
		padding: 0.75rem;
		text-align: left;
		font-weight: 500;
		color: var(--color-foreground-muted);
		border-bottom: 1px solid var(--color-border);
		white-space: nowrap;
	}

	.table-view th.sortable {
		cursor: pointer;
		user-select: none;
	}

	.table-view th.sortable:hover {
		color: var(--color-foreground);
	}

	.th-icon {
		width: 40px;
	}
	.th-title {
		min-width: 200px;
	}
	.th-tags {
		min-width: 150px;
	}
	.th-date {
		width: 120px;
	}

	.table-row {
		cursor: pointer;
		transition: background 150ms;
	}

	.table-row:hover {
		background: var(--color-surface-elevated);
	}

	.table-view td {
		padding: 0.75rem;
		border-bottom: 1px solid var(--color-border);
		vertical-align: middle;
	}

	.td-icon :global(.item-icon) {
		color: var(--color-foreground-subtle);
	}

	.td-icon :global(.item-icon.pinned) {
		color: var(--color-primary);
	}

	.td-title {
		color: var(--color-foreground);
		font-weight: 500;
	}

	.table-row:hover .td-title {
		color: var(--color-primary);
	}

	.td-tags .item-tags {
		justify-content: flex-start;
	}

	.td-date {
		color: var(--color-foreground-subtle);
		white-space: nowrap;
	}

	/* Gallery View */
	.gallery-view {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
		gap: 1rem;
	}

	.gallery-card {
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border);
		border-radius: 0.75rem;
		overflow: hidden;
		cursor: pointer;
		text-align: left;
		transition: all 150ms;
	}

	.gallery-card:hover {
		border-color: var(--color-primary);
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1);
	}

	.card-cover {
		height: 120px;
		background: linear-gradient(
			135deg,
			color-mix(in srgb, var(--color-foreground) 5%, transparent),
			color-mix(in srgb, var(--color-foreground) 10%, transparent)
		);
		background-size: cover;
		background-position: center;
		display: flex;
		align-items: center;
		justify-content: center;
		position: relative;
	}

	.card-cover :global(.cover-placeholder-icon) {
		color: var(--color-foreground-subtle);
		opacity: 0.5;
	}

	.card-pinned {
		position: absolute;
		top: 0.5rem;
		right: 0.5rem;
		background: var(--color-primary);
		color: var(--color-background);
		padding: 0.25rem;
		border-radius: 0.25rem;
	}

	.card-content {
		padding: 0.75rem;
	}

	.card-title-row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-bottom: 0.5rem;
	}

	.card-content :global(.card-icon) {
		color: var(--color-foreground-subtle);
		flex-shrink: 0;
	}

	.card-title {
		font-weight: 500;
		color: var(--color-foreground);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.gallery-card:hover .card-title {
		color: var(--color-primary);
	}

	.card-tags {
		margin-bottom: 0.5rem;
	}

	.card-date {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}
</style>
