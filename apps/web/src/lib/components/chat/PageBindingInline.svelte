<script lang="ts">
	/**
	 * PageBindingInline
	 *
	 * Inline component shown when the AI needs user to bind a page.
	 * Used for:
	 * - Newly created pages that need binding
	 * - Requests to edit a page when none is bound
	 */

	interface Props {
		pageId?: string;
		pageTitle?: string;
		message?: string;
		onBind: (pageId: string, title: string) => void;
	}

	let { pageId, pageTitle, message, onBind }: Props = $props();

	const displayTitle = $derived(pageTitle || 'Untitled');
	const displayMessage = $derived(message || 'Select a page to edit');

	function handleBind() {
		if (pageId) {
			onBind(pageId, displayTitle);
		}
	}
</script>

<div class="page-binding-inline">
	<div class="binding-content">
		<div class="binding-icon">
			<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
				<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
				<polyline points="14 2 14 8 20 8" />
				<line x1="16" y1="13" x2="8" y2="13" />
				<line x1="16" y1="17" x2="8" y2="17" />
				<polyline points="10 9 9 9 8 9" />
			</svg>
		</div>
		<div class="binding-text">
			<span class="binding-message">{displayMessage}</span>
			{#if pageId}
				<span class="binding-page-title">{displayTitle}</span>
			{/if}
		</div>
	</div>
	{#if pageId}
		<button class="bind-btn" onclick={handleBind} type="button">
			Open & Edit
		</button>
	{/if}
</div>

<style>
	.page-binding-inline {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.75rem;
		padding: 0.75rem 1rem;
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border);
		border-radius: 0.5rem;
		margin: 0.75rem 0;
	}

	.binding-content {
		display: flex;
		align-items: center;
		gap: 0.625rem;
	}

	.binding-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--color-accent);
		flex-shrink: 0;
	}

	.binding-text {
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
	}

	.binding-message {
		font-size: 0.8125rem;
		color: var(--color-text-secondary);
	}

	.binding-page-title {
		font-size: 0.875rem;
		font-weight: 500;
		color: var(--color-text);
	}

	.bind-btn {
		padding: 0.375rem 0.75rem;
		background: var(--color-accent);
		color: var(--color-accent-contrast);
		border: none;
		border-radius: 0.375rem;
		font-size: 0.8125rem;
		font-weight: 500;
		cursor: pointer;
		transition: opacity 0.15s ease;
		white-space: nowrap;
	}

	.bind-btn:hover {
		opacity: 0.9;
	}
</style>
