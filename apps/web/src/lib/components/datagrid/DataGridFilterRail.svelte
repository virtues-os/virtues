<!--
	DataGridFilterRail.svelte

	Horizontal rail of active filter chips. The "+ Filter" trigger lives in
	the toolbar (orchestrator-owned) so the chip family stays separated
	from the control family. The orchestrator owns filterValues and adds.
-->

<script lang="ts" generics="T">
	import DataGridFilterChip from './DataGridFilterChip.svelte';
	import type { FilterDef, FilterOption, FilterValue } from './types';

	interface Props {
		filters: FilterDef<T>[];
		filterValues: Record<string, FilterValue>;
		asyncOptionsCache: Record<string, FilterOption[]>;
		justAddedId?: string | null;
		onChange: (id: string, next: FilterValue) => void;
		onClear: (id: string) => void;
		onLoadAsync: (id: string) => Promise<FilterOption[]>;
	}

	let {
		filters,
		filterValues,
		asyncOptionsCache,
		justAddedId = null,
		onChange,
		onClear,
		onLoadAsync
	}: Props = $props();

	const activeFilters = $derived(filters.filter((f) => f.id in filterValues));
</script>

{#if activeFilters.length > 0}
	<div class="filter-rail">
		{#each activeFilters as def (def.id)}
			<DataGridFilterChip
				{def}
				value={filterValues[def.id] ?? null}
				loadedOptions={asyncOptionsCache[def.id]}
				autoOpen={justAddedId === def.id}
				onChange={(next) => onChange(def.id, next)}
				onClear={() => onClear(def.id)}
				onLoadAsync={() => onLoadAsync(def.id)}
			/>
		{/each}
	</div>
{/if}

<style>
	.filter-rail {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		flex-wrap: wrap;
		gap: 0.75rem;
		padding: 0 0 0.625rem 0;
	}
</style>
