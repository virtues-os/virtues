<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { Page } from "$lib";
	import type { DriveFile } from "$lib/api/client";
	import {
		listDriveTrash,
		restoreDriveFile,
		purgeDriveFile,
		emptyDriveTrash,
	} from "$lib/api/client";
	import Icon from "$lib/components/Icon.svelte";
	import Modal from "$lib/components/Modal.svelte";
	import { onMount } from "svelte";
	import { spaceStore } from "$lib/stores/space.svelte";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// State
	let trashFiles = $state<DriveFile[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	// Modal state
	let fileToRestore = $state<DriveFile | null>(null);
	let restoring = $state(false);
	let fileToPurge = $state<DriveFile | null>(null);
	let purging = $state(false);
	let showEmptyTrashModal = $state(false);
	let emptyingTrash = $state(false);

	// Toast notification
	let toastMessage = $state<string | null>(null);
	let toastTimeout: ReturnType<typeof setTimeout> | null = null;

	onMount(async () => {
		await loadTrash();
	});

	function showToast(message: string) {
		if (toastTimeout) clearTimeout(toastTimeout);
		toastMessage = message;
		toastTimeout = setTimeout(() => {
			toastMessage = null;
		}, 3000);
	}

	async function loadTrash() {
		loading = true;
		error = null;
		try {
			trashFiles = await listDriveTrash();
		} catch (e) {
			error = e instanceof Error ? e.message : "Failed to load trash";
		} finally {
			loading = false;
		}
	}

	// Calculate days remaining until permanent deletion
	function getDaysRemaining(deletedAt: string): number {
		const deleted = new Date(deletedAt);
		const now = new Date();
		const thirtyDaysAfter = new Date(
			deleted.getTime() + 30 * 24 * 60 * 60 * 1000,
		);
		const remaining = thirtyDaysAfter.getTime() - now.getTime();
		return Math.max(0, Math.ceil(remaining / (24 * 60 * 60 * 1000)));
	}

	// Format bytes
	function formatBytes(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
		if (bytes < 1024 * 1024 * 1024)
			return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
		return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
	}

	// Format date
	function formatDate(dateStr: string): string {
		const date = new Date(dateStr);
		return date.toLocaleDateString(undefined, {
			month: "short",
			day: "numeric",
			year: "numeric",
		});
	}

	// Get icon for file type
	function getFileIcon(file: DriveFile): string {
		if (file.is_folder) return "ri:folder-fill";

		const ext = file.filename.split(".").pop()?.toLowerCase();
		const mime = file.mime_type?.toLowerCase() || "";

		if (
			mime.startsWith("image/") ||
			["jpg", "jpeg", "png", "gif", "webp", "svg"].includes(ext || "")
		) {
			return "ri:image-fill";
		}
		if (
			mime.startsWith("video/") ||
			["mp4", "mov", "avi", "mkv", "webm"].includes(ext || "")
		) {
			return "ri:movie-fill";
		}
		if (
			mime.startsWith("audio/") ||
			["mp3", "wav", "ogg", "m4a", "flac"].includes(ext || "")
		) {
			return "ri:music-fill";
		}
		if (["pdf"].includes(ext || "")) return "ri:file-pdf-fill";
		if (["doc", "docx"].includes(ext || "")) return "ri:file-word-fill";
		if (["xls", "xlsx"].includes(ext || "")) return "ri:file-excel-fill";
		if (["ppt", "pptx"].includes(ext || "")) return "ri:file-ppt-fill";
		if (
			[
				"js",
				"ts",
				"jsx",
				"tsx",
				"py",
				"rs",
				"go",
				"java",
				"cpp",
				"c",
				"h",
			].includes(ext || "")
		) {
			return "ri:file-code-fill";
		}
		if (
			["txt", "md", "json", "yaml", "yml", "toml", "xml", "csv"].includes(
				ext || "",
			)
		) {
			return "ri:file-text-fill";
		}
		if (["zip", "tar", "gz", "rar", "7z"].includes(ext || "")) {
			return "ri:file-zip-fill";
		}

		return "ri:file-fill";
	}

	// Get icon color for file type
	function getFileIconColor(file: DriveFile): string {
		if (file.is_folder) return "text-yellow-500";

		const ext = file.filename.split(".").pop()?.toLowerCase();
		const mime = file.mime_type?.toLowerCase() || "";

		if (
			mime.startsWith("image/") ||
			["jpg", "jpeg", "png", "gif", "webp", "svg"].includes(ext || "")
		) {
			return "text-purple-500";
		}
		if (
			mime.startsWith("video/") ||
			["mp4", "mov", "avi", "mkv", "webm"].includes(ext || "")
		) {
			return "text-red-500";
		}
		if (
			mime.startsWith("audio/") ||
			["mp3", "wav", "ogg", "m4a", "flac"].includes(ext || "")
		) {
			return "text-pink-500";
		}
		if (["pdf"].includes(ext || "")) return "text-red-600";
		if (["doc", "docx"].includes(ext || "")) return "text-blue-600";
		if (["xls", "xlsx"].includes(ext || "")) return "text-green-600";

		return "text-foreground-subtle";
	}

	// Restore file from trash
	async function handleRestore() {
		if (!fileToRestore) return;

		restoring = true;
		error = null;

		try {
			await restoreDriveFile(fileToRestore.id);
			trashFiles = await listDriveTrash();
			showToast(`"${fileToRestore.filename}" restored`);
			fileToRestore = null;
		} catch (e) {
			error = e instanceof Error ? e.message : "Restore failed";
		} finally {
			restoring = false;
		}
	}

	// Permanently delete file
	async function handlePurge() {
		if (!fileToPurge) return;

		purging = true;
		error = null;

		try {
			await purgeDriveFile(fileToPurge.id);
			trashFiles = await listDriveTrash();
			showToast(`"${fileToPurge.filename}" permanently deleted`);
			fileToPurge = null;
		} catch (e) {
			error = e instanceof Error ? e.message : "Permanent delete failed";
		} finally {
			purging = false;
		}
	}

	// Empty entire trash
	async function handleEmptyTrash() {
		emptyingTrash = true;
		error = null;

		try {
			const result = await emptyDriveTrash();
			trashFiles = [];
			showToast(`${result.deleted_count} items permanently deleted`);
			showEmptyTrashModal = false;
		} catch (e) {
			error = e instanceof Error ? e.message : "Failed to empty trash";
		} finally {
			emptyingTrash = false;
		}
	}

	function navigateToDrive() {
		spaceStore.openTabFromRoute("/drive");
	}
</script>

<Page>
	<div class="max-w-7xl">
		<!-- Header -->
		<div class="mb-6">
			<div class="flex items-center gap-2 mb-2">
				<button
					class="text-foreground-muted hover:text-foreground transition-colors"
					onclick={navigateToDrive}
				>
					<Icon icon="ri:arrow-left-line" class="text-lg" />
				</button>
				<h1 class="text-3xl font-serif font-medium text-foreground">
					Trash
				</h1>
			</div>
			<p class="text-foreground-muted">
				Items in Trash are automatically deleted after 30 days
			</p>
		</div>

		<!-- Toolbar -->
		<div class="flex items-center justify-between mb-4">
			<span class="text-sm text-foreground-muted">
				{trashFiles.length}
				{trashFiles.length === 1 ? "item" : "items"}
			</span>
			{#if trashFiles.length > 0}
				<button
					class="flex items-center gap-2 px-3 py-1.5 text-sm text-red-500 hover:bg-red-500/10 rounded-lg transition-colors"
					onclick={() => (showEmptyTrashModal = true)}
				>
					<Icon icon="ri:delete-bin-line" />
					Empty Trash
				</button>
			{/if}
		</div>

		<!-- Error Message -->
		{#if error}
			<div
				class="bg-red-500/10 border border-red-500/20 rounded-lg p-4 mb-4"
			>
				<p class="text-sm text-red-600 dark:text-red-400">{error}</p>
			</div>
		{/if}

		<!-- Trash Table -->
		<div class="border border-border rounded-lg overflow-hidden">
			{#if loading}
				<div class="flex items-center justify-center p-12">
					<Icon icon="ri:loader-4-line" width="20" class="spin" />
				</div>
			{:else if trashFiles.length === 0}
				<div class="p-12 text-center">
					<Icon
						icon="ri:delete-bin-line"
						class="text-6xl text-foreground-subtle mb-4"
					/>
					<h3 class="text-lg font-medium text-foreground mb-2">
						Trash is empty
					</h3>
					<p class="text-foreground-muted">
						Deleted files will appear here for 30 days before being
						permanently removed
					</p>
				</div>
			{:else}
				<table class="w-full">
					<thead class="bg-surface-elevated border-b border-border">
						<tr>
							<th
								class="px-4 py-3 text-left text-xs font-medium text-foreground-subtle uppercase tracking-wide"
							>
								Name
							</th>
							<th
								class="px-4 py-3 text-right text-xs font-medium text-foreground-subtle uppercase tracking-wide"
							>
								Size
							</th>
							<th
								class="px-4 py-3 text-right text-xs font-medium text-foreground-subtle uppercase tracking-wide"
							>
								Deleted
							</th>
							<th
								class="px-4 py-3 text-right text-xs font-medium text-foreground-subtle uppercase tracking-wide"
							>
								Expires
							</th>
							<th class="px-4 py-3 w-24"></th>
						</tr>
					</thead>
					<tbody class="divide-y divide-border">
						{#each trashFiles as file}
							{@const daysRemaining = file.deleted_at
								? getDaysRemaining(file.deleted_at)
								: 30}
							{@const isWarning = daysRemaining <= 7}
							{@const isCritical = daysRemaining <= 3}
							<tr
								class="group hover:bg-surface-elevated transition-colors"
							>
								<td class="px-4 py-3">
									<div class="flex items-center gap-3">
										<Icon
											icon={getFileIcon(file)}
											class="text-xl {getFileIconColor(
												file,
											)} opacity-50"
										/>
										<div>
											<span
												class="text-sm text-foreground"
												>{file.filename}</span
											>
											<p
												class="text-xs text-foreground-subtle"
											>
												{file.path}
											</p>
										</div>
									</div>
								</td>
								<td
									class="px-4 py-3 text-right text-sm text-foreground-muted"
								>
									{file.is_folder
										? "-"
										: formatBytes(file.size_bytes)}
								</td>
								<td
									class="px-4 py-3 text-right text-sm text-foreground-muted"
								>
									{file.deleted_at
										? formatDate(file.deleted_at)
										: "-"}
								</td>
								<td class="px-4 py-3 text-right">
									<span
										class="text-xs px-2 py-1 rounded-full {isCritical
											? 'bg-red-500/10 text-red-600 dark:text-red-400'
											: isWarning
												? 'bg-yellow-500/10 text-yellow-600 dark:text-yellow-400'
												: 'text-foreground-muted'}"
									>
										{daysRemaining}
										{daysRemaining === 1 ? "day" : "days"}
									</span>
								</td>
								<td class="px-4 py-3 text-right">
									<div
										class="flex items-center justify-end gap-1 opacity-0 group-hover:opacity-100 transition-all"
									>
										<button
											class="p-1 text-foreground-subtle hover:text-emerald-500 transition-colors"
											onclick={() =>
												(fileToRestore = file)}
											aria-label="Restore {file.filename}"
											title="Restore"
										>
											<Icon
												icon="ri:arrow-go-back-line"
											/>
										</button>
										<button
											class="p-1 text-foreground-subtle hover:text-red-500 transition-colors"
											onclick={() => (fileToPurge = file)}
											aria-label="Delete forever {file.filename}"
											title="Delete forever"
										>
											<Icon icon="ri:delete-bin-7-line" />
										</button>
									</div>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			{/if}
		</div>
	</div>
</Page>

<!-- Toast Notification -->
{#if toastMessage}
	<div
		class="fixed bottom-4 left-1/2 -translate-x-1/2 z-50 animate-in fade-in slide-in-from-bottom-2 duration-200"
	>
		<div
			class="bg-foreground text-background px-4 py-2 rounded-lg shadow-lg text-sm"
		>
			{toastMessage}
		</div>
	</div>
{/if}

<!-- Restore Confirmation Modal -->
<Modal
	open={!!fileToRestore}
	onClose={() => (fileToRestore = null)}
	title="Restore {fileToRestore?.is_folder ? 'Folder' : 'File'}?"
	width="sm"
>
	<p class="text-foreground-muted">
		"{fileToRestore?.filename}" will be restored to its original location.
		{#if fileToRestore?.is_folder}
			This includes all contents inside the folder.
		{/if}
	</p>
	{#snippet footer()}
		<button
			class="modal-btn modal-btn-secondary"
			onclick={() => (fileToRestore = null)}
		>
			Cancel
		</button>
		<button
			class="modal-btn bg-emerald-500 text-white hover:bg-emerald-600 disabled:opacity-50"
			onclick={handleRestore}
			disabled={restoring}
		>
			{restoring ? "Restoring..." : "Restore"}
		</button>
	{/snippet}
</Modal>

<!-- Purge Confirmation Modal -->
<Modal
	open={!!fileToPurge}
	onClose={() => (fileToPurge = null)}
	title="Delete Forever?"
	width="sm"
>
	<p class="text-foreground-muted">
		"{fileToPurge?.filename}" will be permanently deleted.
		{#if fileToPurge?.is_folder}
			This includes all contents inside the folder.
		{/if}
	</p>
	<p class="text-red-500 font-medium mt-2">This action cannot be undone.</p>
	{#snippet footer()}
		<button
			class="modal-btn modal-btn-secondary"
			onclick={() => (fileToPurge = null)}
		>
			Cancel
		</button>
		<button
			class="modal-btn bg-red-500 text-white hover:bg-red-600 disabled:opacity-50"
			onclick={handlePurge}
			disabled={purging}
		>
			{purging ? "Deleting..." : "Delete Forever"}
		</button>
	{/snippet}
</Modal>

<!-- Empty Trash Confirmation Modal -->
<Modal
	open={showEmptyTrashModal}
	onClose={() => (showEmptyTrashModal = false)}
	title="Empty Trash?"
	width="sm"
>
	<p class="text-foreground-muted">
		All {trashFiles.length}
		{trashFiles.length === 1 ? "item" : "items"} in Trash will be permanently
		deleted.
	</p>
	<p class="text-red-500 font-medium mt-2">This action cannot be undone.</p>
	{#snippet footer()}
		<button
			class="modal-btn modal-btn-secondary"
			onclick={() => (showEmptyTrashModal = false)}
		>
			Cancel
		</button>
		<button
			class="modal-btn bg-red-500 text-white hover:bg-red-600 disabled:opacity-50"
			onclick={handleEmptyTrash}
			disabled={emptyingTrash}
		>
			{emptyingTrash ? "Emptying..." : "Empty Trash"}
		</button>
	{/snippet}
</Modal>
