<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import { onMount } from 'svelte';
	import { pagesStore } from '$lib/stores/pages.svelte';
	import { fade } from 'svelte/transition';

	interface Props {
		onSelect: (pageId: string, pageTitle: string) => void;
		onClose: () => void;
	}

	let { onSelect, onClose }: Props = $props();

	let query = $state('');
	let selectedIndex = $state(0);
	let inputEl: HTMLInputElement | null = $state(null);

	// Filter pages based on search
	const filteredPages = $derived.by(() => {
		const pages = pagesStore.pages || [];
		if (!query.trim()) {
			return pages.slice(0, 8);
		}
		const q = query.toLowerCase();
		return pages.filter((p) => p.title.toLowerCase().includes(q)).slice(0, 8);
	});

	// Reset selection when results change
	$effect(() => {
		if (filteredPages.length > 0 && selectedIndex >= filteredPages.length) {
			selectedIndex = 0;
		}
	});

	onMount(() => {
		inputEl?.focus();
		// Load pages if not already loaded
		pagesStore.loadPages();
	});

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.preventDefault();
			onClose();
		} else if (e.key === 'ArrowDown') {
			e.preventDefault();
			selectedIndex = Math.min(selectedIndex + 1, filteredPages.length - 1);
		} else if (e.key === 'ArrowUp') {
			e.preventDefault();
			selectedIndex = Math.max(selectedIndex - 1, 0);
		} else if (e.key === 'Enter') {
			e.preventDefault();
			const page = filteredPages[selectedIndex];
			if (page) {
				onSelect(page.id, page.title);
			}
		}
	}

	function handleSelect(page: { id: string; title: string }) {
		onSelect(page.id, page.title);
	}
</script>

<svelte:window on:keydown={handleKeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="picker-backdrop" onclick={onClose} transition:fade={{ duration: 100 }}></div>

<div class="page-picker" transition:fade={{ duration: 100 }}>
	<div class="search-row">
		<Icon icon="ri:search-line" width="14" />
		<input
			bind:this={inputEl}
			bind:value={query}
			type="text"
			placeholder="Search pages..."
			class="search-input"
		/>
	</div>

	<div class="results">
		{#if filteredPages.length === 0}
			<div class="empty">
				{query ? 'No pages found' : 'No pages yet'}
			</div>
		{:else}
			{#each filteredPages as page, i (page.id)}
				<button
					class="result-item"
					class:selected={i === selectedIndex}
					onclick={() => handleSelect(page)}
					onmouseenter={() => (selectedIndex = i)}
					type="button"
				>
					<Icon icon="ri:file-text-line" width="14" />
					<span class="page-title">{page.title || 'Untitled'}</span>
				</button>
			{/each}
		{/if}
	</div>

	<div class="footer">
		<span class="hint"><kbd>↑↓</kbd> navigate</span>
		<span class="hint"><kbd>↵</kbd> select</span>
		<span class="hint"><kbd>esc</kbd> close</span>
	</div>
</div>

<style>
	.picker-backdrop {
		position: fixed;
		inset: 0;
		z-index: 100;
	}

	.page-picker {
		position: absolute;
		bottom: 100%;
		left: 0;
		margin-bottom: 8px;
		width: 280px;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		box-shadow: 0 4px 20px rgba(0, 0, 0, 0.15);
		z-index: 101;
		overflow: hidden;
	}

	.search-row {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 10px 12px;
		border-bottom: 1px solid var(--color-border);
		color: var(--color-foreground-muted);
	}

	.search-input {
		flex: 1;
		border: none;
		background: none;
		outline: none;
		font-size: 13px;
		color: var(--color-foreground);
	}

	.search-input::placeholder {
		color: var(--color-foreground-subtle);
	}

	.results {
		max-height: 240px;
		overflow-y: auto;
	}

	.empty {
		padding: 20px;
		text-align: center;
		color: var(--color-foreground-muted);
		font-size: 13px;
	}

	.result-item {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 8px 12px;
		border: none;
		background: none;
		text-align: left;
		cursor: pointer;
		color: var(--color-foreground);
		font-size: 13px;
		transition: background-color 0.1s;
	}

	.result-item:hover,
	.result-item.selected {
		background: var(--color-primary-subtle);
	}

	.result-item :global(svg) {
		color: var(--color-foreground-muted);
		flex-shrink: 0;
	}

	.page-title {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.footer {
		display: flex;
		gap: 12px;
		padding: 8px 12px;
		border-top: 1px solid var(--color-border);
		background: var(--color-surface-elevated);
	}

	.hint {
		font-size: 11px;
		color: var(--color-foreground-subtle);
	}

	.hint kbd {
		display: inline-block;
		padding: 1px 4px;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 3px;
		font-family: inherit;
		font-size: 10px;
	}
</style>
