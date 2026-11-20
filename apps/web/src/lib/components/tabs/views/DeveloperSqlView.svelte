<script lang="ts">
	/**
	 * Sources · Query: SQL against your own server, and deleting rows of your
	 * own records.
	 *
	 * Left, the tables: your records (`data_*`), grouped by kind with the
	 * prefix trimmed, then your server's own tables. Picking one writes its
	 * full name into the query and runs it. Right, the query, always open,
	 * and what it returned.
	 *
	 * Every query runs read only (api/developer.ts). Rows can be deleted only
	 * from one of your records' tables, and only while the grid shows that
	 * table as picked from the list: a query typed by hand may join or
	 * rename, and then a row's id says nothing about which row it is.
	 */
	import type { Tab } from '$lib/tabs/types';
	import Button from '$lib/components/Button.svelte';
	import LoadError from '$lib/components/settings/LoadError.svelte';
	import ServerSaid from '$lib/components/settings/ServerSaid.svelte';
	import { getDeveloperTables, ApiError } from '$lib/api/client';
	import { runSql } from '$lib/api/developer';
	import { deleteRows } from '$lib/api/deletions';
	import { confirmAction } from '$lib/stores/dialog.svelte';
	import { toast } from 'svelte-sonner';
	import { onMount } from 'svelte';

	// Sources passes the tab; Query keeps nothing per tab.
	let {}: { tab: Tab; active: boolean } = $props();

	let sql = $state('');
	let columns = $state<string[]>([]);
	let results = $state<Record<string, unknown>[] | null>(null);
	let elapsedMs = $state<number | null>(null);
	let error = $state<string | null>(null);
	let loading = $state(false);

	let tables = $state<string[]>([]);
	let loadingTables = $state(true);
	let tablesError = $state<string | null>(null);
	let selectedTable = $state<string | null>(null);

	let resultsTable = $state<string | null>(null);
	let selected = $state<Set<string>>(new Set());
	let deleting = $state(false);
	let deleteError = $state<string | null>(null);

	/** The kinds of record, by the word after `data_`. */
	const KINDS: Record<string, string> = {
		activity: 'Activity',
		audio: 'Audio',
		calendar: 'Calendar',
		communication: 'Messages and mail',
		content: 'Content',
		environment: 'Weather',
		financial: 'Money',
		health: 'Health',
		location: 'Location',
	};

	const recordGroups = $derived.by(() => {
		const groups = new Map<string, { label: string; tables: { full: string; short: string }[] }>();
		for (const full of tables.filter((t) => t.startsWith('data_')).sort()) {
			const [, kind, ...rest] = full.split('_');
			const label = KINDS[kind] ?? kind.charAt(0).toUpperCase() + kind.slice(1);
			const group = groups.get(label) ?? { label, tables: [] };
			group.tables.push({ full, short: rest.join('_') || kind });
			groups.set(label, group);
		}
		return [...groups.values()].sort((a, b) => a.label.localeCompare(b.label));
	});
	const serverTables = $derived(tables.filter((t) => !t.startsWith('data_')).sort());

	const canDelete = $derived(
		resultsTable !== null && resultsTable.startsWith('data_') && columns.includes('id') && columns.includes('source_stream_id'),
	);
	const rowIds = $derived.by((): string[] => {
		if (!canDelete) return [];
		return (results ?? []).map((r) => r.id).filter((id): id is string => typeof id === 'string');
	});
	const allSelected = $derived(rowIds.length > 0 && rowIds.every((id) => selected.has(id)));

	const summary = $derived.by(() => {
		if (!results) return '';
		const n = results.length;
		const rows = `${n.toLocaleString()} ${n === 1 ? 'row' : 'rows'}`;
		const took = elapsedMs != null ? `, in ${elapsedMs} ms` : '';
		const picked = selected.size ? ` ${selected.size.toLocaleString()} selected.` : '';
		return `${rows}${took}.${picked}`;
	});

	async function loadTables() {
		loadingTables = true;
		tablesError = null;
		try {
			tables = await getDeveloperTables<string[]>();
		} catch (e) {
			tablesError =
				e instanceof ApiError && e.status === 404
					? 'Your server couldn’t find the table list. Restart your server, then open Query again.'
					: `Your server couldn’t list its tables: ${e instanceof Error ? e.message : String(e)}`;
		} finally {
			loadingTables = false;
		}
	}

	onMount(loadTables);

	async function runQuery(fromTable: string | null = null) {
		if (!sql.trim() || loading) return;
		loading = true;
		error = null;
		deleteError = null;
		results = null;
		columns = [];
		resultsTable = fromTable;
		// A query typed by hand is no longer the table picked from the list.
		if (!fromTable) selectedTable = null;
		selected = new Set();
		const started = performance.now();
		try {
			const data = await runSql(sql);
			elapsedMs = Math.round(performance.now() - started);
			columns = data.columns ?? [];
			results = data.rows ?? [];
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	function selectTable(name: string) {
		selectedTable = name;
		sql = `SELECT * FROM ${name} LIMIT 100;`;
		void runQuery(name);
	}

	function toggleRow(id: string) {
		const next = new Set(selected);
		if (!next.delete(id)) next.add(id);
		selected = next;
	}

	function toggleAll() {
		selected = allSelected ? new Set() : new Set(rowIds);
	}

	async function deleteSelected() {
		const table = resultsTable;
		const ids = rowIds.filter((id) => selected.has(id));
		if (!table || ids.length === 0 || deleting) return;
		const rows = `${ids.length} ${ids.length === 1 ? 'row' : 'rows'}`;
		const ok = await confirmAction({
			title: `Delete ${rows} from ${table}?`,
			body:
				'Your server removes them with their search entries, their files and the raw copy each sync kept. ' +
				'It refuses them if a sync sends them again. ' +
				'Copies stay in backups made before today, and the rows can’t be brought back from here.',
			confirmLabel: `Delete ${rows}`,
			danger: true,
		});
		if (!ok) return;
		deleting = true;
		deleteError = null;
		try {
			const receipt = await deleteRows(table, ids);
			const gone = receipt.records ?? 0;
			toast(`Deleted ${gone.toLocaleString()} ${gone === 1 ? 'row' : 'rows'} from ${table}`);
			await runQuery(table);
		} catch (e) {
			deleteError = `Your server didn’t delete those rows: ${e instanceof Error ? e.message : String(e)}`;
		} finally {
			deleting = false;
		}
	}

	function handleKeydown(e: KeyboardEvent) {
		if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
			e.preventDefault();
			void runQuery();
		}
	}

	function cell(value: unknown): string {
		if (value === null || value === undefined) return 'NULL';
		return typeof value === 'object' ? JSON.stringify(value) : String(value);
	}
</script>

{#snippet tableButton(full: string, label: string)}
	<button type="button" class="table" class:active={selectedTable === full} title={full} onclick={() => selectTable(full)}>{label}</button>
{/snippet}

<div class="query">
	<aside class="tables" aria-label="Tables">
		{#if loadingTables}
			<p class="quiet">Reading your tables…</p>
		{:else if tablesError}
			<LoadError message={tablesError} onretry={loadTables} />
		{:else}
			<p class="list-title">Your records</p>
			<p class="list-sub">You can delete rows from these.</p>
			{#each recordGroups as group (group.label)}
				<p class="kind">{group.label}</p>
				{#each group.tables as t (t.full)}{@render tableButton(t.full, t.short)}{/each}
			{/each}
			<p class="list-title server">Your server’s own tables</p>
			{#each serverTables as t (t)}{@render tableButton(t, t)}{/each}
		{/if}
	</aside>

	<section class="main">
		<textarea
			class="sql"
			bind:value={sql}
			onkeydown={handleKeydown}
			rows="4"
			spellcheck="false"
			aria-label="SQL query"
			placeholder="SELECT * FROM data_health_heart_rate LIMIT 100;"
		></textarea>
		<div class="run-row">
			<span class="note">Queries only read.</span>
			<span class="run">
				<kbd>⌘↵</kbd>
				<Button variant="primary" {loading} disabled={!sql.trim()} onclick={() => runQuery()}>Run</Button>
			</span>
		</div>

		{#if error}
			<p class="failed">This query didn’t run.</p>
			<ServerSaid text={error} />
		{:else if results}
			<div class="result-bar">
				<span class="note">{summary}</span>
				{#if canDelete && selected.size > 0}
					<Button variant="danger" loading={deleting} onclick={deleteSelected}>
						Delete {selected.size.toLocaleString()} {selected.size === 1 ? 'row' : 'rows'}
					</Button>
				{/if}
			</div>
			{#if deleteError}<p class="delete-error" role="alert">{deleteError}</p>{/if}
			{#if columns.length}
				<div class="grid">
					<table>
						<thead>
							<tr>
								{#if canDelete}
									<th class="check">
										<input type="checkbox" aria-label="Select every row shown" checked={allSelected} onchange={toggleAll} />
									</th>
								{/if}
								{#each columns as name (name)}<th>{name}</th>{/each}
							</tr>
						</thead>
						<tbody>
							{#each results as row, i (i)}
								{@const id = typeof row.id === 'string' ? row.id : null}
								<tr class:picked={canDelete && id !== null && selected.has(id)}>
									{#if canDelete}
										<td class="check">
											{#if id !== null}
												<input type="checkbox" aria-label="Select this row" checked={selected.has(id)} onchange={() => toggleRow(id)} />
											{/if}
										</td>
									{/if}
									{#each columns as name (name)}
										<td class:null={row[name] === null || row[name] === undefined} class:num={typeof row[name] === 'number'} title={cell(row[name])}
											>{cell(row[name])}</td
										>
									{/each}
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{/if}
		{:else}
			<p class="empty">Pick a table, or write a query and press ⌘↵.</p>
		{/if}
	</section>
</div>

<style>
	.query {
		display: flex;
		height: 100%;
		min-height: 0;
	}

	.tables {
		flex: none;
		width: 248px;
		box-sizing: border-box;
		padding: 16px 12px 24px;
		border-right: 1px solid var(--color-border);
		overflow-y: auto;
	}

	.list-title {
		margin: 0 8px 2px;
		font-size: 13px;
		color: var(--color-foreground);
	}

	.list-title.server {
		margin-top: 24px;
		margin-bottom: 4px;
	}

	.list-sub,
	.kind {
		margin: 0 8px;
		font-size: 12px;
		line-height: 1.4;
		color: var(--color-foreground-muted);
	}

	.kind {
		margin-top: 12px;
		margin-bottom: 2px;
	}

	.table {
		display: block;
		width: 100%;
		height: 28px;
		padding: 0 8px;
		border: none;
		border-radius: var(--radius-sm);
		background: none;
		font: inherit;
		font-size: 13px;
		text-align: left;
		color: var(--color-foreground-muted);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		cursor: pointer;
		transition: color var(--motion-hover-out) var(--ease-premium);
	}

	.table:hover {
		color: var(--color-foreground);
		transition-duration: var(--motion-hover-in);
	}

	.table.active {
		background: color-mix(in srgb, var(--color-primary) 10%, transparent);
		color: var(--color-foreground);
	}

	.main {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		padding: 16px 24px 0;
	}

	.sql {
		box-sizing: border-box;
		width: 100%;
		min-height: 88px;
		padding: 8px 12px;
		border: 1px solid var(--color-border-field);
		border-radius: var(--radius-md);
		background: var(--color-background);
		/* design-ok: the query is code you write, and it lines up in columns. */
		font-family: var(--font-mono);
		font-size: 13px;
		line-height: 1.55;
		color: var(--color-foreground);
		resize: vertical;
	}

	.run-row,
	.result-bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
	}

	.run-row {
		margin-top: 8px;
	}

	.result-bar {
		min-height: 34px;
		margin: 24px 0 8px;
	}

	.run {
		display: flex;
		align-items: center;
		gap: 8px;
		flex: none;
	}

	.note,
	kbd {
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	kbd {
		font-family: inherit;
	}

	.failed {
		margin: 24px 0 0;
		font-size: 15px;
		color: var(--color-foreground);
	}

	.delete-error {
		margin: 0 0 8px;
		font-size: 13px;
		color: var(--color-error);
	}

	.empty {
		margin: 24px 0 0;
		font-size: 14px;
		color: var(--color-foreground-muted);
	}

	.grid {
		flex: 1;
		min-height: 0;
		overflow: auto;
		margin: 0 -24px;
		padding: 0 24px 24px;
	}

	table {
		width: 100%;
		border-collapse: collapse;
	}

	th {
		position: sticky;
		top: 0;
		z-index: 1;
		height: 32px;
		padding: 0 12px;
		border-bottom: 1px solid var(--color-border);
		background: var(--page, var(--color-background));
		font-size: 13px;
		font-weight: 400;
		text-align: left;
		color: var(--color-foreground-muted);
		white-space: nowrap;
	}

	td {
		max-width: 280px;
		height: 32px;
		padding: 0 12px;
		border-bottom: 1px solid var(--color-border);
		/* design-ok: a cell is the value exactly as the database returned it. */
		font-family: var(--font-mono);
		font-size: 12px;
		color: var(--color-foreground);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	td.num {
		text-align: right;
		font-variant-numeric: tabular-nums;
	}

	td.null {
		color: var(--color-foreground-muted);
	}

	.check {
		width: 20px;
		padding-right: 0;
	}

	.check input {
		accent-color: var(--color-primary);
	}

	tr.picked td {
		background: color-mix(in srgb, var(--color-primary) 6%, transparent);
	}

	/* On a phone the list sits above the query, in a box of its own, and the
	   keyboard shortcut goes: there is no keyboard to press it on. */
	@media (max-width: 720px) {
		.query {
			flex-direction: column;
		}

		.tables {
			width: auto;
			max-height: 200px;
			border-right: none;
			border-bottom: 1px solid var(--color-border);
		}

		.main {
			min-height: 0;
			flex: 1;
			padding: 16px 16px 0;
		}

		.grid {
			margin: 0 -16px;
			padding: 0 16px 24px;
		}

		kbd {
			display: none;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.table {
			transition: none;
		}
	}
</style>
