<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { onMount } from "svelte";
	import Icon from "$lib/components/Icon.svelte";
	import { getView, resolveView, executeSql, addViewItem, removeViewItem, reorderViewItems } from "$lib/api/client";
	import type { View, ViewEntity, SqlResult } from "$lib/api/client";
	import { spaceStore } from "$lib/stores/space.svelte";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// View data
	let view = $state<View | null>(null);
	let entities = $state<ViewEntity[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	// Smart folder SQL state
	let sql = $state("");
	let sqlResults = $state<SqlResult | null>(null);
	let sqlError = $state<string | null>(null);
	let sqlLoading = $state(false);

	// Extract view ID from route: /view/view_xxx
	const viewId = $derived.by(() => {
		const match = tab.route.match(/^\/view\/(view_[^/]+)$/);
		return match?.[1] || '';
	});

	// Template SQL examples for new smart folders
	const SQL_TEMPLATES = [
		{ label: 'Recent Chats', sql: "SELECT '/chat/' || id AS id, title AS name, 'ri:chat-1-line' AS icon, updated_at FROM chats ORDER BY updated_at DESC LIMIT 20" },
		{ label: 'Recent Pages', sql: "SELECT '/page/' || id AS id, title AS name, 'ri:file-text-line' AS icon, updated_at FROM pages ORDER BY updated_at DESC LIMIT 20" },
		{ label: 'All People', sql: "SELECT '/person/' || id AS id, canonical_name AS name, 'ri:user-line' AS icon FROM wiki_people ORDER BY canonical_name ASC LIMIT 50" },
	];

	onMount(async () => {
		await loadView();
	});

	async function loadView() {
		if (!viewId) return;
		loading = true;
		error = null;

		try {
			const [viewData, resolution] = await Promise.all([
				getView(viewId),
				resolveView(viewId),
			]);
			view = viewData;
			entities = resolution.entities;

			// For smart views, extract query from query_config
			if (viewData.view_type === 'smart' && viewData.query_config) {
				try {
					const config = JSON.parse(viewData.query_config);
					if (config.raw_sql) {
						sql = config.raw_sql;
					} else if (config.namespace) {
						// System smart view — show the namespace-based query as read-only info
						sql = `-- System smart view: namespace="${config.namespace}", limit=${config.limit || 50}`;
					}
				} catch {
					// ignore parse errors
				}
			}
		} catch (e: unknown) {
			error = e instanceof Error ? e.message : 'Failed to load view';
		} finally {
			loading = false;
		}
	}

	async function runSql() {
		if (!sql.trim() || sql.startsWith('--')) return;
		sqlLoading = true;
		sqlError = null;
		sqlResults = null;

		try {
			sqlResults = await executeSql(sql);
		} catch (e: unknown) {
			sqlError = e instanceof Error ? e.message : 'Query failed';
		} finally {
			sqlLoading = false;
		}
	}

	function handleSqlKeydown(e: KeyboardEvent) {
		if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
			e.preventDefault();
			runSql();
		}
	}

	async function handleRemoveItem(url: string) {
		if (!view) return;
		try {
			await removeViewItem(view.id, url);
			entities = entities.filter(e => e.id !== url);
			spaceStore.invalidateViewCache();
		} catch (e) {
			console.error('[FolderView] Failed to remove item:', e);
		}
	}

	function openEntity(entity: ViewEntity) {
		const href = entity.id.startsWith('/') ? entity.id : `/${entity.namespace}/${entity.id}`;
		spaceStore.openTabFromRoute(href, {
			label: entity.name,
			preferEmptyPane: true,
		});
	}

	function useTemplate(templateSql: string) {
		sql = templateSql;
		runSql();
	}

	const isSmartView = $derived(view?.view_type === 'smart');
	const isSystemView = $derived(view?.is_system ?? false);
	const viewIcon = $derived(view?.icon || (isSmartView ? 'ri:filter-line' : 'ri:folder-line'));
	const viewName = $derived(view?.name || 'Folder');
</script>

