<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import { onMount } from 'svelte';

	interface PageOption {
		id: string;
		title: string;
	}

	interface Props {
		/** Called when user selects a page and clicks Allow Editing */
		onBind: (pageId: string, pageTitle: string) => void;
		/** Called when user dismisses the request */
		onDismiss?: () => void;
	}

	let { onBind, onDismiss }: Props = $props();

	let pages = $state<PageOption[]>([]);
	let selectedPageId = $state<string>('');
	let loading = $state(true);
	let error = $state<string | null>(null);

	onMount(async () => {
		try {
			const response = await fetch('/api/pages?limit=50');
			if (!response.ok) throw new Error('Failed to load pages');
			const data = await response.json();
			pages = data.pages || [];
			if (pages.length > 0) {
				selectedPageId = pages[0].id;
			}
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load pages';
		} finally {
			loading = false;
		}
	});

	function handleBind() {
		const page = pages.find((p) => p.id === selectedPageId);
		if (page) {
			onBind(page.id, page.title);
		}
	}
</script>

<div class="binding-request">
	<div class="header">
		<Icon icon="ri:edit-line" width="16" />
		<span>Select a page to edit</span>
	</div>

	{#if loading}
		<div class="loading">
			<Icon icon="ri:loader-4-line" width="16" class="animate-spin" />
			Loading pages...
		</div>
	{:else if error}
		<div class="error">{error}</div>
	{:else if pages.length === 0}
		<div class="empty">No pages found. Create a page first.</div>
	{:else}
		<div class="controls">
			<select bind:value={selectedPageId} class="page-select">
				{#each pages as page}
					<option value={page.id}>{page.title}</option>
				{/each}
			</select>
			<button class="bind-btn" onclick={handleBind} type="button">
				<Icon icon="ri:check-line" width="14" />
				Allow Editing
			</button>
		</div>
	{/if}

	{#if onDismiss}
		<button class="dismiss-btn" onclick={onDismiss} type="button" title="Dismiss">
			<Icon icon="ri:close-line" width="14" />
		</button>
	{/if}
</div>

<style>
	.binding-request {
		position: relative;
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 16px;
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		margin: 8px 0;
	}

	.header {
		display: flex;
		align-items: center;
		gap: 8px;
		font-weight: 500;
		color: var(--color-foreground);
	}

	.loading,
	.error,
	.empty {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 14px;
		color: var(--color-foreground-muted);
	}

	.error {
		color: var(--color-error);
	}

	.controls {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.page-select {
		flex: 1;
		padding: 8px 12px;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: var(--color-surface);
		color: var(--color-foreground);
		font-size: 14px;
		cursor: pointer;
	}

	.page-select:focus {
		outline: none;
		border-color: var(--color-primary);
	}

	.bind-btn {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 8px 16px;
		background: var(--color-primary);
		color: white;
		border: none;
		border-radius: 6px;
		font-size: 14px;
		font-weight: 500;
		cursor: pointer;
		transition: background-color 0.15s ease;
	}

	.bind-btn:hover {
		background: var(--color-primary-hover);
	}

	.dismiss-btn {
		position: absolute;
		top: 8px;
		right: 8px;
		background: none;
		border: none;
		color: var(--color-foreground-muted);
		cursor: pointer;
		padding: 4px;
		border-radius: 4px;
	}

	.dismiss-btn:hover {
		background: var(--color-surface-hover);
		color: var(--color-foreground);
	}

	:global(.animate-spin) {
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		from {
			transform: rotate(0deg);
		}
		to {
			transform: rotate(360deg);
		}
	}
</style>
