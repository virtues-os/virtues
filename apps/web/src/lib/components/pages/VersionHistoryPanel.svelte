<script lang="ts">
	/**
	 * VersionHistoryPanel - Popover content for page version history
	 *
	 * Allows users to save snapshots and restore to previous versions.
	 * Use inside a Popover primitive for proper positioning and dismiss behavior.
	 */
	import Icon from '$lib/components/Icon.svelte';
	import Button from '$lib/components/Button.svelte';
	import { confirmAction } from '$lib/stores/dialog.svelte';
	import { formatTimeAgo } from '$lib/utils/dateUtils';
	import type { YjsDocument } from '$lib/yjs';
	import { listVersions, restoreVersion, saveVersion, type PageVersion } from '$lib/yjs/versions';
	import { onMount } from 'svelte';

	interface Props {
		close: () => void;
		pageId: string;
		yjsDoc?: YjsDocument;
	}

	let { close, pageId, yjsDoc }: Props = $props();

	let versions = $state<PageVersion[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let saving = $state(false);
	let restoringId = $state<string | null>(null);

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
		if (!yjsDoc) return;
		saving = true;
		error = null;
		try {
			const result = await saveVersion(yjsDoc.ydoc, pageId);
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

	async function handleRestore(versionId: string) {
		if (!yjsDoc) return;
		const ok = await confirmAction({
			title: 'Restore this version?',
			body: 'The current content is replaced. A snapshot of it is saved first, so this is undoable.',
			confirmLabel: 'Restore',
		});
		if (!ok) return;

		restoringId = versionId;
		error = null;
		try {
			// Snapshot current state before restoring so the user can undo the restore
			await saveVersion(yjsDoc.ydoc, pageId, 'Auto-saved before restore', 'auto');

			const success = await restoreVersion(yjsDoc, versionId);
			if (success) {
				close();
			} else {
				error = 'Failed to restore';
			}
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to restore';
		} finally {
			restoringId = null;
		}
	}

	function formatDate(dateString: string): string {
		return formatTimeAgo(dateString);
	}
</script>

<div class="version-panel">
	<div class="panel-header">
		<span>Versions</span>
		<Button
			variant="secondary"
			size="sm"
			icon="ri:add-line"
			loading={saving}
			disabled={!yjsDoc}
			onclick={handleSave}
		>
			Save
		</Button>
	</div>

	{#if error}
		<div class="error">{error}</div>
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
				<div class="version-row">
					<div class="version-info">
						<span class="version-num">v{version.version_number}</span>
						<span class="version-date">{formatDate(version.created_at)}</span>
						{#if version.created_by === 'ai'}
							<span class="badge-ai">AI</span>
						{:else if version.created_by === 'auto'}
							<span class="badge-auto">Auto</span>
						{/if}
					</div>
					<Button
						variant="ghost"
						size="sm"
						loading={restoringId === version.id}
						disabled={restoringId !== null}
						onclick={() => handleRestore(version.id)}
					>
						Restore
					</Button>
				</div>
			{/each}
		{/if}
	</div>
</div>

<style>
	.version-panel {
		width: 260px;
		max-height: 320px;
		display: flex;
		flex-direction: column;
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

	.versions-list {
		flex: 1;
		overflow-y: auto;
		max-height: 260px;
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

	.version-row:hover {
		background: var(--hover-bg);
	}

	.version-info {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
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

	:global(.spin) {
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		from { transform: rotate(0deg); }
		to { transform: rotate(360deg); }
	}
</style>
