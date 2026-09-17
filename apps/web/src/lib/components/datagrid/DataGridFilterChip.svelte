<!--
	DataGridFilterChip.svelte

	One pill for an active filter. Click body to open dropdown to change the
	value. Click × to clear. Dropdown content varies by filter kind:
	  - enum:  single-select list of options
	  - multi: several options at once, each row ticked
	  - async: lazy-loaded options + optional search input
-->

<script lang="ts" generics="T">
	import Icon from '$lib/components/Icon.svelte';
	import IconButton from '$lib/components/IconButton.svelte';
	import MenuItem from '$lib/components/MenuItem.svelte';
	import Popover from '$lib/floating/primitives/Popover.svelte';
	import type { FilterDef, FilterOption, FilterValue } from './types';
	import { describeFilter, isFilterActive } from './types';

	interface Props {
		def: FilterDef<T>;
		value: FilterValue;
		loadedOptions?: FilterOption[];
		autoOpen?: boolean;
		onChange: (next: FilterValue) => void;
		onClear: () => void;
		onLoadAsync?: () => Promise<FilterOption[]>;
	}

	let {
		def,
		value,
		loadedOptions,
		autoOpen = false,
		onChange,
		onClear,
		onLoadAsync
	}: Props = $props();

	// svelte-ignore state_referenced_locally
	let open = $state(autoOpen);

	$effect(() => {
		if (autoOpen) void ensureAsyncLoaded();
	});
	// svelte-ignore state_referenced_locally
	let asyncOptions = $state<FilterOption[]>(loadedOptions ?? []);
	let asyncLoading = $state(false);
	let asyncQuery = $state('');

	// Sync from parent cache when it updates (parent owns the cache).
	$effect(() => {
		if (loadedOptions && loadedOptions.length > 0 && asyncOptions.length === 0) {
			asyncOptions = loadedOptions;
		}
	});

	const removable = $derived(def.removable !== false);
	const active = $derived(isFilterActive(value));
	const labelText = $derived(active ? describeFilter(def, value, asyncOptions) : def.label);

	// Auto-clear when popover closes with no value selected and no default — avoids
	// "ghost chips" left in the rail that don't filter anything.
	let wasOpen = $state(false);
	$effect(() => {
		if (wasOpen && !open) {
			if (!isFilterActive(value) && def.defaultValue === undefined && removable) {
				onClear();
			}
		}
		wasOpen = open;
	});

	async function ensureAsyncLoaded() {
		if (def.kind !== 'async') return;
		if (asyncOptions.length > 0 || asyncLoading) return;
		asyncLoading = true;
		try {
			asyncOptions = onLoadAsync ? await onLoadAsync() : await def.loadOptions();
		} finally {
			asyncLoading = false;
		}
	}

	function pickEnum(opt: FilterOption) {
		onChange(opt.value);
		open = false;
	}

	function toggleMulti(opt: FilterOption) {
		const arr = Array.isArray(value) ? [...value] : [];
		const i = arr.indexOf(opt.value);
		if (i === -1) arr.push(opt.value);
		else arr.splice(i, 1);
		onChange(arr.length === 0 ? null : arr);
	}

	function isMultiChecked(opt: FilterOption): boolean {
		return Array.isArray(value) && value.includes(opt.value);
	}

	const visibleAsyncOptions = $derived.by(() => {
		if (def.kind !== 'async') return asyncOptions;
		const q = asyncQuery.trim().toLowerCase();
		if (!q) return asyncOptions;
		return asyncOptions.filter((o) => o.label.toLowerCase().includes(q));
	});

	function badgeFor(opt: FilterOption): string {
		return opt.badgeColor ?? '';
	}
</script>

