<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import type { PageSummary } from "$lib/api/client";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { pagesStore } from "$lib/stores/pages.svelte";
	import { contextMenu } from "$lib/stores/contextMenu.svelte";
	import type { ContextMenuItem } from "$lib/stores/contextMenu.svelte";
	import { getKeepMenuItems } from "$lib/utils/contextMenuItems";
	import { notifyTrashed, routeIfOpen } from "$lib/utils/toasts";
	import { Button, Page } from "$lib";
	import { onMount } from "svelte";
	import ListRow from "$lib/components/datagrid/ListRow.svelte";
	import { formatRelativeTimestamp } from "$lib/utils/dateUtils";
	import UniversalDataGrid, { type Column, type GroupInfo } from "$lib/components/datagrid/UniversalDataGrid.svelte";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	const NO_THREAD = "Not in a thread";

	let creating = $state(false);

	const loading = $derived(pagesStore.pagesLoading);
	const error = $derived(pagesStore.pagesError);

	onMount(async () => {
		await pagesStore.loadPages();
	});

	function isBlank(page: PageSummary): boolean {
		const title = page.title.trim();
		return !page.excerpt && (title === "" || title === "Untitled");
	}

	// Blank drafts stay out of the list. They are still in search and in
	// the tab they were opened in.
	const pages = $derived(pagesStore.pages.filter((p) => !isBlank(p)));

	/** A thread's id by its name, for the group header's link. */
	const threadIds = $derived.by(() => {
		const ids = new Map<string, string>();
		// A server older than this app sends no `threads`.
		for (const p of pages) for (const t of p.threads ?? []) ids.set(t.name, t.id);
		return ids;
	});

	/** An untitled page shows its first line as its name. */
	function displayTitle(page: PageSummary): string {
		const title = page.title.trim();
		if (title && title !== "Untitled") return title;
		return page.excerpt || "Untitled";
	}

	function displayExcerpt(page: PageSummary): string {
		const title = page.title.trim();
		return title && title !== "Untitled" ? (page.excerpt ?? "") : "";
	}

	function startOfDay(d: Date): number {
		return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
	}

	function daysAgo(dateStr: string): number {
		return Math.round((startOfDay(new Date()) - startOfDay(new Date(dateStr))) / 86_400_000);
	}

	function dateGroup(dateStr: string): string {
		const date = new Date(dateStr);
		const days = daysAgo(dateStr);
		if (days <= 0) return "Today";
		if (days === 1) return "Yesterday";
		if (days < 7) return "Earlier this week";
		const sameYear = date.getFullYear() === new Date().getFullYear();
		return date.toLocaleDateString("en-US", { month: "long", year: sameYear ? undefined : "numeric" });
	}

	// The list sorts newest first and groups in order of appearance, so the
	// thread or month with the latest page comes first either way.
	const columns: Column<PageSummary>[] = [
		{ key: "title", label: "Title", getValue: displayTitle },
		{ key: "excerpt", label: "Text", hidden: true },
		{
			key: "threads",
			label: "Thread",
			hidden: true,
			groupable: true,
			groupSort: "appearance",
			groupFallback: NO_THREAD,
			getValue: (p) => (p.threads ?? []).map((t) => t.name).join(", "),
			groupValues: (p) => (p.threads ?? []).map((t) => t.name),
		},
		{
			key: "updated_at",
			label: "Date",
			hidden: true,
			groupable: true,
			groupSort: "appearance",
			getValue: (p) => dateGroup(p.updated_at),
		},
	];

	function lastWritten(group: GroupInfo<PageSummary>): string {
		const latest = group.items[0];
		if (!latest) return "";
		return daysAgo(latest.updated_at) <= 0 ? "Last written today" : `Last written ${formatRelativeTimestamp(latest.updated_at)}`;
	}

	function openPage(page: PageSummary) {
		pagesStore.markAsRecent(page.id);
		windowShellStore.openTabFromRoute(`/page/${page.id}`, {
			label: page.title,
			preferEmptyPane: true,
		});
	}

	function openThread(name: string, e: MouseEvent) {
		const id = threadIds.get(name);
		if (!id) return;
		windowShellStore.openTabFromRoute(`/thread/${id}`, {
			forceNew: e.metaKey || e.ctrlKey,
			label: name,
			preferEmptyPane: true,
		});
	}

	function handleContextMenu(page: PageSummary, e: MouseEvent) {
		e.preventDefault();
		const items: ContextMenuItem[] = [
			{
				id: "open-new-tab",
				label: "Open in New Tab",
				icon: "mynaui:external-link",
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
				icon: "mynaui:trash",
				variant: "destructive",
				dividerBefore: true,
				// No confirm: the page goes to Recently deleted and the toast
				// carries the Undo.
				action: async () => {
					const reopen = routeIfOpen(`/page/${page.id}`);
					await pagesStore.removePage(page.id);
					notifyTrashed({ kind: "page", id: page.id, name: page.title, reopen });
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
			pagesStore.addPage({ ...page, excerpt: "", threads: [] });
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

<Page title="Pages" maxWidth="reading">
	{#snippet actions()}
		<Button
			variant="secondary"
			size="sm"
			icon="mynaui:plus"
			loading={creating}
			onclick={createNewPage}>New page</Button
		>
	{/snippet}

	<UniversalDataGrid
		items={pages}
		{columns}
		entityType="pages-list"
		viewModes={["list"]}
		defaultViewMode="list"
		defaultGroupBy="threads"
		showCounts={false}
		pageSize={100000}
		{loading}
		{error}
		emptyIcon="mynaui:file-text"
		emptyMessage="Pages you write show up here. Start one with New page."
		loadingMessage="Loading pages…"
		searchPlaceholder="Search pages"
		onItemClick={openPage}
		onItemContextMenu={handleContextMenu}
		onRetry={() => pagesStore.loadPages()}
	>
		{#snippet groupHeader(group: GroupInfo<PageSummary>)}
			{#if threadIds.has(group.key)}
				<button type="button" class="group-name link" onclick={(e) => openThread(group.key, e)}>{group.key}</button>
				<span class="group-note">{lastWritten(group)}</span>
			{:else}
				<h2 class="group-name">{group.key}</h2>
			{/if}
		{/snippet}

		{#snippet listRow(page: PageSummary)}
			<ListRow
				icon={page.icon || "mynaui:file-text"}
				title={displayTitle(page)}
				line={displayExcerpt(page)}
				note={formatRelativeTimestamp(page.updated_at)}
			/>
		{/snippet}
	</UniversalDataGrid>
</Page>

<style>
	.group-name {
		margin: 0;
		padding: 0;
		border: 0;
		background: none;
		font-family: var(--font-serif);
		font-size: var(--md-h2-size);
		font-weight: 400;
		color: var(--color-foreground);
		text-align: left;
	}
	.group-name.link {
		/* Room for the focus tint, given back so the name stays aligned. */
		margin: 0 -0.375rem;
		padding: 0 0.375rem;
		border-radius: var(--radius-sm);
		cursor: pointer;
	}
	.group-name.link:hover {
		color: var(--color-foreground-muted);
	}
	/* No focus rings: keyboard focus takes the same tint as a focused row. */
	.group-name.link:focus-visible {
		background: color-mix(in srgb, var(--wash-ink) 4%, transparent);
	}
	.group-note {
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}

</style>
