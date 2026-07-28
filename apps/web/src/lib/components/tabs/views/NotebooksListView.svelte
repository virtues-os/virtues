<script lang="ts">
	import { onMount, tick } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { notebookStore } from '$lib/stores/notebook.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { Page } from '$lib';
	import UniversalDataGrid, { type Column } from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import type { NotebookSummary } from '$lib/api/client';

	let { active: _active }: { tab?: unknown; active?: boolean } = $props();

	let creating = $state(false);
	// Inline draft — `prompt()` is a no-op in the Tauri/WKWebView shell.
	let drafting = $state(false);
	let draftName = $state('');
	let inputEl = $state<HTMLInputElement | null>(null);

	onMount(() => {
		notebookStore.load();
	});

	const notebooks = $derived(notebookStore.notebooks);

	const columns: Column<NotebookSummary>[] = [
		{
			key: 'name',
			label: 'Name',
			icon: 'ri:layout-masonry-line',
			width: '35%',
			minWidth: '180px'
		},
		{
			key: 'current_status',
			label: 'Memo',
			icon: 'ri:sticky-note-line',
			width: '35%',
			minWidth: '160px'
		},
		{
			key: 'chat_count',
			label: 'Chats',
			icon: 'ri:chat-3-line',
			width: '10%',
			minWidth: '70px',
			format: 'number',
			hideOnMobile: true
		},
		{
			key: 'updated_at',
			label: 'Updated',
			icon: 'ri:time-line',
			width: '20%',
			minWidth: '120px',
			hideOnMobile: true,
			getValue: (item) => formatRelativeDate(item.updated_at)
		}
	];

	function formatRelativeDate(dateStr?: string | null): string | null {
		if (!dateStr) return null;
		const date = new Date(dateStr);
		if (Number.isNaN(date.getTime())) return null;
		const diffMs = Date.now() - date.getTime();
		if (diffMs < 0) return 'Upcoming';
		const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

		if (diffDays === 0) return 'Today';
		if (diffDays === 1) return 'Yesterday';
		if (diffDays < 7) return `${diffDays} days ago`;
		if (diffDays < 30) return `${Math.floor(diffDays / 7)} weeks ago`;
		if (diffDays < 365) return `${Math.floor(diffDays / 30)} months ago`;
		return `${Math.floor(diffDays / 365)} years ago`;
	}

	function open(id: string) {
		windowShellStore.openTabFromRoute(`/notebook/${id}`);
	}

	async function startDraft() {
		drafting = true;
		draftName = '';
		await tick();
		inputEl?.focus();
	}
	function cancelDraft() {
		drafting = false;
		draftName = '';
	}
	async function commitDraft() {
		const name = draftName.trim();
		if (!name || creating) {
			if (!name) cancelDraft();
			return;
		}
		creating = true;
		try {
			const notebook = await notebookStore.create(name);
			cancelDraft();
			if (notebook) open(notebook.id);
		} finally {
			creating = false;
		}
	}
	function onDraftKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter') {
			e.preventDefault();
			commitDraft();
		} else if (e.key === 'Escape') {
			e.preventDefault();
			cancelDraft();
		}
	}
</script>

<Page
	title="Notebooks"
	description="Notebooks you return to — a project, pet, hobby, or goal. Each chat lives in one."
	maxWidth="wide"
