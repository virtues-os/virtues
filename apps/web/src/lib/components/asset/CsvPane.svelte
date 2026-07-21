<script lang="ts">
	// CSV/TSV pane for AssetView: parses client-side and renders through
	// UniversalDataGrid, so a spreadsheet file gets the same sortable grid
	// chrome as every other list surface (the list doctrine). First row is
	// treated as the header. Byte/row/column caps keep huge exports snappy;
	// every cap is surfaced in a banner, never silent.
	import Icon from "$lib/components/Icon.svelte";
	import UniversalDataGrid, {
		type Column,
	} from "$lib/components/datagrid/UniversalDataGrid.svelte";
	import { fetchTextCapped, parseDelimited, sniffDelimiter } from "./text";

	let { url, filename }: { url: string; filename: string } = $props();

	const FETCH_CAP = 4 * 1024 * 1024;
	const MAX_ROWS = 10_000;
	const MAX_COLS = 64;

	type CsvRow = { id: string } & Record<string, string>;

	let loading = $state(true);
	let error = $state<string | null>(null);
	let columns = $state<Column<CsvRow>[]>([]);
	let items = $state<CsvRow[]>([]);
	let capNotes = $state<string[]>([]);

	$effect(() => {
		const target = url;
		loading = true;
		error = null;
		columns = [];
		items = [];
		capNotes = [];
		fetchTextCapped(target, FETCH_CAP)
			.then((r) => {
				const notes: string[] = [];
				const delimiter = sniffDelimiter(r.text, filename);
				// +1 row for the header; +1 more so we can detect the row cap.
				const rows = parseDelimited(r.text, delimiter, MAX_ROWS + 2);
				if (r.truncated) {
					// The stream was cut mid-file; the last row may be partial.
					rows.pop();
					notes.push("first 4 MB of the file");
				}
				if (rows.length === 0) {
					error = "Empty file";
					return;
				}
				const header = rows[0];
				let colCount = header.length;
				if (colCount > MAX_COLS) {
					colCount = MAX_COLS;
					notes.push(`first ${MAX_COLS} of ${header.length} columns`);
				}
				let dataRows = rows.slice(1);
				if (dataRows.length > MAX_ROWS) {
					dataRows = dataRows.slice(0, MAX_ROWS);
					notes.push(`first ${MAX_ROWS.toLocaleString()} rows`);
				}
				columns = Array.from({ length: colCount }, (_, i) => ({
					key: `c${i}` as keyof CsvRow,
					label: header[i]?.trim() || `Column ${i + 1}`,
				}));
				items = dataRows.map((row, rowIndex) => {
					const item: CsvRow = { id: String(rowIndex) };
					for (let i = 0; i < colCount; i++) item[`c${i}`] = row[i] ?? "";
					return item;
				});
				capNotes = notes;
			})
			.catch((e) => {
				error = e instanceof Error ? e.message : "Failed to load file";
			})
			.finally(() => {
				loading = false;
			});
	});
</script>

<div class="csv-pane">
	{#if capNotes.length > 0}
		<div class="csv-banner">
			<Icon icon="ri:scissors-cut-line" width="13" />
			Large file — showing the {capNotes.join(", ")}. Download for the full contents.
		</div>
	{/if}
	<div class="csv-grid">
		<UniversalDataGrid
			{items}
			{columns}
			entityType="csv-file"
			{loading}
			{error}
			emptyIcon="ri:table-line"
			emptyMessage="No rows"
			searchPlaceholder="Filter rows..."
			pageSize={50}
			defaultViewMode="table"
			animateMount={false}
		/>
	</div>
</div>

<style>
	.csv-pane {
		display: flex;
		flex-direction: column;
		width: 100%;
		height: 100%;
		min-height: 0;
	}

	.csv-banner {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 14px;
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		border-bottom: 1px solid var(--color-border);
		flex-shrink: 0;
	}

	.csv-grid {
		flex: 1;
		min-height: 0;
		overflow: auto;
		padding: 0 14px 14px;
	}
</style>
