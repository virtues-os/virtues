<!--
	A query's rows as a table. An arbitrary result has no typed columns, so
	this is the one place a chat draws its own table rather than
	UniversalDataGrid, the same exception the developer console takes. A row
	that came from one record carries its ref, and opens it beside the chat.
-->
<script lang="ts">
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { columnLabel, formatCell, hasValues, isNumericColumn, isTimeValues, rowRef, type Row } from "./show";

	interface Props {
		columns: string[];
		rows: Row[];
		/** The query had more rows than the box sent. */
		more: boolean;
	}
	let { columns: all, rows, more }: Props = $props();

	// A column with nothing in it says nothing; it only narrows the others.
	const columns = $derived(all.filter((c) => hasValues(rows, c)));
	const times = $derived(new Set(columns.filter((c) => isTimeValues(rows, c))));

	const FOLDED = 10;
	let open = $state(false);
	const shown = $derived(open ? rows : rows.slice(0, FOLDED));
	const numeric = $derived(new Set(columns.filter((c) => isNumericColumn(rows, c))));

	function openRow(row: Row) {
		const ref = rowRef(row);
		if (ref) windowShellStore.openRouteBeside(ref, formatCell(row[columns[0]]));
	}
</script>

<div class="scroll">
	<table>
		<thead>
			<tr>
				{#each columns as c}
					<th class:num={numeric.has(c)} scope="col">{columnLabel(c)}</th>
				{/each}
			</tr>
		</thead>
		<tbody>
			{#each shown as row}
				{@const ref = rowRef(row)}
				<tr class:linked={!!ref} onclick={ref ? () => openRow(row) : undefined}>
					{#each columns as c, ci}
						<td class:num={numeric.has(c)} class:time={times.has(c)}>
							{#if ci === 0 && ref}
								<button type="button" class="open" onclick={(e) => { e.stopPropagation(); openRow(row); }}>{formatCell(row[c])}</button>
							{:else}
								{formatCell(row[c])}
							{/if}
						</td>
					{/each}
				</tr>
			{/each}
		</tbody>
	</table>
</div>
{#if rows.length > FOLDED || more}
	<div class="foot">
		{#if rows.length > FOLDED}
			<button type="button" class="toggle" onclick={() => (open = !open)}>
				{open ? "Show fewer" : `Show all ${rows.length} rows`}
			</button>
		{/if}
		{#if more && (open || rows.length <= FOLDED)}
			<span>The first {rows.length} rows</span>
		{/if}
	</div>
{/if}

<style>
	.scroll {
		overflow-x: auto;
	}
	table {
		width: 100%;
		border-collapse: collapse;
		font-family: var(--font-sans);
		font-size: 0.875rem;
		font-variant-numeric: tabular-nums;
	}
	th {
		text-align: left;
		font-weight: 400;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		padding: 0 0.75rem 0.375rem 0;
		border-bottom: 1px solid var(--color-border);
		white-space: nowrap;
	}
	td {
		padding: 0.375rem 0.75rem 0.375rem 0;
		border-bottom: 1px solid var(--color-border-subtle);
		color: var(--color-foreground);
		vertical-align: top;
	}
	.num {
		text-align: right;
	}
	.time {
		white-space: nowrap;
	}
	th:last-child,
	td:last-child {
		padding-right: 0;
	}
	tr.linked {
		cursor: pointer;
	}
	tr.linked:hover td {
		background: var(--color-background-hover);
	}
	.open {
		all: unset;
		cursor: pointer;
		color: var(--color-primary);
	}
	.open:focus-visible {
		outline: 2px solid var(--color-border-focus);
		outline-offset: 2px;
	}
	.foot {
		display: flex;
		gap: 1rem;
		align-items: baseline;
		margin-top: 0.5rem;
		font-family: var(--font-sans);
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}
	.toggle {
		all: unset;
		cursor: pointer;
		font-size: 0.875rem;
		color: var(--color-primary);
	}
	.toggle:focus-visible {
		outline: 2px solid var(--color-border-focus);
		outline-offset: 2px;
	}
</style>
