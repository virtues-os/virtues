<script lang="ts">
	import type { StreamObject, ObjectContent } from '$lib/api/client';
	import 'iconify-icon';

	let {
		object = null,
		content = null,
		loading = false,
		error = null,
		open = false,
		onClose
	} = $props<{
		object: StreamObject | null;
		content: ObjectContent | null;
		loading: boolean;
		error: string | null;
		open: boolean;
		onClose: () => void;
	}>();

	let panelEl: HTMLElement | null = $state(null);
	let closeButtonEl: HTMLButtonElement | null = $state(null);

	// Focus the close button when panel opens
	$effect(() => {
		if (open && closeButtonEl) {
			requestAnimationFrame(() => {
				closeButtonEl?.focus();
			});
		}
	});

	// Handle backdrop click
	function handleBackdropClick(e: MouseEvent) {
		if (e.target === e.currentTarget) {
			onClose();
		}
	}

	// Handle keyboard navigation
	function handleKeyDown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			onClose();
			return;
		}

		// Focus trap
		if (e.key === 'Tab' && panelEl) {
			const focusableElements = panelEl.querySelectorAll<HTMLElement>(
				'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
			);
			const firstElement = focusableElements[0];
			const lastElement = focusableElements[focusableElements.length - 1];

			if (e.shiftKey && document.activeElement === firstElement) {
				e.preventDefault();
				lastElement?.focus();
			} else if (!e.shiftKey && document.activeElement === lastElement) {
				e.preventDefault();
				firstElement?.focus();
			}
		}
	}

	function formatBytes(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
		return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
	}

	function formatTimestamp(ts: string | null): string {
		if (!ts) return '—';
		return new Date(ts).toLocaleString();
	}
</script>

<svelte:window onkeydown={handleKeyDown} />

