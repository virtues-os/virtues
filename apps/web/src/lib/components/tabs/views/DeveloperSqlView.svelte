<script lang="ts">
    import type { Tab } from "$lib/tabs/types";
    import Icon from "$lib/components/Icon.svelte";
    import { Button, IconButton } from "$lib";
    import { getDeveloperTables, ApiError } from "$lib/api/client";
    import { deleteRows } from "$lib/api/deletions";
    import { confirmAction } from "$lib/stores/dialog.svelte";
    import { toast } from "svelte-sonner";
    import { onMount } from "svelte";

    let { tab, active }: { tab: Tab; active: boolean } = $props();

    let sql = $state("SELECT * FROM sources LIMIT 10;");
    let columns: string[] = $state([]);
    let results: any[] | null = $state(null);
    let error: string | null = $state(null);
    let loading = $state(false);
    let tables: string[] = $state([]);
    let loadingTables = $state(true);
    let queryExpanded = $state(false);
    let selectedTable = $state<string | null>(null);

    // Deleting rows. Only a data table's rows can go, and only while the grid
    // shows that table as picked from the list: a query typed by hand may
    // join or rename, and then a row's id says nothing about which row it is.
    let resultsTable = $state<string | null>(null);
    let selected = $state<Set<string>>(new Set());
    let deleting = $state(false);
    const canDelete = $derived(
        resultsTable !== null &&
            resultsTable.startsWith("data_") &&
            columns.includes("id") &&
            columns.includes("source_stream_id"),
    );
    const rowIds = $derived.by((): string[] => {
        if (!canDelete) return [];
        const rows = (results ?? []) as Record<string, unknown>[];
        return rows.map((r) => r.id).filter((id): id is string => typeof id === "string");
    });
    const allSelected = $derived(rowIds.length > 0 && rowIds.every((id) => selected.has(id)));

    onMount(async () => {
        try {
            tables = await getDeveloperTables<string[]>();
        } catch (e: any) {
            if (e instanceof ApiError) {
                console.error("Failed to load tables:", e.status, e.message);
                if (e.status === 404) {
                    error =
                        "Your server couldn't find the table list. Restart your server, then open this tab again.";
                } else {
                    error = `Failed to load tables: ${e.status} ${e.message}`;
                }
            } else {
                console.error("Failed to load tables", e);
                error = `Network error loading tables: ${e.message}`;
            }
        } finally {
            loadingTables = false;
        }
    });

    async function runQuery(fromTable: string | null = null) {
        loading = true;
        error = null;
        results = null;
        columns = [];
        resultsTable = fromTable;
        selected = new Set();

        try {
            const response = await fetch("/api/developer/sql", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ sql }),
            });

            const data = await response.json();

            if (!response.ok) {
                throw new Error(data.error || "Query failed");
            }

            columns = data.columns || [];
            results = data.rows || [];
        } catch (e: any) {
            error = e.message;
        } finally {
            loading = false;
        }
    }

    function selectTable(tableName: string) {
        selectedTable = tableName;
        sql = `SELECT * FROM ${tableName} LIMIT 100;`;
        runQuery(tableName);
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
        const rows = `${ids.length} ${ids.length === 1 ? "row" : "rows"}`;
        const ok = await confirmAction({
            title: `Delete ${rows} from ${table}?`,
            body:
                "Your server removes them with their search entries, their files and the raw copy each sync kept. " +
                "It refuses them if a sync sends them again. " +
                "Copies stay in backups made before today, and the rows can't be brought back from here.",
            confirmLabel: `Delete ${rows}`,
            danger: true,
        });
        if (!ok) return;
        deleting = true;
        try {
            const receipt = await deleteRows(table, ids);
            const gone = receipt.records ?? 0;
            toast(`Deleted ${gone.toLocaleString()} ${gone === 1 ? "row" : "rows"} from ${table}`);
            await runQuery(table);
        } catch (e: any) {
            error = `Your server didn't delete those rows: ${e?.message ?? e}`;
        } finally {
            deleting = false;
        }
    }

    function handleKeydown(e: KeyboardEvent) {
        if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
            e.preventDefault();
            runQuery();
        }
    }
</script>

