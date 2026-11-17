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
	import Icon from "$lib/components/Icon.svelte";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	type Grouping = "thread" | "date";
	type Group = { key: string; name: string; threadId: string | null; pages: PageSummary[] };

	const GROUPING_KEY = "virtues.pages.grouping";

	let creating = $state(false);
	let query = $state("");
	let grouping = $state<Grouping>(readGrouping());

	const loading = $derived(pagesStore.pagesLoading);
	const error = $derived(pagesStore.pagesError);

	onMount(async () => {
		await pagesStore.loadPages();
	});

	function readGrouping(): Grouping {
		try {
			return localStorage.getItem(GROUPING_KEY) === "date" ? "date" : "thread";
		} catch {
			return "thread";
		}
	}

	function setGrouping(next: Grouping) {
		grouping = next;
		try {
			localStorage.setItem(GROUPING_KEY, next);
		} catch {
			// The choice just isn't remembered in this browser.
		}
	}

	function isBlank(page: PageSummary): boolean {
		const title = page.title.trim();
		return !page.excerpt && (title === "" || title === "Untitled");
	}

	/** A blank page shows its first line as its name instead of "Untitled". */
	function displayTitle(page: PageSummary): string {
		const title = page.title.trim();
		if (title && title !== "Untitled") return title;
		return page.excerpt || "Untitled";
	}

	function displayExcerpt(page: PageSummary): string {
		const title = page.title.trim();
		return title && title !== "Untitled" ? (page.excerpt ?? "") : "";
	}

	// Blank drafts stay out of the list. They are still in search and in
	// the tab they were opened in.
	const visible = $derived.by(() => {
		const q = query.trim().toLowerCase();
		return pagesStore.pages.filter((p) => {
			if (isBlank(p)) return false;
			if (!q) return true;
			return p.title.toLowerCase().includes(q) || (p.excerpt ?? "").toLowerCase().includes(q);
		});
	});

	function startOfDay(d: Date): number {
		return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
	}

	function dateGroup(dateStr: string): { key: string; name: string } {
		const date = new Date(dateStr);
		const now = new Date();
		const days = Math.round((startOfDay(now) - startOfDay(date)) / 86_400_000);
		if (days <= 0) return { key: "today", name: "Today" };
		if (days === 1) return { key: "yesterday", name: "Yesterday" };
		if (days < 7) return { key: "week", name: "Earlier this week" };
		const sameYear = date.getFullYear() === now.getFullYear();
		const name = date.toLocaleDateString("en-US", { month: "long", year: sameYear ? undefined : "numeric" });
		return { key: `${date.getFullYear()}-${date.getMonth()}`, name };
	}

	// Pages arrive newest first, so each group's first page is its latest and
	// groups come out in the order of their latest page.
	const groups = $derived.by((): Group[] => {
		const out = new Map<string, Group>();
		const add = (key: string, name: string, threadId: string | null, page: PageSummary) => {
			let g = out.get(key);
			if (!g) {
				g = { key, name, threadId, pages: [] };
				out.set(key, g);
			}
			g.pages.push(page);
		};
		if (grouping === "date") {
			for (const p of visible) {
				const { key, name } = dateGroup(p.updated_at);
				add(key, name, null, p);
			}
			return [...out.values()];
		}
		const loose: PageSummary[] = [];
		for (const p of visible) {
			// A server older than this app sends no `threads`; its pages are
			// listed as not in a thread instead of the list failing.
			const threads = p.threads ?? [];
			if (threads.length === 0) loose.push(p);
			for (const t of threads) add(t.id, t.name, t.id, p);
		}
		const list = [...out.values()];
		if (loose.length) list.push({ key: "none", name: "Not in a thread", threadId: null, pages: loose });
		return list;
	});

	function formatDate(dateStr: string): string {
		const date = new Date(dateStr);
		const now = new Date();
		const days = Math.round((startOfDay(now) - startOfDay(date)) / 86_400_000);
		if (days <= 0) return date.toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" });
		if (days === 1) return "Yesterday";
		if (days < 7) return date.toLocaleDateString("en-US", { weekday: "long" });
		return date.toLocaleDateString("en-US", {
			month: "short",
			day: "numeric",
			year: now.getFullYear() !== date.getFullYear() ? "numeric" : undefined,
		});
	}

	function groupNote(g: Group): string {
		if (!g.threadId || g.pages.length === 0) return "";
		const when = formatDate(g.pages[0].updated_at);
		return /\d:\d/.test(when) ? "Last written today" : `Last written ${when}`;
	}

	function openPage(page: PageSummary, e?: MouseEvent) {
		pagesStore.markAsRecent(page.id);
		windowShellStore.openTabFromRoute(`/page/${page.id}`, {
			forceNew: !!(e && (e.metaKey || e.ctrlKey)),
			label: page.title,
			preferEmptyPane: true,
		});
	}

	function openThread(g: Group, e?: MouseEvent) {
		if (!g.threadId) return;
		windowShellStore.openTabFromRoute(`/thread/${g.threadId}`, {
			forceNew: !!(e && (e.metaKey || e.ctrlKey)),
			label: g.name,
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

<Page title="Pages" maxWidth="prose">
	{#snippet actions()}
		<Button
			variant="secondary"
			size="sm"
			icon="mynaui:plus"
			loading={creating}
			onclick={createNewPage}>New page</Button
		>
	{/snippet}

	<div class="controls">
		<label class="search">
			<Icon icon="mynaui:search" width="15" />
			<input type="search" placeholder="Search pages" aria-label="Search pages" bind:value={query} />
		</label>
		<div class="switch" role="group" aria-label="Group pages by">
			<button type="button" aria-pressed={grouping === "thread"} onclick={() => setGrouping("thread")}>By thread</button>
			<button type="button" aria-pressed={grouping === "date"} onclick={() => setGrouping("date")}>By date</button>
		</div>
	</div>

	{#if error && pagesStore.pages.length === 0}
		<div class="state">
			<p>Your pages didn't load. {error}</p>
			<Button variant="secondary" size="sm" onclick={() => pagesStore.loadPages()}>Try again</Button>
		</div>
	{:else if loading && pagesStore.pages.length === 0}
		<p class="state">Loading pages…</p>
	{:else if groups.length === 0}
		<p class="state">
			{query.trim() ? "No pages match that search." : "Pages you write show up here. Start one with New page."}
		</p>
	{:else}
		{#each groups as g (g.key)}
			<section class="group">
				<header class="group-head">
					{#if g.threadId}
						<button type="button" class="group-name link" onclick={(e) => openThread(g, e)}>{g.name}</button>
						<span class="group-note">{groupNote(g)}</span>
					{:else}
						<h2 class="group-name">{g.name}</h2>
					{/if}
				</header>
				<ul class="rows">
					{#each g.pages as page (page.id)}
						<li>
							<button
								type="button"
								class="row"
								onclick={(e) => openPage(page, e)}
								oncontextmenu={(e) => handleContextMenu(page, e)}
							>
								<span class="row-icon"><Icon icon={page.icon || "mynaui:file-text"} width="16" /></span>
								<span class="row-text">
									<span class="row-title">{displayTitle(page)}</span>
									{#if displayExcerpt(page)}
										<span class="row-excerpt">{displayExcerpt(page)}</span>
									{/if}
								</span>
								<span class="row-date">{formatDate(page.updated_at)}</span>
							</button>
						</li>
					{/each}
				</ul>
			</section>
		{/each}
	{/if}
</Page>

<style>
	.controls {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.75rem;
		margin-bottom: 2.5rem;
	}
	.search {
		flex: 1 1 16rem;
		display: flex;
		align-items: center;
		gap: 0.5rem;
		height: 2.25rem;
		padding: 0 0.75rem;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		color: var(--color-foreground-muted);
	}
	.search input {
		flex: 1;
		min-width: 0;
		border: 0;
		outline: 0;
		background: transparent;
		font: inherit;
		font-size: 0.875rem;
		color: var(--color-foreground);
	}
	.switch {
		display: flex;
		padding: 4px;
		border-radius: var(--radius-md);
		background: color-mix(in srgb, var(--wash-ink) 5%, transparent);
	}
	.switch button {
		height: 1.875rem;
		padding: 0 0.75rem;
		border: 1px solid transparent;
		border-radius: var(--radius-sm);
		background: transparent;
		font: inherit;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}
	.switch button[aria-pressed="true"] {
		background: var(--color-background);
		color: var(--color-foreground);
		border-color: var(--color-border);
	}

	.state {
		color: var(--color-foreground-muted);
		font-size: 0.9375rem;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 0.75rem;
	}

	.group {
		margin-bottom: 2.5rem;
	}
	.group-head {
		display: flex;
		align-items: baseline;
		gap: 0.75rem;
		padding-bottom: 0.5rem;
		border-bottom: 1px solid var(--color-border);
	}
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
		cursor: pointer;
	}
	.group-name.link:hover {
		color: var(--color-foreground-muted);
	}
	.group-note {
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}

	.rows {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.row {
		display: flex;
		align-items: flex-start;
		gap: 0.875rem;
		width: 100%;
		padding: 0.875rem 0.25rem;
		border: 0;
		border-bottom: 1px solid color-mix(in srgb, var(--color-border) 55%, transparent);
		background: none;
		font: inherit;
		text-align: left;
		color: inherit;
		cursor: pointer;
	}
	.row:hover .row-title {
		color: var(--color-foreground-muted);
	}
	.row-icon {
		flex: 0 0 16px;
		margin-top: 0.25rem;
		color: var(--color-foreground-muted);
	}
	.row-text {
		flex: 1 1 auto;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}
	.row-title {
		font-family: var(--font-serif);
		font-size: 1.125rem;
		line-height: 1.3;
		color: var(--color-foreground);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.row-excerpt {
		font-family: var(--font-serif);
		font-size: 1rem;
		line-height: 1.45;
		color: var(--color-foreground-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.row-date {
		flex: 0 0 auto;
		margin-top: 0.25rem;
		font-size: 0.8125rem;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-muted);
	}
</style>
