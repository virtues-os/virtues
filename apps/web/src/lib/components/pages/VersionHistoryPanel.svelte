<script lang="ts">
	/**
	 * VersionHistoryPanel - Popover content for page version history
	 *
	 * Allows users to save snapshots and restore to previous versions.
	 * Use inside a Popover primitive for proper positioning and dismiss behavior.
	 *
	 * A block page's history is the server's: Save asks it to keep the page as
	 * it stands, Restore asks it to put a version back (it keeps the page as it
	 * was first, and records "Put back vN"), and a row opens a preview of the
	 * version drawn as the editor draws it. A markdown page's versions are cut
	 * and restored from the document this browser holds.
	 */
	import Icon from '$lib/components/Icon.svelte';
	import Button from '$lib/components/Button.svelte';
	import DocumentPreview from '$lib/components/pages/DocumentPreview.svelte';
	import { confirmAction } from '$lib/stores/dialog.svelte';
	import { formatTimeAgo } from '$lib/utils/dateUtils';
	import { getPageVersion, type DocumentNote, type PageFormat } from '$lib/api/client';
	import type { YjsDocument } from '$lib/yjs';
	import {
		cutServerVersion,
		listVersions,
		restorePageVersion,
		restoreVersion,
		saveVersion,
		type PageVersion,
	} from '$lib/yjs/versions';
	import { onMount } from 'svelte';

	interface Props {
		close: () => void;
		pageId: string;
		/** How the page holds its text; markdown when the server does not say. */
		format?: PageFormat;
		/** A markdown page's document; a block page needs none. */
		yjsDoc?: YjsDocument;
	}

	let { close, pageId, format = 'markdown', yjsDoc }: Props = $props();

	const tree = $derived(format === 'tree');

	let versions = $state<PageVersion[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let saving = $state(false);
	let restoringId = $state<string | null>(null);
	/** What converting the version changed, after a block page's restore. */
	let notes = $state<DocumentNote[]>([]);
	/** What a block page's restore did when it is worth saying, the panel staying open. */
	let status = $state<string | null>(null);
	/** The block version shown, as its HTML or, from before the page was blocks, its markdown. */
	let preview = $state<{ id: string; number: number; html: string | null; markdown: string } | null>(null);
	let previewLoading = $state<string | null>(null);

	onMount(() => {
		loadVersions();
	});

	async function loadVersions() {
		loading = true;
		error = null;
		try {
			versions = await listVersions(pageId);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load';
			versions = [];
		} finally {
			loading = false;
		}
	}

	async function handleSave() {
		if (!tree && !yjsDoc) return;
		saving = true;
		error = null;
		try {
			const result = tree ? await cutServerVersion(pageId) : await saveVersion(yjsDoc!.ydoc, pageId);
			if (result) {
				await loadVersions();
			} else {
				error = 'Failed to save';
			}
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to save';
		} finally {
			saving = false;
		}
	}

	async function handlePreview(version: PageVersion) {
		if (preview?.id === version.id) {
			preview = null;
			return;
		}
		previewLoading = version.id;
		error = null;
		try {
			const detail = await getPageVersion(version.id);
			preview = {
				id: version.id,
				number: version.version_number,
				html: detail.html ?? null,
				markdown: detail.markdown ?? detail.content_preview ?? '',
			};
		} catch (e) {
			error = e instanceof Error ? e.message : "Your server couldn't open that version. Try again.";
		} finally {
			previewLoading = null;
		}
	}

	async function confirmRestore(): Promise<boolean> {
		return confirmAction({
			title: 'Restore this version?',
			body: "This replaces what's on the page now. Your server saves a snapshot first, so you can undo it.",
			confirmLabel: 'Restore',
		});
	}

	async function handleRestore(version: PageVersion) {
		if (tree) return restoreTree(version);
		if (!yjsDoc) return;
		if (!(await confirmRestore())) return;

		restoringId = version.id;
		error = null;
		try {
			// A version holds the page as the edit it records left it. The one
			// before keeps any edits not yet saved, so the restore can be undone;
			// the one after is the restore itself, yours in History. The confirm
			// promised the one before, so without it nothing is put back.
			const kept = await saveVersion(yjsDoc.ydoc, pageId, 'Auto-saved before restore', 'auto');
			if (!kept) {
				error = "Your server couldn't save a copy of this page first, so it didn't put the version back. Try again.";
				return;
			}

			const success = await restoreVersion(yjsDoc, version.id);
			if (!success) {
				error = 'Failed to restore';
				return;
			}
			const recorded = await saveVersion(yjsDoc.ydoc, pageId, `Put back v${version.version_number}`, 'user');
			if (!recorded) {
				// The list gains the copy saved before; the panel stays open to say why.
				await loadVersions();
				error = "Your server put the version back but couldn't add it to History. Save a version to keep it there.";
				return;
			}
			close();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to restore';
		} finally {
			restoringId = null;
		}
	}

	/** One request: the server keeps the page as it is, writes only what differs, and records it. */
	async function restoreTree(version: PageVersion) {
		if (!(await confirmRestore())) return;
		restoringId = version.id;
		error = null;
		notes = [];
		status = null;
		try {
			const restored = await restorePageVersion(pageId, version.id);
			if (restored.changed === false) {
				status = `The page already reads as v${version.version_number}.`;
				return;
			}
			if (restored.version_number === null) {
				// The page is put back; the list gains the copy kept before.
				await loadVersions();
				error = "Your server put the version back but couldn't add it to History. Save a version to keep it there.";
				return;
			}
			if (restored.notes.length) {
				// The panel stays open to show what converting changed.
				notes = restored.notes;
				preview = null;
				await loadVersions();
				return;
			}
			close();
		} catch (e) {
			error = e instanceof Error ? e.message : "Your server couldn't put that version back. Try again.";
		} finally {
			restoringId = null;
		}
	}

	function formatDate(dateString: string): string {
		return formatTimeAgo(dateString);
	}
</script>

<div class="version-panel" class:with-preview={preview !== null}>
	<div class="panel-header">
		<span>Versions</span>
		<Button
			variant="secondary"
			size="sm"
			icon="ri:add-line"
			loading={saving}
			disabled={!tree && !yjsDoc}
			onclick={handleSave}
		>
			Save
		</Button>
	</div>

	{#if error}
		<div class="error">{error}</div>
	{/if}

	{#if status}
		<div class="notes" role="status">{status}</div>
	{/if}

	{#if notes.length}
		<div class="notes" role="status">
			<p class="notes-title">Put back. Converting the version changed this:</p>
			<ul>
				{#each notes as note, i (i)}
					<li>{note.message}</li>
				{/each}
			</ul>
		</div>
	{/if}

	<div class="versions-list">
		{#if loading}
			<div class="empty">
				<Icon icon="ri:loader-4-line" width="14" class="spin"/>
			</div>
		{:else if versions.length === 0}
			<div class="empty">
				<span>No versions yet</span>
			</div>
		{:else}
			{#each versions as version}
				<div class="version-row" class:selected={preview?.id === version.id}>
					{#if tree}
						<button
							class="version-info version-open"
							title="Preview this version"
							aria-pressed={preview?.id === version.id}
							onclick={() => handlePreview(version)}
						>
							<span class="version-num">v{version.version_number}</span>
							<span class="version-date">{formatDate(version.created_at)}</span>
							{#if version.created_by === 'ai'}
								<span class="badge-ai">AI</span>
							{:else if version.created_by === 'auto'}
								<span class="badge-auto">Auto</span>
							{/if}
							{#if previewLoading === version.id}
								<Icon icon="ri:loader-4-line" width="12" class="spin" />
							{/if}
						</button>
					{:else}
						<div class="version-info">
							<span class="version-num">v{version.version_number}</span>
							<span class="version-date">{formatDate(version.created_at)}</span>
							{#if version.created_by === 'ai'}
								<span class="badge-ai">AI</span>
							{:else if version.created_by === 'auto'}
								<span class="badge-auto">Auto</span>
							{/if}
						</div>
					{/if}
					<Button
						variant="ghost"
						size="sm"
						loading={restoringId === version.id}
						disabled={restoringId !== null}
						onclick={() => handleRestore(version)}
					>
						Restore
					</Button>
				</div>
			{/each}
		{/if}
	</div>

	{#if preview}
		<div class="preview" aria-label={`Version ${preview.number}`}>
			{#if preview.html !== null}
				<DocumentPreview html={preview.html} />
			{:else}
				<pre class="preview-markdown">{preview.markdown}</pre>
			{/if}
		</div>
	{/if}
</div>

<style>
	.version-panel {
		width: 260px;
		max-height: 320px;
		display: flex;
		flex-direction: column;
	}

	.version-panel.with-preview {
		width: min(560px, calc(100vw - 32px));
		max-height: min(640px, calc(100vh - 96px));
	}

	.panel-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 10px 12px;
		font-size: 12px;
		font-weight: 500;
		color: var(--color-foreground);
		border-bottom: 1px solid var(--color-border);
	}

	.error {
		padding: 8px 12px;
		font-size: 11px;
		color: var(--color-error);
		background: color-mix(in srgb, var(--color-error) 10%, transparent);
	}

	.notes {
		padding: 8px 12px;
		font-size: 12px;
		color: var(--color-foreground);
		border-bottom: 1px solid var(--color-border);
	}

	.notes-title {
		margin: 0 0 4px;
	}

	.notes ul {
		margin: 0;
		padding-left: 16px;
		color: var(--color-foreground-muted);
	}

	.versions-list {
		flex: 1;
		overflow-y: auto;
		max-height: 260px;
	}

	.with-preview .versions-list {
		flex: none;
		max-height: 160px;
	}

	.empty {
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 24px;
		font-size: 12px;
		color: var(--color-foreground-muted);
	}

	.version-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 8px 12px;
		transition: background 100ms;
	}

	.version-row:hover,
	.version-row.selected {
		background: var(--hover-bg);
	}

	.version-info {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
	}

	.version-open {
		flex: 1;
		min-width: 0;
		padding: 0;
		border: none;
		background: none;
		color: inherit;
		font: inherit;
		font-size: 12px;
		text-align: left;
		cursor: pointer;
	}

	.version-num {
		font-weight: 600;
		color: var(--color-foreground);
	}

	.version-date {
		color: var(--color-foreground-muted);
	}

	.badge-ai {
		padding: 1px 4px;
		font-size: 9px;
		font-weight: 600;
		color: var(--color-primary);
		background: color-mix(in srgb, var(--color-primary) 12%, transparent);
		border-radius: 3px;
	}

	.badge-auto {
		padding: 1px 4px;
		font-size: 9px;
		font-weight: 600;
		color: var(--color-foreground-muted);
		background: color-mix(in srgb, var(--color-foreground-muted) 12%, transparent);
		border-radius: 3px;
	}

	.preview {
		flex: 1;
		min-height: 0;
		overflow: auto;
		padding: 12px 16px;
		border-top: 1px solid var(--color-border);
	}

	.preview-markdown {
		margin: 0;
		font-size: 12px;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		color: var(--color-foreground);
	}

	:global(.spin) {
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		from { transform: rotate(0deg); }
		to { transform: rotate(360deg); }
	}
</style>
