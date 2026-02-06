<script lang="ts">
	/**
	 * PageEditResult
	 *
	 * Displays the result of an edit_page tool call in the chat.
	 * Shows different states: page_created, edit_applied, edit_failed
	 */
	import Icon from '$lib/components/Icon.svelte';
	import { spaceStore } from '$lib/stores/space.svelte';

	interface Props {
		type: 'page_created' | 'edit_applied' | 'edit_failed';
		title?: string;
		description?: string;
		pageId?: string;
		onOpenPage?: (pageId: string) => void;
	}

	let { type, title, description, pageId, onOpenPage }: Props = $props();

	const config = $derived({
		page_created: {
			icon: 'ri:file-add-line',
			label: 'Created page',
			colorClass: 'result-created',
			showOpenButton: true
		},
		edit_applied: {
			icon: 'ri:check-line',
			label: 'Edit applied',
			colorClass: 'result-success',
			showOpenButton: false
		},
		edit_failed: {
			icon: 'ri:error-warning-line',
			label: 'Edit failed',
			colorClass: 'result-error',
			showOpenButton: false
		}
	}[type]);

	function handleOpenPage() {
		if (pageId && onOpenPage) {
			onOpenPage(pageId);
		} else if (pageId) {
			// Fallback to direct store call if handler is missing
			spaceStore.openTabFromRoute(`/page/${pageId}`, { paneId: 'right' });
		}
	}
</script>

<div class="page-edit-result {config.colorClass}">
	<div class="result-content">
		<Icon icon={config.icon} width="16" />
		<span class="result-label">{config.label}</span>
		{#if title}
			<span class="result-title">{title}</span>
		{/if}
		{#if description}
			<span class="result-description">{description}</span>
		{/if}
	</div>
	<div class="result-actions">
		{#if config.showOpenButton && pageId}
			<button class="open-btn" onclick={handleOpenPage} type="button">
				<Icon icon="ri:external-link-line" width="14" />
				Open
			</button>
		{/if}
	</div>
</div>

<style>
	.page-edit-result {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.75rem;
		padding: 0.625rem 0.875rem;
		border-radius: 0.5rem;
		margin: 0.5rem 0;
		font-size: 0.8125rem;
		border: 1px solid;
	}

	.result-content {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex: 1;
		min-width: 0;
	}

	.result-label {
		font-weight: 500;
		flex-shrink: 0;
	}

	.result-title {
		font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
		font-weight: 400;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.result-description {
		color: inherit;
		opacity: 0.8;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.result-actions {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex-shrink: 0;
	}

	/* Color variants */
	.result-created {
		background: var(--color-surface-elevated);
		border-color: var(--color-border);
		color: var(--color-foreground);
	}

	.result-success {
		background: var(--color-surface-elevated);
		border-color: var(--color-success-subtle);
		color: var(--color-success);
	}

	.result-error {
		background: var(--color-surface-elevated);
		border-color: var(--color-error-subtle);
		color: var(--color-error);
	}

	/* Open button */
	.open-btn {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0.375rem 0.625rem;
		background: transparent;
		border: none;
		border-radius: 9999px;
		font-size: 0.75rem;
		font-weight: 500;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: all 0.15s ease;
	}

	.open-btn:hover {
		color: var(--color-foreground);
	}
</style>
