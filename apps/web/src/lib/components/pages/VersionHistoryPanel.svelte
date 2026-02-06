<script lang="ts">
	/**
	 * VersionHistoryPanel - Popover content for page version history
	 *
	 * Allows users to save snapshots and restore to previous versions.
	 * Use inside a Popover primitive for proper positioning and dismiss behavior.
	 */
	import Icon from '$lib/components/Icon.svelte';
	import { listVersions, saveVersion, restoreVersion, type PageVersion } from '$lib/yjs/versions';
	import type { YjsDocument } from '$lib/yjs';
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
		if (!confirm('Restore this version? Current content will be replaced.')) return;

		restoringId = versionId;
		error = null;
		try {
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
		const date = new Date(dateString);
		const now = new Date();
		const diffMs = now.getTime() - date.getTime();
		const diffMins = Math.floor(diffMs / 60000);
		const diffHours = Math.floor(diffMs / 3600000);
		const diffDays = Math.floor(diffMs / 86400000);

		if (diffMins < 1) return 'Just now';
		if (diffMins < 60) return `${diffMins}m ago`;
		if (diffHours < 24) return `${diffHours}h ago`;
		if (diffDays < 7) return `${diffDays}d ago`;

		return date.toLocaleDateString(undefined, {
			month: 'short',
			day: 'numeric',
			year: date.getFullYear() !== now.getFullYear() ? 'numeric' : undefined
		});
	}
</script>

<div class="version-panel">
	<div class="panel-header">
		<span>Versions</span>
		<button
			class="save-btn"
			onclick={handleSave}
			disabled={saving || !yjsDoc}
		>
			{#if saving}
				<Icon icon="ri:loader-4-line" width="12" class="spin"/>
			{:else}
				<Icon icon="ri:add-line" width="12"/>
			{/if}
			Save
		</button>
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
						{/if}
					</div>
					<button
						class="restore-btn"
						onclick={() => handleRestore(version.id)}
						disabled={restoringId !== null}
					>
						{#if restoringId === version.id}
							<Icon icon="ri:loader-4-line" width="11" class="spin"/>
						{:else}
							Restore
						{/if}
					</button>
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

	.save-btn {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 4px 8px;
		font-size: 11px;
		font-weight: 500;
		color: var(--color-primary);
		background: transparent;
		border: 1px solid var(--color-border);
		border-radius: 4px;
		cursor: pointer;
		transition: all 100ms;
	}

	.save-btn:hover {
		background: var(--color-surface-elevated);
	}

	.save-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
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
		background: var(--color-surface-elevated);
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

	.restore-btn {
		padding: 3px 8px;
		font-size: 11px;
		font-weight: 500;
		color: var(--color-foreground-muted);
		background: transparent;
		border: 1px solid transparent;
		border-radius: 4px;
		cursor: pointer;
		transition: all 100ms;
	}

	.restore-btn:hover {
		color: var(--color-foreground);
		border-color: var(--color-border);
	}

	.restore-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	:global(.spin) {
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		from { transform: rotate(0deg); }
		to { transform: rotate(360deg); }
	}
</style>
