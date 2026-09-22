<!--
	Recently deleted — one list for everything deleted anywhere in the app.

	Chats, pages and projects (the box's `api::trash`, migration 0030) and
	Drive files (the older `/api/drive/trash`) wait here 30 days with a
	restore, and then the server removes them for good. Two APIs, one room:
	a second "trash" would be two lists that must agree, and the person
	deleting a page does not care which table it lived in.

	Restore needs no confirm — it is the safe direction. Delete forever and
	Empty are the two one-way doors in the app, and they get the dangerous
	dialog: red button, the consequence said in the same breath.
-->
<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { Button, IconButton, Page } from "$lib";
	import type { DriveFile, TrashItem, TrashKind } from "$lib/api/client";
	import {
		listDriveTrash,
		restoreDriveFile,
		purgeDriveFile,
		emptyDriveTrash,
		listTrash,
		restoreTrashed,
		purgeTrashed,
		emptyTrash,
	} from "$lib/api/client";
	import UniversalDataGrid, {
		type Column,
	} from "$lib/components/datagrid/UniversalDataGrid.svelte";
	import { confirmAction } from "$lib/stores/dialog.svelte";
	import { refreshAfterRestore } from "$lib/utils/toasts";
	import { toast } from "svelte-sonner";
	import { onMount } from "svelte";

	let { tab: _tab, active: _active }: { tab: Tab; active: boolean } = $props();

	const RETENTION_DAYS = 30;

	type RowKind = TrashKind | "file";

	/** One row, whichever store it came from. `id` is the grid's key. */
	interface TrashRow {
		id: string;
		kind: RowKind;
		title: string;
		/** Where it was: a Drive path, or the kind for the rest. */
		detail: string;
		deleted_at: string;
		expires_at: string;
		days_remaining: number;
		raw: TrashItem | DriveFile;
	}

	let rows = $state<TrashRow[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let busy = $state(false);

	onMount(() => {
		load();
	});

	const KIND_LABEL: Record<RowKind, string> = {
		chat: "Chat",
		page: "Page",
		project: "Project",
		file: "File",
	};

	const KIND_ICON: Record<RowKind, string> = {
		chat: "ri:chat-3-line",
		page: "ri:file-text-line",
		project: "ri:folder-3-line",
		file: "ri:file-line",
	};

	function daysLeft(expiresAt: string): number {
		const ms = new Date(expiresAt).getTime() - Date.now();
		return Math.max(0, Math.ceil(ms / (24 * 60 * 60 * 1000)));
	}

	function addDays(iso: string, days: number): string {
		return new Date(new Date(iso).getTime() + days * 24 * 60 * 60 * 1000).toISOString();
	}

	function fromRecord(item: TrashItem): TrashRow {
		return {
			id: `${item.kind}:${item.id}`,
			kind: item.kind,
			title: item.title,
			detail: KIND_LABEL[item.kind],
			deleted_at: item.deleted_at,
			expires_at: item.expires_at,
			days_remaining: daysLeft(item.expires_at),
			raw: item,
		};
	}

	function fromFile(file: DriveFile): TrashRow {
		const deleted = file.deleted_at ?? new Date().toISOString();
		const expires = addDays(deleted, RETENTION_DAYS);
		return {
			id: `file:${file.id}`,
			kind: "file",
			title: file.filename,
			detail: file.path || "Drive",
			deleted_at: deleted,
			expires_at: expires,
			days_remaining: daysLeft(expires),
			raw: file,
		};
	}

	async function load() {
		loading = true;
		error = null;
		try {
			const [records, files] = await Promise.all([listTrash(), listDriveTrash()]);
			rows = [...records.map(fromRecord), ...files.map(fromFile)].sort(
				(a, b) => new Date(b.deleted_at).getTime() - new Date(a.deleted_at).getTime(),
			);
		} catch (e) {
			error = e instanceof Error ? e.message : "Couldn't load Recently deleted";
		} finally {
			loading = false;
		}
	}

	async function restore(row: TrashRow) {
		if (busy) return;
		busy = true;
		error = null;
		try {
			if (row.kind === "file") await restoreDriveFile((row.raw as DriveFile).id);
			else await restoreTrashed(row.kind, (row.raw as TrashItem).id);
			await Promise.all([load(), refreshAfterRestore(row.kind)]);
			toast(`Restored "${row.title}"`);
		} catch (e) {
			error = e instanceof Error ? e.message : `Couldn't restore "${row.title}"`;
		} finally {
			busy = false;
		}
	}

	async function purge(row: TrashRow) {
		if (busy) return;
		const ok = await confirmAction({
			title: "Delete forever?",
			body: `"${row.title}" will be gone for good. This can't be undone.`,
			confirmLabel: "Delete forever",
			danger: true,
		});
		if (!ok) return;
		busy = true;
		error = null;
		try {
			if (row.kind === "file") await purgeDriveFile((row.raw as DriveFile).id);
			else await purgeTrashed(row.kind, (row.raw as TrashItem).id);
			await load();
			toast(`Deleted "${row.title}" forever`);
		} catch (e) {
			error = e instanceof Error ? e.message : `Couldn't delete "${row.title}"`;
		} finally {
			busy = false;
		}
	}

	async function emptyAll() {
		if (busy || rows.length === 0) return;
		const n = rows.length;
		const ok = await confirmAction({
			title: "Empty Recently deleted?",
			body: `All ${n} ${n === 1 ? "item" : "items"} will be gone for good. This can't be undone.`,
			confirmLabel: `Delete ${n} ${n === 1 ? "item" : "items"} forever`,
			danger: true,
		});
		if (!ok) return;
		busy = true;
		error = null;
		try {
			const [records, files] = await Promise.all([emptyTrash(), emptyDriveTrash()]);
			await load();
			toast(`Deleted ${records.deleted_count + files.deleted_count} items forever`);
		} catch (e) {
			error = e instanceof Error ? e.message : "Couldn't empty Recently deleted";
		} finally {
			busy = false;
		}
	}

	function formatDate(iso: string): string {
		return new Date(iso).toLocaleDateString(undefined, {
			month: "short",
			day: "numeric",
			year: "numeric",
		});
	}

	const columns: Column<TrashRow>[] = [
		{ key: "title", label: "Name", width: "40%", minWidth: "200px" },
		{ key: "detail", label: "Was", width: "18%", minWidth: "100px", hideOnMobile: true },
		{
			key: "deleted_at",
			label: "Deleted",
			width: "16%",
			minWidth: "110px",
			hideOnMobile: true,
			getValue: (row) => formatDate(row.deleted_at),
		},
		{
			key: "days_remaining",
			label: "Gone in",
			width: "14%",
			minWidth: "90px",
			getValue: (row) =>
				row.days_remaining === 0
					? "today"
					: `${row.days_remaining} ${row.days_remaining === 1 ? "day" : "days"}`,
		},
	];
</script>

<Page
	title="Recently deleted"
	description="Deleted chats, pages, projects and files stay here for 30 days. After that your server removes them for good."
	maxWidth="wide"
>
	{#snippet actions()}
		{#if rows.length > 0}
			<Button variant="danger" size="sm" icon="ri:delete-bin-line" disabled={busy} onclick={emptyAll}>
				Empty
			</Button>
		{/if}
	{/snippet}

	{#if error}
		<div class="bg-error/10 border border-error/20 rounded-lg p-4 mb-4">
			<p class="text-sm text-error">{error}</p>
		</div>
	{/if}

	<UniversalDataGrid
		items={rows}
		{columns}
		entityType="trash"
		{loading}
		rowIcon={(row) => KIND_ICON[row.kind]}
		emptyIcon="ri:delete-bin-line"
		emptyMessage="Nothing here. Anything you delete waits here for 30 days before it's gone."
		loadingMessage="Loading…"
		searchPlaceholder="Search Recently deleted"
		defaultViewMode="table"
		mobileViewMode="table"
		onRefresh={load}
	>
		{#snippet rowActions(row: TrashRow)}
			<IconButton
				icon="ri:arrow-go-back-line"
				label={`Restore ${row.title}`}
				size="sm"
				disabled={busy}
				onclick={(e) => {
					e.stopPropagation();
					restore(row);
				}}
			/>
			<IconButton
				icon="ri:delete-bin-7-line"
				label={`Delete ${row.title} forever`}
				size="sm"
				variant="danger"
				disabled={busy}
				onclick={(e) => {
					e.stopPropagation();
					purge(row);
				}}
			/>
		{/snippet}
	</UniversalDataGrid>
</Page>
