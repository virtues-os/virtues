<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { workspaceStore } from '$lib/stores/workspace.svelte';
	import { pagesStore } from '$lib/stores/pages.svelte';
	import { Page } from '$lib';
	import { onMount } from 'svelte';
	import "iconify-icon";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	interface PageSummary {
		id: string;
		title: string;
		updated_at: string;
	}

	let pages = $state<PageSummary[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let creating = $state(false);

	onMount(async () => {
		await loadPages();
	});

	async function loadPages() {
		loading = true;
		error = null;
		try {
			const response = await fetch('/api/pages');
			if (!response.ok) throw new Error('Failed to load pages');
			const data = await response.json();
			pages = data.pages || [];
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load pages';
		} finally {
			loading = false;
		}
	}

	function formatDate(dateStr: string): string {
		const date = new Date(dateStr);
		const now = new Date();
		const diffMs = now.getTime() - date.getTime();
		const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

		if (diffDays === 0) {
			return date.toLocaleTimeString('en-US', {
				hour: 'numeric',
				minute: '2-digit'
			});
		} else if (diffDays === 1) {
			return 'Yesterday';
		} else if (diffDays < 7) {
			return date.toLocaleDateString('en-US', { weekday: 'long' });
		} else {
			return date.toLocaleDateString('en-US', {
				month: 'short',
				day: 'numeric',
				year: now.getFullYear() !== date.getFullYear() ? 'numeric' : undefined
			});
		}
	}

	function handlePageClick(pageId: string, title: string) {
		workspaceStore.openTabFromRoute(`/pages/${pageId}`, {
			label: title,
			preferEmptyPane: true
		});
	}

	async function createNewPage() {
		if (creating) return;
		creating = true;

		try {
			const response = await fetch("/api/pages", {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({ title: "Untitled", content: "" }),
			});

			if (!response.ok) {
				throw new Error("Failed to create page");
			}

			const page = await response.json();
			
			// Sync with sidebar pages store
			pagesStore.addPage(page);
			
			workspaceStore.openTabFromRoute(`/pages/${page.id}`, {
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
	<div class="max-w-2xl">
		<div class="mb-8 flex items-start justify-between">
			<div>
				<h1 class="text-3xl font-serif font-medium text-foreground mb-2">Pages</h1>
				<p class="text-foreground-muted">
					{pages.length} page{pages.length !== 1 ? 's' : ''}
				</p>
			</div>
			<button
				onclick={createNewPage}
				disabled={creating}
				class="flex items-center gap-2 px-4 py-2 bg-foreground text-background rounded-lg hover:bg-foreground/90 transition-colors disabled:opacity-50"
			>
				<iconify-icon icon="ri:add-line" width="18"></iconify-icon>
				New Page
			</button>
		</div>

		{#if loading}
			<div class="text-center py-12 text-foreground-muted">Loading...</div>
		{:else if error}
			<div class="p-4 bg-error-subtle border border-error rounded-lg text-error">
				{error}
			</div>
		{:else if pages.length === 0}
			<div class="text-center py-12 text-foreground-muted">
				<iconify-icon icon="ri:file-text-line" class="text-6xl mb-4 text-foreground-subtle"></iconify-icon>
				<p class="mb-4">No pages yet</p>
				<button
					onclick={createNewPage}
					disabled={creating}
					class="text-primary hover:underline"
				>
					Create your first page
				</button>
			</div>
		{:else}
			<ul class="space-y-1">
				{#each pages as page}
					<li>
						<button
							onclick={() => handlePageClick(page.id, page.title)}
							class="w-full text-left block py-3 px-3 -mx-3 rounded-md hover:bg-surface-elevated transition-colors group"
						>
							<div class="flex items-center gap-3">
								<iconify-icon 
									icon="ri:file-text-line" 
									class="text-foreground-subtle group-hover:text-foreground transition-colors"
									width="18"
								></iconify-icon>
								<span class="text-foreground group-hover:text-primary transition-colors flex-1">
									{page.title}
								</span>
								<span class="text-foreground-subtle text-sm">
									{formatDate(page.updated_at)}
								</span>
							</div>
						</button>
					</li>
				{/each}
			</ul>
		{/if}
	</div>
</Page>
