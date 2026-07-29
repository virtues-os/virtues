<!--
	EntityRecordsSection.svelte

	The entity page's evidence feed: every raw record that references this
	entity (via wiki_entity_refs), newest first — the CRM view of a
	relationship. Server-paginated: the grid holds one page; search, paging
	and the ontology chips all travel to the box as a query, so an entity
	with a hundred thousand records costs one page at a time.

	Chips come from a dedicated facets endpoint (counting loaded rows would
	lie once the grid stopped loading everything), keyed by DISPLAY name with
	a set of member raw source_types — "message:imessage" + "message:sms"
	make one Messages chip, not two.
-->

<script lang="ts">
	import UniversalDataGrid, {
		type Column,
	} from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import type { GridQuery, GridPage } from '$lib/components/datagrid/types';
	import {
		getEntityRecordsPage,
		getEntityRecordFacets,
		type EntityRecordApi,
	} from '$lib/wiki/api';
	import { getOntologyName } from '$lib/wiki/ontology';

	interface Props {
		entityId: string;
	}

	let { entityId }: Props = $props();

	type RecordRow = EntityRecordApi;

	// One chip per display name, carrying every raw source_type it covers.
	interface Chip {
		name: string;
		types: string[];
		count: number;
		continuous: boolean;
	}

	let chips = $state<Chip[]>([]);
	let facetsLoaded = $state(false);
	// Discrete streams on by default, continuous measurement streams off.
	let activeNames = $state<Set<string>>(new Set());

	$effect(() => {
		const id = entityId;
		facetsLoaded = false;
		getEntityRecordFacets(id)
			.then((facets) => {
				const byName = new Map<string, Chip>();
				for (const f of facets) {
					const name = getOntologyName(f.source_type);
					let chip = byName.get(name);
					if (!chip) {
						chip = { name, types: [], count: 0, continuous: f.continuous };
						byName.set(name, chip);
					}
					chip.types.push(f.source_type);
					chip.count += f.count;
					chip.continuous = chip.continuous && f.continuous;
				}
				chips = [...byName.values()].sort((a, b) => a.name.localeCompare(b.name));
				activeNames = new Set(chips.filter((c) => !c.continuous).map((c) => c.name));
			})
			.finally(() => {
				facetsLoaded = true;
			});
	});

	function toggleChip(name: string) {
		const next = new Set(activeNames);
		if (next.has(name)) next.delete(name);
		else next.add(name);
		activeNames = next;
	}

	/** The raw source_types the active chips cover, stable-sorted. */
	const activeTypes = $derived(
		chips
			.filter((c) => activeNames.has(c.name))
			.flatMap((c) => c.types)
			.sort()
	);

	/** Everything on: no narrowing needed — send the empty allowlist. */
	const allOn = $derived(chips.length > 0 && activeNames.size === chips.length);

	// Part of the grid's cache key: a different entity or chip set is a
	// different result set, never a cache hit.
	const serverExtra = $derived({ entity: entityId, types: allOn ? [] : activeTypes });

	async function fetchPage(q: GridQuery): Promise<GridPage<RecordRow>> {
		const page = await getEntityRecordsPage(entityId, {
			offset: q.offset,
			limit: q.limit,
			search: q.search || undefined,
			types: allOn ? undefined : activeTypes,
			// "When" is the only server-sortable column; anything else stays
			// newest-first.
			dir: q.sort?.key === 'timestamp' && q.sort.dir === 'asc' ? 'asc' : 'desc',
		});
		return {
			// Grid rows need a unique id; a record id can repeat across ontologies.
			items: page.items.map((r) => ({ ...r, id: `${r.source_type}:${r.id}` })),
			total: page.total,
		};
	}

	function formatWhen(iso: string): string {
		const d = new Date(iso);
		return d.toLocaleDateString('en-US', {
			month: 'short',
			day: 'numeric',
			year: 'numeric',
		});
	}

	const columns: Column<RecordRow>[] = [
		{
			key: 'timestamp',
			label: 'When',
			icon: 'ri:time-line',
			width: '7.5rem',
			getValue: (item) => formatWhen(item.timestamp),
		},
		{
			key: 'source_type',
			label: 'Ontology',
			icon: 'ri:database-2-line',
			width: '10rem',
			getValue: (item) => getOntologyName(item.source_type),
			sortable: false,
		},
		{
			key: 'role',
			label: 'Role',
			icon: 'ri:user-shared-line',
			width: '6.5rem',
			hideOnMobile: true,
			getValue: (item) => item.role ?? '—',
			sortable: false,
		},
		{
			key: 'label',
			label: 'Description',
			icon: 'ri:file-text-line',
			sortable: false,
		},
		{
			key: 'preview',
			label: 'Detail',
			icon: 'ri:information-line',
			hideOnMobile: true,
			sortable: false,
		},
	];
</script>

<div class="records">
	{#if chips.length > 0}
		<div class="chip-row" role="group" aria-label="Filter by ontology">
			{#each chips as chip (chip.name)}
				<button
					class="chip"
					class:on={activeNames.has(chip.name)}
					onclick={() => toggleChip(chip.name)}
					aria-pressed={activeNames.has(chip.name)}
				>
					{chip.name}
					<span class="chip-count">{chip.count}</span>
				</button>
			{/each}
		</div>
	{/if}

	{#if facetsLoaded}
		<UniversalDataGrid
			items={[]}
			{columns}
			entityType="entity-records"
			server={fetchPage}
			{serverExtra}
			pageSize={10}
			emptyIcon="ri:database-2-line"
			emptyMessage="No records reference this entity yet"
			loadingMessage="Reading the record..."
			searchPlaceholder="Filter records..."
		/>
	{/if}
</div>

<style>
	.records {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}

	.chip-row {
		display: flex;
		flex-wrap: wrap;
		gap: 0.375rem;
	}

	.chip {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		font: inherit;
		font-size: 0.75rem;
		padding: 0.25rem 0.625rem;
		border-radius: 999px;
		border: 1px solid var(--color-border);
		background: none;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		transition: all 0.12s ease;
	}

	.chip.on {
		border-color: var(--color-foreground-muted);
		background: var(--color-surface-elevated);
		color: var(--color-foreground);
	}

	.chip-count {
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-subtle);
	}
</style>
