<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import PagePicker from './PagePicker.svelte';

	interface Props {
		boundPage: { id: string; title: string } | null;
		onChangePage: () => void;
		onClear: () => void;
		onSelectPage?: (pageId: string, pageTitle: string) => void;
	}

	let { boundPage, onChangePage, onClear, onSelectPage }: Props = $props();

	let showPicker = $state(false);

	function handleTogglePicker() {
		if (onSelectPage) {
			showPicker = !showPicker;
		} else {
			onChangePage();
		}
	}

	function handleSelect(pageId: string, pageTitle: string) {
		showPicker = false;
		onSelectPage?.(pageId, pageTitle);
	}

	function handleClose() {
		showPicker = false;
	}
</script>

<div class="page-indicator-wrapper">
	{#if boundPage}
		<div class="page-indicator">
			<Icon icon="ri:edit-line" width="14" />
			<button class="page-name" onclick={handleTogglePicker} type="button">
				{boundPage.title}
			</button>
			<button class="clear-btn" onclick={onClear} title="Stop editing" type="button">
				<Icon icon="ri:close-line" width="14" />
			</button>
		</div>
	{:else}
		<button class="page-activate" onclick={handleTogglePicker} type="button">
			<Icon icon="ri:edit-line" width="14" />
		</button>
	{/if}

	{#if showPicker}
		<PagePicker onSelect={handleSelect} onClose={handleClose} />
	{/if}
</div>

<style>
	.page-indicator-wrapper {
		position: relative;
	}

	.page-indicator {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 4px 8px;
		background: var(--color-primary-subtle);
		border: 1px solid var(--color-primary);
		border-radius: 6px;
		font-size: 12px;
		color: var(--color-primary);
	}

	.page-name {
		color: var(--color-primary);
		background: none;
		border: none;
		cursor: pointer;
		font-weight: 500;
		font-size: 12px;
		max-width: 100px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		padding: 0;
	}

	.page-name:hover {
		text-decoration: underline;
	}

	.clear-btn {
		background: none;
		border: none;
		color: var(--color-primary);
		cursor: pointer;
		padding: 2px;
		border-radius: 4px;
		display: flex;
		align-items: center;
		justify-content: center;
		opacity: 0.7;
	}

	.clear-btn:hover {
		opacity: 1;
		background: var(--color-error-subtle);
		color: var(--color-error);
	}

	.page-activate {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		background: none;
		border: 1px solid transparent;
		border-radius: 6px;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: all 0.15s ease;
	}

	.page-activate:hover {
		background: var(--color-surface-elevated);
		border-color: var(--color-border);
		color: var(--color-foreground);
	}
</style>
