<!--
	SourceConnectButton.svelte

	Button + anchored popover for picking a source to connect. Used in two
	places on the Sources page:
	  - the page header (small / right-aligned popover)
	  - the empty-state hero (large / centered popover)

	The popover is a thin router. Each pick fires `onPick(source)` and the
	parent dispatches the actual flow (OAuth redirect, QR pair modal, or
	api-key form modal). Click outside to dismiss; no backdrop, no focus
	trap — this is a 1-click affordance, not a wizard.
-->

<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import Button from '$lib/components/Button.svelte';
	import Badge from '$lib/components/Badge.svelte';
	import Popover from '$lib/floating/primitives/Popover.svelte';
	import type { SourceCatalogItem } from '$lib/api/client';

	interface Props {
		catalog: SourceCatalogItem[];
		onPick: (source: SourceCatalogItem) => void;
		/** Visual style of the trigger button. */
		variant?: 'primary' | 'ghost';
		/** Trigger label. */
		label?: string;
		/** Where the popover anchors relative to the trigger. */
		align?: 'right' | 'center';
	}

	let {
		catalog,
		onPick,
		variant = 'primary',
		label = 'Connect',
		align = 'right'
	}: Props = $props();

	let open = $state(false);
	let searchInputEl = $state<HTMLInputElement | null>(null);
	let query = $state('');

	// Reset + focus search whenever the popover opens. Search hidden when the
	// catalog is small enough to scan at a glance.
	const SEARCH_THRESHOLD = 8;

	$effect(() => {
		if (!open) return;
		query = '';
		// Defer focus until the popover is in the DOM.
		queueMicrotask(() => searchInputEl?.focus());
	});

	const visibleCatalog = $derived.by(() => {
		const q = query.trim().toLowerCase();
		if (!q) return catalog;
		return catalog.filter(
			(s) =>
				s.name.toLowerCase().includes(q) ||
				s.id.toLowerCase().includes(q) ||
				(s.description ?? '').toLowerCase().includes(q)
		);
	});

	function handlePick(source: SourceCatalogItem) {
		open = false;
		onPick(source);
	}

	function onSearchKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter' && visibleCatalog.length > 0) {
			e.preventDefault();
			handlePick(visibleCatalog[0]);
		} else if (e.key === 'Escape') {
			open = false;
		}
	}
</script>

<div class="connect-wrapper" class:align-center={align === 'center'}>
	<Popover bind:open placement={align === 'center' ? 'bottom' : 'bottom-end'} offset={6}>
		{#snippet trigger({ toggle })}
			<Button {variant} onclick={toggle}>
				<Icon icon="ri:add-line" width="16" />
				{label}
			</Button>
		{/snippet}
		{#snippet children()}
			<div class="popover" role="menu">
				<div class="popover-header">
					<span>Connect a source</span>
				</div>
				{#if catalog.length >= SEARCH_THRESHOLD}
					<div class="search-row">
						<Icon icon="ri:search-line" width="14" />
						<input
							bind:this={searchInputEl}
							type="text"
							placeholder="Search sources…"
							bind:value={query}
							onkeydown={onSearchKeydown}
						/>
					</div>
				{/if}
				{#if visibleCatalog.length === 0}
					<div class="empty-row">No sources match "{query}"</div>
				{/if}
				<ul class="source-list">
					{#each visibleCatalog as source (source.id)}
						<li>
							<button type="button" class="source-row" onclick={() => handlePick(source)}>
								<div class="row-icon">
									<Icon icon={source.icon ?? 'ri:plug-line'} width="16" />
								</div>
								<div class="row-body">
									<div class="row-title-line">
										<span class="row-title">{source.name}</span>
										{#if source.credential_count > 0}
											<Badge variant="success">{source.credential_count}</Badge>
										{/if}
									</div>
									{#if source.description}
										<div class="row-desc">{source.description}</div>
									{/if}
								</div>
							</button>
						</li>
					{/each}
				</ul>
			</div>
		{/snippet}
	</Popover>
</div>

<style>
	.connect-wrapper {
		position: relative;
		display: inline-block;
	}
	.connect-wrapper.align-center {
		display: inline-flex;
		flex-direction: column;
		align-items: center;
	}
	/* Popover (positioning handled by the floating primitive) */
	.popover {
		width: 320px;
		max-height: min(60vh, 480px);
		overflow-y: auto;
		background: var(--color-surface, #fff);
		border: 1px solid var(--color-border, #e5e7eb);
		border-radius: 10px;
		box-shadow: 0 8px 28px rgba(0, 0, 0, 0.08), 0 2px 6px rgba(0, 0, 0, 0.04);
	}

	.popover-header {
		padding: 0.625rem 0.875rem 0.5rem;
		font-size: 0.6875rem;
		font-weight: 600;
		color: var(--color-foreground-subtle, #9ca3af);
		border-bottom: 1px solid var(--color-border-subtle, #f3f4f6);
	}

	.search-row {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.375rem 0.625rem;
		border-bottom: 1px solid var(--color-border-subtle, #f3f4f6);
		color: var(--color-foreground-subtle, #9ca3af);
	}
	.search-row input {
		flex: 1;
		background: transparent;
		border: none;
		outline: none;
		font: inherit;
		font-size: 0.8125rem;
		color: var(--color-foreground, #111827);
	}
	.search-row input::placeholder {
		color: var(--color-foreground-subtle, #9ca3af);
	}

	.empty-row {
		padding: 0.625rem 0.875rem;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle, #9ca3af);
	}

	.source-list {
		list-style: none;
		padding: 0.25rem;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 1px;
	}

	.source-row {
		display: grid;
		grid-template-columns: 28px 1fr;
		align-items: flex-start;
		gap: 0.625rem;
		width: 100%;
		padding: 0.5rem 0.625rem;
		background: transparent;
		border: none;
		border-radius: 6px;
		cursor: pointer;
		text-align: left;
		font: inherit;
		color: inherit;
		transition: background 100ms ease;
	}
	.source-row:hover,
	.source-row:focus-visible {
		background: var(--color-surface-elevated, #f3f4f6);
		outline: none;
	}

	.row-icon {
		display: grid;
		place-items: center;
		width: 28px;
		height: 28px;
		border-radius: var(--radius-full);
		background: var(--color-surface-elevated, #f3f4f6);
		color: var(--color-foreground-muted, #6b7280);
	}
	.source-row:hover .row-icon {
		background: var(--color-surface, #fff);
	}

	.row-body {
		display: flex;
		flex-direction: column;
		gap: 0.0625rem;
		min-width: 0;
	}
	.row-title-line {
		display: flex;
		align-items: center;
		gap: 0.375rem;
	}
	.row-title {
		font-size: 0.8125rem;
		font-weight: 500;
		color: var(--color-foreground, #111827);
	}
	.row-desc {
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle, #9ca3af);
		line-height: 1.3;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
</style>