<div class="folder-view" class:active>
	{#if loading}
		<div class="loading-state">
			<Icon icon="ri:loader-4-line" width="20" class="spinner" />
			<span>Loading...</span>
		</div>
	{:else if error}
		<div class="error-state">
			<Icon icon="ri:error-warning-line" width="20" />
			<span>{error}</span>
			<button onclick={loadView}>Retry</button>
		</div>
	{:else}
		<!-- Header -->
		<div class="view-header">
			<Icon icon={viewIcon} width="20" />
			<h2>{viewName}</h2>
			<span class="item-count">{entities.length} items</span>
		</div>

		<!-- Smart folder: SQL section -->
		{#if isSmartView}
			<div class="sql-section">
				<div class="sql-header">
					<span class="sql-label">Query</span>
					{#if !isSystemView}
						<button class="run-btn" onclick={runSql} disabled={sqlLoading}>
							{sqlLoading ? 'Running...' : 'Run'}
						</button>
					{/if}
				</div>
				{#if isSystemView}
					<pre class="sql-readonly">{sql}</pre>
				{:else}
					<textarea
						class="sql-editor"
						bind:value={sql}
						onkeydown={handleSqlKeydown}
						placeholder="SELECT id, name, icon, updated_at FROM ..."
						rows="4"
					></textarea>
					{#if !sql.trim()}
						<div class="sql-templates">
							<span class="templates-label">Templates:</span>
							{#each SQL_TEMPLATES as tmpl}
								<button class="template-btn" onclick={() => useTemplate(tmpl.sql)}>
									{tmpl.label}
								</button>
							{/each}
						</div>
					{/if}
				{/if}
				{#if sqlError}
					<div class="sql-error">{sqlError}</div>
				{/if}
			</div>

			<!-- SQL Results table -->
			{#if sqlResults}
				<div class="results-section">
					<div class="results-header">
						<span>{sqlResults.row_count} results</span>
					</div>
					<div class="results-table-wrap">
						<table class="results-table">
							<thead>
								<tr>
									{#each sqlResults.columns as col}
										<th>{col}</th>
									{/each}
								</tr>
							</thead>
							<tbody>
								{#each sqlResults.rows as row}
									<tr>
										{#each sqlResults.columns as col}
											<td>{row[col] ?? ''}</td>
										{/each}
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				</div>
			{/if}
		{/if}

		<!-- Items list -->
		<div class="items-section">
			{#if !isSmartView || !sqlResults}
				<div class="items-header">
					<span class="items-label">Items</span>
				</div>
			{/if}
			{#if entities.length === 0}
				<div class="empty-items">
					{#if isSmartView}
						No matches found
					{:else}
						This folder is empty. Drag items here from the sidebar.
					{/if}
				</div>
			{:else}
				<div class="items-list">
					{#each entities as entity (entity.id)}
						<div class="item-row">
							<button class="item-main" onclick={() => openEntity(entity)}>
								<Icon icon={entity.icon || 'ri:file-line'} width="16" />
								<span class="item-name">{entity.name}</span>
								{#if entity.updated_at}
									<span class="item-date">{new Date(entity.updated_at).toLocaleDateString()}</span>
								{/if}
							</button>
							{#if !isSystemView && !isSmartView}
								<button
									class="item-remove"
									title="Remove from folder"
									onclick={() => handleRemoveItem(entity.id)}
								>
									<Icon icon="ri:close-line" width="14" />
								</button>
							{/if}
						</div>
					{/each}
				</div>
			{/if}
		</div>
	{/if}
</div>

<style>
	.folder-view {
		display: flex;
		flex-direction: column;
		height: 100%;
		overflow-y: auto;
		padding: 24px;
		gap: 16px;
	}

	.loading-state, .error-state {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 24px;
		color: var(--color-foreground-subtle);
	}

	.error-state button {
		margin-left: 8px;
		padding: 4px 12px;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: transparent;
		color: var(--color-foreground);
		cursor: pointer;
	}

	.view-header {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.view-header h2 {
		font-size: 20px;
		font-weight: 500;
		margin: 0;
		color: var(--color-foreground);
	}

	.item-count {
		font-size: 13px;
		color: var(--color-foreground-subtle);
		margin-left: auto;
	}

	/* SQL Section */
	.sql-section {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 12px;
		border: 1px solid var(--color-border);
		border-radius: 8px;
		background: color-mix(in srgb, var(--color-background) 50%, var(--color-surface));
	}

	.sql-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}

	.sql-label {
		font-size: 12px;
		font-weight: 500;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--color-foreground-subtle);
	}

	.run-btn {
		padding: 4px 12px;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: transparent;
		color: var(--color-foreground);
		font-size: 12px;
		cursor: pointer;
	}

	.run-btn:hover {
		background: var(--color-surface);
	}

	.sql-readonly {
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		line-height: 1.5;
		color: var(--color-foreground-subtle);
		white-space: pre-wrap;
		margin: 0;
		padding: 8px;
		background: transparent;
	}

	.sql-editor {
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		line-height: 1.5;
		padding: 8px;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: var(--color-background);
		color: var(--color-foreground);
		resize: vertical;
		min-height: 60px;
	}

	.sql-editor:focus {
		outline: 2px solid var(--color-primary);
		outline-offset: -1px;
	}

	.sql-templates {
		display: flex;
		align-items: center;
		gap: 6px;
		flex-wrap: wrap;
	}

	.templates-label {
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}

	.template-btn {
		padding: 2px 8px;
		border: 1px solid var(--color-border);
		border-radius: 4px;
		background: transparent;
		color: var(--color-foreground-subtle);
		font-size: 11px;
		cursor: pointer;
	}

	.template-btn:hover {
		background: var(--color-surface);
		color: var(--color-foreground);
	}

	.sql-error {
		font-size: 12px;
		color: var(--color-danger, #ef4444);
		padding: 4px 0;
	}

	/* Results table */
	.results-section {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.results-header {
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}

	.results-table-wrap {
		overflow-x: auto;
		border: 1px solid var(--color-border);
		border-radius: 8px;
	}

	.results-table {
		width: 100%;
		border-collapse: collapse;
		font-size: 12px;
	}

	.results-table th {
		text-align: left;
		padding: 8px 12px;
		font-weight: 500;
		color: var(--color-foreground-subtle);
		border-bottom: 1px solid var(--color-border);
		background: color-mix(in srgb, var(--color-surface) 50%, transparent);
	}

	.results-table td {
		padding: 6px 12px;
		border-bottom: 1px solid color-mix(in srgb, var(--color-border) 50%, transparent);
		color: var(--color-foreground);
		max-width: 300px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.results-table tbody tr:hover {
		background: color-mix(in srgb, var(--color-surface) 30%, transparent);
	}

	/* Items list */
	.items-section {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.items-header {
		display: flex;
		align-items: center;
	}

	.items-label {
		font-size: 12px;
		font-weight: 500;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--color-foreground-subtle);
	}

	.empty-items {
		padding: 24px;
		text-align: center;
		color: var(--color-foreground-subtle);
		font-size: 13px;
	}

	.items-list {
		display: flex;
		flex-direction: column;
		border: 1px solid var(--color-border);
		border-radius: 8px;
		overflow: hidden;
	}

	.item-row {
		display: flex;
		align-items: center;
		border-bottom: 1px solid color-mix(in srgb, var(--color-border) 50%, transparent);
	}

	.item-row:last-child {
		border-bottom: none;
	}

	.item-main {
		flex: 1;
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 12px;
		background: transparent;
		border: none;
		color: var(--color-foreground);
		font-size: 13px;
		cursor: pointer;
		text-align: left;
	}

	.item-main:hover {
		background: color-mix(in srgb, var(--color-surface) 50%, transparent);
	}

	.item-name {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.item-date {
		font-size: 11px;
		color: var(--color-foreground-subtle);
		flex-shrink: 0;
	}

	.item-remove {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		margin-right: 4px;
		border: none;
		border-radius: 4px;
		background: transparent;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		opacity: 0;
	}

	.item-row:hover .item-remove {
		opacity: 1;
	}

	.item-remove:hover {
		background: color-mix(in srgb, var(--color-danger, #ef4444) 15%, transparent);
		color: var(--color-danger, #ef4444);
	}

	@keyframes spin {
		from { transform: rotate(0deg); }
		to { transform: rotate(360deg); }
	}

	:global(.spinner) {
		animation: spin 1s linear infinite;
	}
</style>