>
	{#snippet actions()}
		{#if drafting}
			<input
				bind:this={inputEl}
				bind:value={draftName}
				class="name-input"
				placeholder="Name your Notebook"
				disabled={creating}
				onkeydown={onDraftKeydown}
				onblur={commitDraft}
			/>
		{:else}
			<button class="new-btn" onclick={startDraft} disabled={creating}>
				<Icon icon="ri:add-line" width="16" /> New Notebook
			</button>
		{/if}
	{/snippet}

	{#if notebooks.length === 0 && !notebookStore.loading && !notebookStore.error}
		<div class="empty">
			<Icon icon="ri:layout-masonry-line" width="28" />
			<p>No Notebooks yet.</p>
			{#if !drafting}
				<button class="new-btn ghost" onclick={startDraft}>Create your first Notebook</button>
			{/if}
		</div>
	{:else}
		<UniversalDataGrid
			items={notebooks}
			{columns}
			entityType="notebook"
			loading={notebookStore.loading}
			error={notebookStore.error}
			emptyIcon="ri:layout-masonry-line"
			emptyMessage="No Notebooks yet"
			loadingMessage="Loading Notebooks..."
			searchPlaceholder="Search Notebooks..."
			defaultViewMode="grid"
			gridMinWidth="200px"
			onItemClick={(nb) => open(nb.id)}
			onRetry={() => notebookStore.load()}
		>
			{#snippet tableRow(nb: NotebookSummary)}
				<td class="col-name">
					<div class="name-cell">
						<span
							class="row-icon"
							class:tinted={!!nb.accent_color}
							style={nb.accent_color ? `--room-accent: ${nb.accent_color}` : ''}
						>
							<Icon icon={nb.icon || 'ri:layout-masonry-line'} width="15" />
						</span>
						<span class="name-text">{nb.name}</span>
					</div>
				</td>
				<td class="col-memo">
					{#if nb.current_status}
						<span class="memo-text">{nb.current_status}</span>
					{:else}
						<span class="empty-cell">—</span>
					{/if}
				</td>
				<td class="col-chats hide-mobile">
					<span class="count-text">{nb.chat_count}</span>
				</td>
				<td class="col-updated hide-mobile">
					{#if formatRelativeDate(nb.updated_at)}
						<span class="date-text">{formatRelativeDate(nb.updated_at)}</span>
					{:else}
						<span class="empty-cell">—</span>
					{/if}
				</td>
			{/snippet}

			{#snippet card(nb: NotebookSummary)}
				<div
					class="nb-card"
					class:tinted={!!nb.accent_color}
					style={nb.accent_color ? `--room-accent: ${nb.accent_color}` : ''}
				>
					<div class="nb-card-icon"><Icon icon={nb.icon || 'ri:layout-masonry-line'} width="20" /></div>
					<div class="nb-card-name">{nb.name}</div>
					{#if nb.current_status}
						<div class="nb-card-memo">{nb.current_status}</div>
					{/if}
					<div class="nb-card-meta">
						<span>{nb.chat_count} {nb.chat_count === 1 ? 'chat' : 'chats'}</span>
						<span class="dot-sep">·</span>
						<span>{nb.item_count} pinned</span>
					</div>
				</div>
			{/snippet}
		</UniversalDataGrid>
	{/if}
</Page>

<style>
	.new-btn {
		display: inline-flex; align-items: center; gap: 5px;
		padding: 7px 12px; border: 1px solid var(--color-border); border-radius: 8px;
		background: var(--color-surface-elevated); color: var(--color-foreground);
		font-size: 13px; font-weight: 500; cursor: pointer; white-space: nowrap;
	}
	.new-btn:hover { background: var(--color-surface); }
	.new-btn.ghost { background: transparent; margin-top: 10px; }
	.name-input {
		padding: 7px 12px; border: 1px solid var(--color-border); border-radius: 8px;
		background: var(--color-surface-elevated); color: var(--color-foreground);
		font-size: 13px; outline: none; min-width: 200px;
	}
	.name-input:focus { border-color: var(--color-primary, var(--color-foreground-muted)); }

	/* Card (grid view) — fills the grid's unstyled card button */
	.nb-card {
		display: flex; flex-direction: column; gap: 6px;
		padding: 14px; border: 1px solid var(--color-border); border-radius: 12px;
		background: var(--color-surface);
		width: 100%; height: 100%;
		transition: border-color 0.12s ease, background 0.12s ease;
	}
	.nb-card:hover { background: var(--hover-bg); }
	.nb-card.tinted { box-shadow: inset 3px 0 0 var(--room-accent); border-color: color-mix(in srgb, var(--room-accent) 30%, var(--color-border)); }
	.nb-card-icon {
		display: grid; place-items: center; width: 36px; height: 36px;
		border-radius: 9px; background: var(--color-surface-elevated); color: var(--color-foreground); margin-bottom: 2px;
	}
	.nb-card.tinted .nb-card-icon {
		background: color-mix(in srgb, var(--room-accent) 16%, transparent);
		color: color-mix(in srgb, var(--room-accent) 78%, var(--color-foreground));
	}
	.nb-card-name { font-size: 14.5px; font-weight: 600; color: var(--color-foreground); }
	.nb-card-memo {
		font-size: 12.5px; color: var(--color-foreground-muted); line-height: 1.4;
		display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;
	}
	.nb-card-meta { display: flex; align-items: center; gap: 6px; font-size: 11.5px; color: var(--color-foreground-subtle, #9ca3af); margin-top: 2px; }
	.dot-sep { opacity: 0.5; }

	/* Table row styles */
	.name-cell { display: flex; align-items: center; gap: 0.5rem; }
	.row-icon {
		display: grid; place-items: center; width: 24px; height: 24px;
		border-radius: 6px; background: var(--color-surface-elevated);
		color: var(--color-foreground); flex-shrink: 0;
	}
	.row-icon.tinted {
		background: color-mix(in srgb, var(--room-accent) 16%, transparent);
		color: color-mix(in srgb, var(--room-accent) 78%, var(--color-foreground));
	}
	.name-text { font-weight: 500; color: var(--color-foreground); }
	.memo-text {
		color: var(--color-foreground-muted); font-size: 0.8125rem;
		display: -webkit-box; -webkit-line-clamp: 1; line-clamp: 1; -webkit-box-orient: vertical; overflow: hidden;
	}
	.count-text { color: var(--color-foreground-muted); font-size: 0.8125rem; font-variant-numeric: tabular-nums; }
	.date-text { color: var(--color-foreground-muted); font-size: 0.8125rem; }
	.empty-cell { color: var(--color-foreground-subtle); }

	.col-name { width: 35%; min-width: 180px; padding: 0.625rem 0.75rem; padding-left: 0; }
	.col-memo { width: 35%; min-width: 160px; padding: 0.625rem 0.75rem; }
	.col-chats { width: 10%; min-width: 70px; padding: 0.625rem 0.75rem; }
	.col-updated { width: 20%; min-width: 120px; padding: 0.625rem 0.75rem; padding-right: 0; }

	@media (max-width: 768px) {
		.hide-mobile { display: none; }
		.col-name { width: 55%; }
		.col-memo { width: 45%; }
	}

	.empty { display: flex; flex-direction: column; align-items: center; gap: 6px; padding: 64px 0; color: var(--color-foreground-muted); }
	.empty p { margin: 0; font-size: 14px; }
</style>
