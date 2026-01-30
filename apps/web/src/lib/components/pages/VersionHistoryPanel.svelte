<script lang="ts">
	/**
	 * VersionHistoryPanel - Modal for page version history
	 * 
	 * Allows users to:
	 * - Save a new version snapshot
	 * - View version history
	 * - Restore to a previous version
	 */
	import Modal from '$lib/components/Modal.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { listVersions, saveVersion, restoreVersion, type PageVersion } from '$lib/yjs/versions';
	import type { YjsDocument } from '$lib/yjs';

	interface Props {
		open: boolean;
		onClose: () => void;
		pageId: string;
		yjsDoc?: YjsDocument;
	}

	let { open, onClose, pageId, yjsDoc }: Props = $props();

	let versions = $state<PageVersion[]>([]);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let saving = $state(false);
	let restoring = $state(false);
	let description = $state('');

	// Load versions when modal opens
	$effect(() => {
		if (open && pageId) {
			loadVersions();
		}
	});

	async function loadVersions() {
		loading = true;
		error = null;
		try {
			versions = await listVersions(pageId);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load versions';
			versions = [];
		} finally {
			loading = false;
		}
	}

	async function handleSaveVersion() {
		if (!yjsDoc) {
			error = 'No document available';
			return;
		}

		saving = true;
		error = null;
		try {
			const result = await saveVersion(yjsDoc.ydoc, pageId, description || undefined);
			if (result) {
				// Refresh the list
				await loadVersions();
				description = '';
			} else {
				error = 'Failed to save version';
			}
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to save version';
		} finally {
			saving = false;
		}
	}

	async function handleRestore(versionId: string) {
		if (!yjsDoc) {
			error = 'No document available';
			return;
		}

		if (!confirm('Restore this version? Your current content will be replaced.')) {
			return;
		}

		restoring = true;
		error = null;
		try {
			const success = await restoreVersion(yjsDoc, versionId);
			if (success) {
				onClose();
			} else {
				error = 'Failed to restore version';
			}
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to restore version';
		} finally {
			restoring = false;
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

<Modal {open} {onClose} title="Version History" width="md">
	{#snippet children()}
		<!-- Save new version section -->
		<div class="save-section">
			<div class="save-header">
				<Icon icon="ri:save-line" width="16"/>
				<span>Save Current Version</span>
			</div>
			<div class="save-form">
				<input
					type="text"
					class="modal-input"
					placeholder="Version description (optional)"
					bind:value={description}
					disabled={saving}
				/>
				<button
					class="modal-btn modal-btn-primary"
					onclick={handleSaveVersion}
					disabled={saving || !yjsDoc}
				>
					{#if saving}
						<Icon icon="ri:loader-4-line" width="14" class="spin"/>
						Saving...
					{:else}
						Save Snapshot
					{/if}
				</button>
			</div>
		</div>

		{#if error}
			<div class="error-message">
				<Icon icon="ri:error-warning-line" width="14"/>
				{error}
			</div>
		{/if}

		<!-- Version list -->
		<div class="versions-section">
			<div class="versions-header">
				<Icon icon="ri:history-line" width="16"/>
				<span>Previous Versions</span>
				{#if versions.length > 0}
					<span class="count">({versions.length})</span>
				{/if}
			</div>

			{#if loading}
				<div class="loading">
					<Icon icon="ri:loader-4-line" width="16" class="spin"/>
					Loading versions...
				</div>
			{:else if versions.length === 0}
				<div class="empty">
					<Icon icon="ri:archive-line" width="24"/>
					<p>No versions saved yet</p>
					<p class="hint">Save a snapshot to create your first version</p>
				</div>
			{:else}
				<div class="versions-list">
					{#each versions as version}
						<div class="version-item">
							<div class="version-info">
								<div class="version-number">v{version.version_number}</div>
								<div class="version-meta">
									<span class="version-date">{formatDate(version.created_at)}</span>
									{#if version.description}
										<span class="version-desc">{version.description}</span>
									{/if}
								</div>
								{#if version.content_preview}
									<div class="version-preview">{version.content_preview}</div>
								{/if}
							</div>
							<button
								class="restore-btn"
								onclick={() => handleRestore(version.id)}
								disabled={restoring}
								title="Restore this version"
							>
								{#if restoring}
									<Icon icon="ri:loader-4-line" width="14" class="spin"/>
								{:else}
									<Icon icon="ri:refresh-line" width="14"/>
								{/if}
								Restore
							</button>
						</div>
					{/each}
				</div>
			{/if}
		</div>
	{/snippet}
</Modal>

<style>
	.save-section {
		padding-bottom: 16px;
		border-bottom: 1px solid var(--color-border);
		margin-bottom: 16px;
	}

	.save-header {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 13px;
		font-weight: 500;
		color: var(--color-foreground);
		margin-bottom: 12px;
	}

	.save-form {
		display: flex;
		gap: 10px;
	}

	.save-form input {
		flex: 1;
	}

	.error-message {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 10px 12px;
		background: var(--color-error-subtle);
		color: var(--color-error);
		border-radius: 6px;
		font-size: 13px;
		margin-bottom: 16px;
	}

	.versions-section {
		min-height: 200px;
	}

	.versions-header {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 13px;
		font-weight: 500;
		color: var(--color-foreground);
		margin-bottom: 12px;
	}

	.versions-header .count {
		color: var(--color-foreground-muted);
		font-weight: 400;
	}

	.loading {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 8px;
		padding: 40px;
		color: var(--color-foreground-muted);
		font-size: 13px;
	}

	.empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		padding: 40px 20px;
		color: var(--color-foreground-muted);
		text-align: center;
	}

	.empty p {
		margin: 8px 0 0;
		font-size: 14px;
	}

	.empty .hint {
		font-size: 12px;
		opacity: 0.8;
	}

	.versions-list {
		display: flex;
		flex-direction: column;
		gap: 8px;
		max-height: 300px;
		overflow-y: auto;
	}

	.version-item {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 12px;
		padding: 12px;
		background: var(--color-surface-elevated);
		border-radius: 8px;
		transition: background 150ms ease;
	}

	.version-item:hover {
		background: var(--color-surface-overlay);
	}

	.version-info {
		flex: 1;
		min-width: 0;
	}

	.version-number {
		font-size: 12px;
		font-weight: 600;
		color: var(--color-primary);
		margin-bottom: 4px;
	}

	.version-meta {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 12px;
		color: var(--color-foreground-muted);
	}

	.version-date {
		white-space: nowrap;
	}

	.version-desc {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.version-preview {
		margin-top: 6px;
		font-size: 11px;
		color: var(--color-foreground-subtle);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		max-width: 300px;
	}

	.restore-btn {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 10px;
		font-size: 12px;
		font-weight: 500;
		color: var(--color-foreground-muted);
		background: transparent;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		cursor: pointer;
		transition: all 150ms ease;
		white-space: nowrap;
	}

	.restore-btn:hover {
		color: var(--color-foreground);
		background: var(--color-surface);
		border-color: var(--color-foreground-muted);
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
