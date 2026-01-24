<script lang="ts">
    import type { Tab } from "$lib/tabs/types";
    import "iconify-icon";
    import { onMount } from "svelte";

    let { tab, active }: { tab: Tab; active: boolean } = $props();

    let sql = $state("SELECT * FROM sources LIMIT 10;");
    let results: any[] | null = $state(null);
    let error: string | null = $state(null);
    let loading = $state(false);
    let tables: string[] = $state([]);
    let loadingTables = $state(true);
    let queryExpanded = $state(false);
    let selectedTable = $state<string | null>(null);

    onMount(async () => {
        try {
            const res = await fetch("/api/developer/tables");
            if (res.ok) {
                tables = await res.json();
            } else {
                console.error(
                    "Failed to load tables:",
                    res.status,
                    res.statusText,
                );
                if (res.status === 404) {
                    error =
                        "Backend endpoint not found. Please restart the implementation server.";
                } else {
                    error = `Failed to load tables: ${res.status} ${res.statusText}`;
                }
            }
        } catch (e: any) {
            console.error("Failed to load tables", e);
            error = `Network error loading tables: ${e.message}`;
        } finally {
            loadingTables = false;
        }
    });

    async function runQuery() {
        loading = true;
        error = null;
        results = null;

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

            results = data;
        } catch (e: any) {
            error = e.message;
        } finally {
            loading = false;
        }
    }

    function selectTable(tableName: string) {
        selectedTable = tableName;
        sql = `SELECT * FROM ${tableName} LIMIT 100;`;
        runQuery();
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
    <div class="w-64 flex-none border-r border-border bg-surface-elevated">
        <div class="flex h-[53px] items-center border-b border-border px-4">
            <h2 class="text-sm text-foreground-muted">Tables</h2>
        </div>
        <div class="h-[calc(100%-53px)] overflow-y-auto p-2">
            {#if loadingTables}
                <div class="p-2 text-xs text-foreground-muted">
                    Loading tables...
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
                            <iconify-icon icon="ri:table-line" class="text-xs"
                            ></iconify-icon>
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
                class="flex items-center gap-2 text-sm text-foreground-muted hover:text-foreground"
            >
                <iconify-icon
                    icon={queryExpanded
                        ? "ri:arrow-down-s-line"
                        : "ri:arrow-right-s-line"}
                    class="text-lg"
                ></iconify-icon>
                <span>Query</span>
                {#if sql && !queryExpanded}
                    <span
                        class="ml-2 max-w-[300px] truncate font-mono text-xs text-foreground-muted/60"
                    >
                        {sql}
                    </span>
                {/if}
            </button>
            <div class="flex items-center gap-2">
                <span class="text-xs text-warning">Read-only</span>
                <div class="info-wrapper">
                    <button class="info-btn" title="Why read-only?">
                        <iconify-icon icon="ri:information-line" class="text-sm"
                        ></iconify-icon>
                    </button>
                    <div class="info-popover">
                        <div class="info-title">Database is Read-Only</div>
                        <p>
                            These tables are critical for Virtues' internal data
                            processing pipeline. Any modifications could break
                            compatibility with the application.
                        </p>
                        <p class="mt-2">
                            If you need to add, modify, or transform data, we
                            recommend setting up your own SQLite database via
                            the <strong>Terminal</strong> tab.
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
                        class="min-h-[80px] w-full rounded-md border border-border bg-surface-elevated p-2 font-mono text-xs text-foreground focus:border-primary focus:outline-none"
                        placeholder="Enter SQL query..."
                    ></textarea>
                    <div class="mt-2 flex items-center justify-between">
                        <div class="text-xs text-foreground-muted">
                            Press <kbd
                                class="rounded bg-surface-elevated px-1.5 py-0.5 font-mono"
                                >⌘+Enter</kbd
                            > to run
                        </div>
                        <button
                            onclick={runQuery}
                            disabled={loading}
                            class="flex items-center gap-1.5 rounded px-2 py-1 text-xs font-medium text-foreground-muted hover:bg-surface-elevated hover:text-foreground disabled:opacity-50"
                        >
                            <iconify-icon icon="ri:play-fill"></iconify-icon>
                            {loading ? "Running..." : "Run"}
                        </button>
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
                        <iconify-icon icon="ri:error-warning-line"
                        ></iconify-icon>
                        Query Failed
                    </div>
                    <pre class="mt-2 whitespace-pre-wrap text-xs">{error}</pre>
                </div>
            {:else if results}
                {#if results.length === 0}
                    <div
                        class="flex h-full flex-col items-center justify-center text-foreground-muted"
                    >
                        <p>Query returned 0 records.</p>
                    </div>
                {:else}
                    <div class="inline-block min-w-full align-middle">
                        <table class="min-w-full divide-y divide-border">
                            <thead class="bg-surface sticky top-0 z-10">
                                <tr>
                                    {#each Object.keys(results[0]) as header}
                                        <th
                                            scope="col"
                                            class="whitespace-nowrap border-b border-r border-border bg-surface px-4 py-3 text-left text-xs font-semibold text-foreground-muted last:border-r-0"
                                        >
                                            <div
                                                class="flex items-center gap-1"
                                            >
                                                <iconify-icon
                                                    icon="ri:table-line"
                                                    class="opacity-50"
                                                ></iconify-icon>
                                                {header}
                                            </div>
                                        </th>
                                    {/each}
                                </tr>
                            </thead>
                            <tbody class="divide-y divide-border bg-surface">
                                {#each results as row}
                                    <tr class="hover:bg-surface-elevated">
                                        {#each Object.keys(results[0]) as header}
                                            <td
                                                class="max-w-[200px] truncate whitespace-nowrap border-b border-r border-border px-4 py-2 font-mono text-xs text-foreground last:border-r-0"
                                                title={row[header] === null
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
                        </table>
                    </div>
                {/if}
            {:else}
                <div
                    class="flex h-full flex-col items-center justify-center text-foreground-muted opacity-50"
                >
                    <iconify-icon icon="ri:database-2-line" class="text-4xl"
                    ></iconify-icon>
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
        border-radius: 0.375rem;
        text-align: left;
        font-size: 0.875rem;
        color: var(--color-foreground-muted);
        background: transparent;
        border: none;
        cursor: pointer;
        transition:
            background-color 150ms ease,
            color 150ms ease;
    }

    .table-item:hover {
        background: color-mix(in srgb, var(--color-foreground) 7%, transparent);
        color: var(--color-foreground);
    }

    .table-item.active {
        background: color-mix(in srgb, var(--color-foreground) 9%, transparent);
        color: var(--color-foreground);
        font-weight: 500;
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

    .info-btn {
        display: flex;
        align-items: center;
        justify-content: center;
        padding: 0.25rem;
        border: none;
        background: transparent;
        color: var(--color-foreground-muted);
        cursor: pointer;
        border-radius: 4px;
        transition:
            color 150ms ease,
            background-color 150ms ease;
    }

    .info-btn:hover {
        color: var(--color-foreground);
        background: var(--color-surface-elevated);
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
        border-radius: 8px;
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
        z-index: 50;
    }

    .info-wrapper:hover .info-popover,
    .info-btn:focus + .info-popover {
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
