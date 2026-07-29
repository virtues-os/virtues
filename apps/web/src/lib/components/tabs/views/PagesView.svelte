<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import type { PageSummary } from "$lib/api/client";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { pagesStore } from "$lib/stores/pages.svelte";
	import { contextMenu } from "$lib/stores/contextMenu.svelte";
	import type { ContextMenuItem } from "$lib/stores/contextMenu.svelte";
	import { getKeepMenuItems } from "$lib/utils/contextMenuItems";
	import { confirmAction } from "$lib/stores/dialog.svelte";
	import { Page, Button } from "$lib";
	import { onMount } from "svelte";
	import { paneActions } from "$lib/stores/paneActions.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import UniversalDataGrid, { type Column } from "$lib/components/datagrid/UniversalDataGrid.svelte";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// Published to the pane toolbar rather than rendered beside the title, so
	// every view's actions sit in the same place. An $effect rather than
	// onMount: `creating` changes while the tab is open, and the toolbar has to
	// see it — a one-shot registration would freeze the disabled state.
	$effect(() =>
		paneActions.set(tab.id, [
			{
				id: "page.new",
				label: "New page",
				icon: "ri:add-line",
				primary: true,
				disabled: creating,
				run: createNewPage,
			},
		]),
	);

	let creating = $state(false);

	const pages = $derived(pagesStore.pages);
	const loading = $derived(pagesStore.pagesLoading);
	const error = $derived(pagesStore.pagesError);

	onMount(async () => {
		await pagesStore.loadPages();
	});

	function formatDate(dateStr: string): string {
		const date = new Date(dateStr);
		const now = new Date();
		const diffDays = Math.floor((now.getTime() - date.getTime()) / 86_400_000);
		if (diffDays === 0) return date.toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" });
		if (diffDays === 1) return "Yesterday";
		if (diffDays < 7) return date.toLocaleDateString("en-US", { weekday: "long" });
		return date.toLocaleDateString("en-US", {
			month: "short",
			day: "numeric",
			year: now.getFullYear() !== date.getFullYear() ? "numeric" : undefined,
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

	const columns: Column<PageSummary>[] = [
		{ key: "title", label: "Title", icon: "ri:file-text-line", width: "45%", minWidth: "200px" },
		{ key: "tags", label: "Tags", icon: "ri:price-tag-3-line", width: "20%", minWidth: "120px", hideOnMobile: true },
		{ key: "updated_at", label: "Updated", icon: "ri:time-line", width: "17%", minWidth: "100px", getValue: (p) => formatDate(p.updated_at) },
		{ key: "created_at", label: "Created", icon: "ri:calendar-line", width: "18%", minWidth: "100px", hideOnMobile: true, getValue: (p) => formatDate(p.created_at) },
	];

	function handlePageClick(page: PageSummary, e?: MouseEvent) {
		pagesStore.markAsRecent(page.id);
		const forceNew = !!(e && (e.metaKey || e.ctrlKey));
		windowShellStore.openTabFromRoute(`/page/${page.id}`, {
			forceNew,
			label: page.title,
			preferEmptyPane: true,
		});
	}

	function handleContextMenu(page: PageSummary, e: MouseEvent) {
		e.preventDefault();
		const items: ContextMenuItem[] = [
			{
				id: "open-new-tab",
				label: "Open in New Tab",
				icon: "ri:external-link-line",
				action: () => {
					windowShellStore.openTabFromRoute(`/page/${page.id}`, {
						forceNew: true,
						label: page.title,
						preferEmptyPane: true,
					});
				},
			},
			...getKeepMenuItems({
				url: `/page/${page.id}`,
				label: page.title,
				icon: page.icon,
			}),
			{
				id: "delete",
				label: "Delete",
				icon: "ri:delete-bin-line",
				variant: "destructive",
				dividerBefore: true,
				action: async () => {
					const ok = await confirmAction({
						title: "Delete page?",
						body: `"${page.title}" will be deleted. Notebooks that reference it will drop the link.`,
						confirmLabel: "Delete",
						danger: true,
					});
					if (ok) await pagesStore.removePage(page.id);
				},
			},
		];
		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}

	async function createNewPage() {
		if (creating) return;
		creating = true;
		try {
			const page = await pagesStore.createNewPage("Untitled");
			pagesStore.addPage(page);
			pagesStore.markAsRecent(page.id);
			windowShellStore.openTabFromRoute(`/page/${page.id}`, {
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

<Page
	title="Pages"
	description={`${pages.length} page${pages.length !== 1 ? "s" : ""}`}
	maxWidth="wide"
>
	<UniversalDataGrid
		items={pages}
		{columns}
		entityType="page"
		{loading}
		{error}
		emptyIcon="ri:file-text-line"
		emptyMessage="No pages yet"
		loadingMessage="Loading pages…"
		searchPlaceholder="Search pages…"
		onItemClick={handlePageClick}
		onItemContextMenu={handleContextMenu}
		onRetry={() => pagesStore.loadPages()}
	>
		{#snippet tableRow(page: PageSummary)}
			{@const tags = parseTags(page.tags)}
			<td class="col-title">
				<span class="title-cell">
					<Icon icon={getPageIcon(page)} width="16" />
					<span class="title-text">{page.title}</span>
				</span>
			</td>
			<td class="col-tags hide-mobile">
				{#if tags.length > 0}
					<div class="tags-row">
						{#each tags.slice(0, 2) as tag}
							<span class="tag">{tag}</span>
						{/each}
						{#if tags.length > 2}
							<span class="tag-more">+{tags.length - 2}</span>
						{/if}
					</div>
				{:else}
					<span class="empty-cell">—</span>
				{/if}
			</td>
			<td class="col-date">{formatDate(page.updated_at)}</td>
			<td class="col-date hide-mobile">{formatDate(page.created_at)}</td>
		{/snippet}

		{#snippet card(page: PageSummary)}
			{@const tags = parseTags(page.tags)}
			<div class="card-content">
				<div
					class="card-cover"
					style={page.cover_url ? `background-image: url(${page.cover_url})` : ""}
				>
					{#if !page.cover_url}
						<Icon icon={getPageIcon(page)} width="32" />
					{/if}
				</div>
				<div class="card-body">
					<div class="card-title-row">
						<Icon icon={getPageIcon(page)} width="16" />
						<span class="card-title">{page.title}</span>
					</div>
					{#if tags.length > 0}
						<div class="tags-row">
							{#each tags.slice(0, 3) as tag}
								<span class="tag">{tag}</span>
							{/each}
						</div>
					{/if}
					<div class="card-date">{formatDate(page.updated_at)}</div>
				</div>
			</div>
		{/snippet}
	</UniversalDataGrid>
</Page>

<style>
	.col-title {
		font-weight: 500;
		color: var(--color-foreground);
		padding: 0.625rem 0.75rem;
	}
	.title-cell {
		display: inline-flex;
		align-items: center;
		gap: 0.5rem;
		min-width: 0;
	}
	.title-cell :global(svg) {
		flex-shrink: 0;
		color: var(--color-foreground-muted);
	}
	.title-text {
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.col-tags {
		padding: 0.625rem 0.75rem;
	}
	.col-date {
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
		font-size: 0.8125rem;
		padding: 0.625rem 0.75rem;
	}
	.empty-cell {
		color: var(--color-foreground-subtle);
	}
	@media (max-width: 768px) {
		.hide-mobile {
			display: none;
		}
	}

	.tags-row {
		display: flex;
		gap: 0.25rem;
		flex-wrap: wrap;
	}
	.tag {
		display: inline-flex;
		align-items: center;
		padding: 0.125rem 0.5rem;
		font-size: 0.6875rem;
		font-weight: 500;
		border-radius: var(--radius-full);
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
		color: var(--color-foreground-muted);
	}
	.tag-more {
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
	}

	.card-content {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		text-align: left;
		width: 100%;
	}
	.card-cover {
		aspect-ratio: 16 / 9;
		background-size: cover;
		background-position: center;
		background-color: var(--color-surface-elevated);
		border-radius: 6px;
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--color-foreground-subtle);
	}
	.card-body {
		display: flex;
		flex-direction: column;
		gap: 0.375rem;
	}
	.card-title-row {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		color: var(--color-foreground);
	}
	.card-title {
		font-weight: 600;
		font-size: 0.9375rem;
		line-height: 1.3;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.card-date {
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
	}
</style>
