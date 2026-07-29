<!--
	EntityRecordsSection.svelte

	The entity page's evidence feed: every raw record that references this
	entity (via wiki_entity_refs), newest first — the CRM view of a
	relationship. Ontology chips filter; the grid pages.

	Chips are keyed by DISPLAY name with a set of member source_types, so two
	backend types that collapse to one label ("message:imessage" +
	"message:sms" → Messages) make one chip, not two.
-->

<script lang="ts">
	import UniversalDataGrid, {
		type Column,
	} from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import { getEntityRecords, type EntityRecordApi } from '$lib/wiki/api';
	import { getOntologyName } from '$lib/wiki/ontology';

	interface Props {
		entityId: string;
	}

	let { entityId }: Props = $props();

	type RecordRow = EntityRecordApi;

	let records = $state<EntityRecordApi[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	$effect(() => {
		const id = entityId;
		loading = true;
		error = null;
		getEntityRecords(id)
			.then((r) => {
				records = r;
			})
			.catch((e) => {
				error = e instanceof Error ? e.message : 'Failed to load records';
			})
			.finally(() => {
				loading = false;
			});
	});

	// One chip per display name, carrying every raw source_type it covers.
	interface Chip {
		name: string;
		types: Set<string>;
		count: number;
		continuous: boolean;
	}

	const chips = $derived.by(() => {
		const byName = new Map<string, Chip>();
		for (const r of records) {
			const name = getOntologyName(r.source_type);
			let chip = byName.get(name);
			if (!chip) {
				chip = { name, types: new Set(), count: 0, continuous: r.continuous };
				byName.set(name, chip);
			}
			chip.types.add(r.source_type);
			chip.count += 1;
			chip.continuous = chip.continuous && r.continuous;
		}
		return [...byName.values()].sort((a, b) => a.name.localeCompare(b.name));
	});

	// Discrete streams on by default, continuous measurement streams off.
	let activeNames = $state<Set<string>>(new Set());
	$effect(() => {
		activeNames = new Set(chips.filter((c) => !c.continuous).map((c) => c.name));
	});

	function toggleChip(name: string) {
		const next = new Set(activeNames);
		if (next.has(name)) next.delete(name);
		else next.add(name);
		activeNames = next;
	}

	// Grid rows need a unique id; a record id can repeat across ontologies.
	const visibleRows = $derived<RecordRow[]>(
		records
			.filter((r) => activeNames.has(getOntologyName(r.source_type)))
			.map((r) => ({ ...r, id: `${r.source_type}:${r.id}` }))
	);

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
		},
		{
			key: 'role',
			label: 'Role',
			icon: 'ri:user-shared-line',
			width: '6.5rem',
			hideOnMobile: true,
			getValue: (item) => item.role ?? '—',
		},
		{
			key: 'label',
			label: 'Description',
			icon: 'ri:file-text-line',
		},
		{
			key: 'preview',
			label: 'Detail',
			icon: 'ri:information-line',
			hideOnMobile: true,
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

	<UniversalDataGrid
		items={visibleRows}
		{columns}
		entityType="entity-records"
		{loading}
		{error}
		pageSize={10}
		emptyIcon="ri:database-2-line"
		emptyMessage="No records reference this entity yet"
		loadingMessage="Reading the record..."
		searchPlaceholder="Filter records..."
	/>
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