<span class="filter-chip" class:active>
	<Popover bind:open placement="bottom-start" offset={4}>
		{#snippet trigger({ toggle: triggerToggle })}
			<button
				type="button"
				class="chip-body"
				class:active
				onclick={() => {
					triggerToggle();
					if (!open) void ensureAsyncLoaded();
				}}
				aria-haspopup="menu"
				aria-expanded={open}
			>
				<span class="chip-label">{labelText}</span>
			</button>
		{/snippet}
		{#snippet children()}
			<div class="popover" role="menu">
				{#if def.kind === 'async' && def.searchable}
					<div class="popover-search">
						<Icon icon="ri:search-line" width="14" />
						<input
							type="text"
							bind:value={asyncQuery}
							placeholder={def.placeholder ?? 'Search…'}
						/>
					</div>
				{/if}

				{#if def.kind === 'async' && asyncLoading}
					<div class="popover-state">Loading…</div>
				{:else if def.kind === 'async' && asyncOptions.length === 0}
					<div class="popover-state">No options</div>
				{:else if def.kind === 'enum'}
					{#each def.options as opt}
						{#snippet dot()}
							<span class="dot {badgeFor(opt)}"></span>
						{/snippet}
						<MenuItem
							role="menuitemradio"
							label={opt.label}
							checked={value === opt.value}
							leading={opt.badgeColor ? dot : undefined}
							onclick={() => pickEnum(opt)}
						/>
					{/each}
				{:else if def.kind === 'multi'}
					{#each def.options as opt}
						{#snippet dot()}
							<span class="dot {badgeFor(opt)}"></span>
						{/snippet}
						<MenuItem
							role="menuitemcheckbox"
							label={opt.label}
							checked={isMultiChecked(opt)}
							leading={opt.badgeColor ? dot : undefined}
							onclick={() => toggleMulti(opt)}
						/>
					{/each}
				{:else if def.kind === 'async'}
					{#each visibleAsyncOptions as opt}
						<MenuItem
							role="menuitemradio"
							label={opt.label}
							checked={value === opt.value}
							onclick={() => pickEnum(opt)}
						/>
					{/each}
				{/if}
			</div>
		{/snippet}
	</Popover>
	{#if active && removable}
		<IconButton icon="ri:close-line" label="Remove filter" size="xs" onclick={onClear} />
	{/if}
</span>

<style>
	.filter-chip {
		position: relative;
		display: inline-flex;
		align-items: center;
		gap: 0.125rem;
		font-size: 0.75rem;
		line-height: 1;
		white-space: nowrap;
		flex-shrink: 0;
	}

	.chip-body {
		display: inline-flex;
		align-items: center;
		padding: 0.25rem 0.375rem;
		font: inherit;
		color: var(--color-foreground-muted);
		background: transparent;
		border: none;
		border-radius: 4px;
		cursor: pointer;
	}

	.filter-chip.active .chip-body {
		color: var(--color-primary);
	}

	.chip-body:hover {
		color: var(--color-foreground);
		background: var(--color-background-hover);
	}

	.filter-chip.active .chip-body:hover {
		color: var(--color-primary);
		background: color-mix(in srgb, var(--color-primary) 10%, transparent);
	}

	.chip-label {
		max-width: 220px;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.chip-body:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}

	/* Popover (positioning handled by the floating primitive) */
	.popover {
		min-width: 200px;
		max-width: 320px;
		max-height: 280px;
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		padding: 0.25rem;
		background: var(--color-background, #fff);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		box-shadow: 0 8px 24px rgba(0, 0, 0, 0.08);
	}

	.popover-search {
		display: flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.25rem 0.5rem;
		border-bottom: 1px solid var(--color-border);
	}

	.popover-search :global(svg) {
		color: var(--color-foreground-subtle);
		flex-shrink: 0;
	}

	.popover-search input {
		flex: 1;
		font: inherit;
		font-size: 0.8125rem;
		padding: 0.25rem 0;
		border: none;
		background: transparent;
		color: var(--color-foreground);
	}

	.popover-search input:focus {
		outline: none;
	}

	.popover-state {
		padding: 0.5rem;
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
	}

	.dot {
		display: inline-block;
		width: 8px;
		height: 8px;
		border-radius: var(--radius-full);
		flex-shrink: 0;
	}

	.dot.badge-muted { background: color-mix(in srgb, var(--color-foreground) 30%, transparent); }
	.dot.badge-success { background: var(--color-success); }
	.dot.badge-error { background: var(--color-error); }
	.dot.badge-warning { background: var(--color-warning); }
	.dot.badge-info { background: var(--color-info); }
	.dot.badge-primary { background: var(--color-primary); }
</style>