{#if open && object}
	<!-- Backdrop -->
	<div class="panel-backdrop" onclick={handleBackdropClick} role="presentation">
		<!-- Panel -->
		<aside
			bind:this={panelEl}
			class="storage-panel"
			role="dialog"
			aria-label="Storage object details"
			aria-modal="true"
		>
			<!-- Header -->
			<header class="panel-header">
				<div class="header-content">
					<iconify-icon icon="ri:database-2-line" class="text-primary" width="24" height="24"
					></iconify-icon>
					<div class="header-text">
						<h2 class="panel-title">{object.stream_name}</h2>
						<span class="panel-subtitle">{object.source_name}</span>
					</div>
				</div>
				<button
					bind:this={closeButtonEl}
					class="close-button"
					onclick={onClose}
					aria-label="Close panel"
				>
					<iconify-icon icon="ri:close-line" width="20" height="20"></iconify-icon>
				</button>
			</header>

			<!-- Content -->
			<div class="panel-content">
				<!-- Metadata Section -->
				<div class="section">
					<h3 class="section-title">Object Metadata</h3>
					<div class="detail-grid">
						<div class="detail-item">
							<span class="detail-label">Records</span>
							<span class="detail-value">{object.record_count.toLocaleString()}</span>
						</div>
						<div class="detail-item">
							<span class="detail-label">Size</span>
							<span class="detail-value">{formatBytes(object.size_bytes)}</span>
						</div>
						<div class="detail-item">
							<span class="detail-label">Min Timestamp</span>
							<span class="detail-value">{formatTimestamp(object.min_timestamp)}</span>
						</div>
						<div class="detail-item">
							<span class="detail-label">Max Timestamp</span>
							<span class="detail-value">{formatTimestamp(object.max_timestamp)}</span>
						</div>
					</div>
				</div>

				<!-- S3 Key Section -->
				<div class="section">
					<h3 class="section-title">S3 Key</h3>
					<div class="s3-key">{object.s3_key}</div>
				</div>

				<!-- Content Section -->
				<div class="section content-section">
					<h3 class="section-title">
						Decrypted Content
						{#if content}
							<span class="record-count">({content.record_count} records)</span>
						{/if}
					</h3>

					{#if loading}
						<div class="loading-state">
							<iconify-icon
								icon="ri:loader-4-line"
								class="animate-spin text-primary"
								width="24"
								height="24"
							></iconify-icon>
							<span>Decrypting and loading content...</span>
						</div>
					{:else if error}
						<div class="error-state">
							<iconify-icon
								icon="ri:error-warning-line"
								class="text-error"
								width="20"
								height="20"
							></iconify-icon>
							<span>{error}</span>
						</div>
					{:else if content && content.records.length > 0}
						<div class="records-container">
							{#each content.records as record, i}
								<details class="record-item" open={i === 0}>
									<summary class="record-summary">
										Record {i + 1}
									</summary>
									<pre class="record-json">{JSON.stringify(record, null, 2)}</pre>
								</details>
							{/each}
						</div>
					{:else if content && content.records.length === 0}
						<div class="empty-state">
							<span class="text-foreground-muted">No records in this object</span>
						</div>
					{/if}
				</div>
			</div>
		</aside>
	</div>
{/if}

<style>
	.panel-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.3);
		z-index: 100;
		display: flex;
		justify-content: flex-end;
		animation: backdrop-fade-in 0.2s ease-out;
	}

	@keyframes backdrop-fade-in {
		from {
			opacity: 0;
		}
		to {
			opacity: 1;
		}
	}

	.storage-panel {
		width: 100%;
		max-width: 600px;
		height: 100%;
		background: var(--color-surface, #ffffff);
		border-left: 1px solid var(--color-border, #e5e5e5);
		display: flex;
		flex-direction: column;
		animation: panel-slide-in 0.25s ease-out;
		overflow: hidden;
	}

	@keyframes panel-slide-in {
		from {
			transform: translateX(100%);
		}
		to {
			transform: translateX(0);
		}
	}

	.panel-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 16px 20px;
		border-bottom: 1px solid var(--color-border, #e5e5e5);
		background: var(--color-surface-elevated, #fafafa);
	}

	.header-content {
		display: flex;
		align-items: center;
		gap: 12px;
	}

	.header-text {
		display: flex;
		flex-direction: column;
	}

	.panel-title {
		font-size: 1rem;
		font-weight: 600;
		color: var(--color-foreground, #171717);
		margin: 0;
	}

	.panel-subtitle {
		font-size: 0.75rem;
		color: var(--color-foreground-muted, #737373);
	}

	.close-button {
		padding: 8px;
		border: none;
		background: transparent;
		color: var(--color-foreground-muted, #737373);
		cursor: pointer;
		border-radius: 6px;
		transition: all 0.15s;
	}

	.close-button:hover {
		background: var(--color-border, #e5e5e5);
		color: var(--color-foreground, #171717);
	}

	.panel-content {
		flex: 1;
		overflow-y: auto;
		padding: 20px;
	}

	.section {
		margin-bottom: 24px;
	}

	.content-section {
		flex: 1;
		display: flex;
		flex-direction: column;
	}

	.section-title {
		font-size: 0.75rem;
		font-weight: 600;
		color: var(--color-foreground-muted, #737373);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		margin: 0 0 12px 0;
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.record-count {
		font-weight: 400;
		text-transform: none;
		letter-spacing: normal;
	}

	.detail-grid {
		display: grid;
		grid-template-columns: repeat(2, 1fr);
		gap: 12px;
	}

	.detail-item {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.detail-label {
		font-size: 0.6875rem;
		color: var(--color-foreground-muted, #a3a3a3);
		text-transform: uppercase;
		letter-spacing: 0.025em;
	}

	.detail-value {
		font-size: 0.875rem;
		color: var(--color-foreground, #171717);
	}

	.s3-key {
		font-family: 'IBM Plex Mono', monospace;
		font-size: 0.75rem;
		background: var(--color-surface-elevated, #fafafa);
		padding: 12px;
		border-radius: 6px;
		word-break: break-all;
		color: var(--color-foreground-muted, #525252);
		border: 1px solid var(--color-border, #e5e5e5);
	}

	.loading-state,
	.error-state,
	.empty-state {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 24px;
		justify-content: center;
		color: var(--color-foreground-muted, #737373);
		font-size: 0.875rem;
	}

	.error-state {
		color: var(--color-error, #ef4444);
		background: var(--color-error-subtle, #fef2f2);
		border-radius: 6px;
	}

	.records-container {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.record-item {
		border: 1px solid var(--color-border, #e5e5e5);
		border-radius: 6px;
		overflow: hidden;
	}

	.record-summary {
		padding: 10px 12px;
		background: var(--color-surface-elevated, #fafafa);
		cursor: pointer;
		font-size: 0.8125rem;
		font-weight: 500;
		color: var(--color-foreground-muted, #525252);
	}

	.record-summary:hover {
		background: var(--color-border, #e5e5e5);
	}

	.record-json {
		font-family: 'IBM Plex Mono', monospace;
		font-size: 0.75rem;
		padding: 12px;
		margin: 0;
		background: var(--color-surface, #ffffff);
		overflow-x: auto;
		white-space: pre-wrap;
		word-break: break-word;
		color: var(--color-foreground, #171717);
		max-height: 300px;
		overflow-y: auto;
	}
</style>