<div class="flex h-full w-full bg-surface">
    <!-- Sidebar: Table List -->
    <div class="w-52 flex-none border-r border-border bg-surface-elevated">
        <div class="flex h-[53px] items-center border-b border-border px-4">
            <h2 class="text-sm text-foreground-muted">Tables</h2>
        </div>
        <div class="h-[calc(100%-53px)] overflow-y-auto p-2">
            {#if loadingTables}
                <div class="flex items-center justify-center p-4">
                    <Icon icon="mynaui:spinner" width="16" class="spin" />
                </div>
            {:else if error && tables.length === 0}
                <div class="p-2 text-xs text-destructive">{error}</div>
            {:else if tables.length === 0}
                <div class="p-2 text-xs text-foreground-muted">
                    No tables found
                </div>
            {:else}
                <div class="flex flex-col gap-0.5">
                    {#each tables as table}
                        <button
                            onclick={() => selectTable(table)}
                            class="table-item"
                            class:active={selectedTable === table}
                        >
                            <Icon
                                icon="mynaui:table"
                                class="shrink-0 text-xs"
                            />
                            {table}
                        </button>
                    {/each}
                </div>
            {/if}
        </div>
    </div>

    <!-- Main Content: Header + Results -->
    <div class="flex flex-1 flex-col overflow-hidden">
        <!-- Header Bar (matches Tables header height) -->
        <div
            class="flex h-[53px] flex-none items-center justify-between border-b border-border bg-surface px-4"
        >
            <button
                onclick={() => (queryExpanded = !queryExpanded)}
                class="flex min-w-0 items-center gap-2 text-sm text-foreground-muted hover:text-foreground"
            >
                <Icon
                    icon={queryExpanded
                        ? "mynaui:chevron-down"
                        : "mynaui:chevron-right"}
                    class="text-lg"
                />
                <span>Query</span>
                {#if sql && !queryExpanded}
                    <span
                        class="ml-2 min-w-0 max-w-[300px] truncate font-mono text-xs text-foreground-muted/60"
                    >
                        {sql}
                    </span>
                {/if}
            </button>
            <div class="flex flex-none items-center gap-2 pl-3">
                {#if canDelete && selected.size > 0}
                    <Button
                        variant="danger"
                        icon="mynaui:trash"
                        loading={deleting}
                        onclick={deleteSelected}
                    >
                        Delete {selected.size} {selected.size === 1 ? "row" : "rows"}
                    </Button>
                {/if}
                <span class="whitespace-nowrap text-xs text-warning">Read-only queries</span>
                <div class="info-wrapper">
                    <IconButton
                        icon="mynaui:info-circle"
                        label="What you can change here"
                        size="nested"
                    />
                    <div class="info-popover">
                        <div class="info-title">You can read every table and delete rows of your data</div>
                        <p>
                            Pick a table that starts with data_ from the list,
                            select rows, and choose Delete. Your server removes
                            each one with its search entries, its files and its
                            raw copy, and refuses it if a sync sends it again.
                        </p>
                        <p class="mt-2">
                            The other tables are your server's own. It reads
                            them on every request, so an edit here would break
                            what it reads.
                        </p>
                        <p class="mt-2">
                            To keep data of your own, make your own database in a
                            terminal: press ⌘K and choose <strong>Open a terminal on your server</strong>.
                        </p>
                    </div>
                </div>
            </div>
        </div>

        <!-- Expandable Query Editor -->
        <div class="query-accordion" class:expanded={queryExpanded}>
            <div class="query-inner">
                <div class="border-b border-border bg-surface p-4">
                    <textarea
                        bind:value={sql}
                        onkeydown={handleKeydown}
                        class="min-h-[80px] w-full rounded-md border border-border bg-surface-elevated p-2 font-mono text-xs text-foreground focus:border-primary"
                        placeholder="Enter SQL query..."
                    ></textarea>
                    <div class="mt-2 flex items-center justify-between">
                        <div class="text-xs text-foreground-muted">
                            Press <kbd
                                class="rounded bg-surface-elevated px-1.5 py-0.5 font-mono"
                                >⌘+Enter</kbd
                            > to run
                        </div>
                        <Button
                            variant="ghost"
                            icon="mynaui:play-solid"
                            {loading}
                            onclick={() => runQuery()}>Run</Button
                        >
                    </div>
                </div>
            </div>
        </div>

        <!-- Results Table -->
        <div class="flex-1 overflow-auto bg-surface-elevated/30">
            {#if error}
                <div
                    class="m-8 rounded-md border border-destructive/20 bg-destructive/10 p-4 text-destructive"
                >
                    <div class="flex items-center gap-2 font-medium">
                        <Icon icon="mynaui:danger-circle" />
                        Query Failed
                    </div>
                    <pre class="mt-2 whitespace-pre-wrap text-xs">{error}</pre>
                </div>
            {:else if results}
                {#if columns.length > 0}
                    <div class="inline-block min-w-full align-middle">
                        <table class="min-w-full divide-y divide-border">
                            <thead class="bg-surface sticky top-0 z-10">
                                <tr>
                                    {#if canDelete}
                                        <th scope="col" class="select-cell border-b border-r border-border bg-surface">
                                            <input
                                                type="checkbox"
                                                aria-label="Select every row shown"
                                                checked={allSelected}
                                                onchange={toggleAll}
                                            />
                                        </th>
                                    {/if}
                                    {#each columns as header}
                                        <th
                                            scope="col"
                                            class="whitespace-nowrap border-b border-r border-border bg-surface px-4 py-3 text-left text-xs font-semibold text-foreground-muted last:border-r-0"
                                        >
                                            <div
                                                class="flex items-center gap-1"
                                            >
                                                <Icon
                                                    icon="mynaui:table"
                                                    class="opacity-50"
                                                />
                                                {header}
                                            </div>
                                        </th>
                                    {/each}
                                </tr>
                            </thead>
                            {#if results.length > 0}
                                <tbody
                                    class="divide-y divide-border bg-surface"
                                >
                                    {#each results as row}
                                        <tr class="hover:bg-surface-elevated" class:row-selected={canDelete && selected.has(row.id)}>
                                            {#if canDelete}
                                                <td class="select-cell border-b border-r border-border">
                                                    <input
                                                        type="checkbox"
                                                        aria-label="Select this row"
                                                        checked={selected.has(row.id)}
                                                        onchange={() => toggleRow(row.id)}
                                                    />
                                                </td>
                                            {/if}
                                            {#each columns as header}
                                                <td
                                                    class="max-w-[200px] truncate whitespace-nowrap border-b border-r border-border px-4 py-2 font-mono text-xs text-foreground last:border-r-0"
                                                    title={row[header] ===
                                                    null
                                                        ? "NULL"
                                                        : String(row[header])}
                                                >
                                                    {row[header] === null
                                                        ? "NULL"
                                                        : String(row[header])}
                                                </td>
                                            {/each}
                                        </tr>
                                    {/each}
                                </tbody>
                            {/if}
                        </table>
                    </div>
                {/if}
                {#if results.length === 0}
                    <div
                        class="flex items-center justify-center p-8 text-foreground-muted"
                    >
                        <p>Query returned 0 records.</p>
                    </div>
                {/if}
            {:else}
                <div
                    class="flex h-full flex-col items-center justify-center text-foreground-muted opacity-50"
                >
                    <Icon icon="mynaui:database" class="text-4xl" />
                    <p class="mt-2 text-sm">Select a table to execute query</p>
                </div>
            {/if}
        </div>
    </div>
</div>

<style>
    /* Table item styling - matches sidebar nav items */
    .table-item {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        width: 100%;
        padding: 0.375rem 0.5rem;
        border-radius: var(--radius-md);
        text-align: left;
        font-size: 0.75rem;
        color: var(--color-foreground-muted);
        background: transparent;
        border: none;
        cursor: pointer;
        transition:
            background-color 150ms ease,
            color 150ms ease;
    }

    .table-item:hover {
        background: var(--hover-bg);
        color: var(--color-foreground);
    }

    .table-item.active {
        background: var(--active-bg);
        color: var(--color-foreground);
        font-weight: 500;
    }

    /* The row checkbox on a data table. */
    .select-cell {
        width: 2.25rem;
        padding: 0 0.625rem;
        text-align: center;
    }

    .select-cell input {
        accent-color: var(--color-primary);
        cursor: pointer;
    }

    .row-selected {
        background: var(--active-bg);
    }

    /* Query accordion animation */
    .query-accordion {
        display: grid;
        grid-template-rows: 0fr;
        transition: grid-template-rows 200ms ease-out;
    }

    .query-accordion.expanded {
        grid-template-rows: 1fr;
    }

    .query-inner {
        overflow: hidden;
        min-height: 0;
    }

    /* Info popover */
    .info-wrapper {
        position: relative;
    }

    .info-popover {
        position: absolute;
        top: 100%;
        right: 0;
        margin-top: 8px;
        width: 280px;
        padding: 12px 14px;
        background: var(--color-surface-elevated);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-lg);
        box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
        font-size: 12px;
        line-height: 1.5;
        color: var(--color-foreground-muted);
        opacity: 0;
        visibility: hidden;
        transform: translateY(-4px);
        transition:
            opacity 150ms ease,
            transform 150ms ease,
            visibility 150ms ease;
        z-index: var(--z-dropdown);
    }

    .info-wrapper:hover .info-popover,
    .info-wrapper:focus-within .info-popover {
        opacity: 1;
        visibility: visible;
        transform: translateY(0);
    }

    .info-title {
        font-weight: 600;
        color: var(--color-foreground);
        margin-bottom: 6px;
    }

    .info-popover p {
        margin: 0;
    }

    .info-popover strong {
        color: var(--color-foreground);
    }
</style>
