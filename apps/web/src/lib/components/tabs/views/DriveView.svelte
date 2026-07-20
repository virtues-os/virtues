<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { Page } from "$lib";
	import type { DriveFile, DriveUsage } from "$lib/api/client";
	import {
		listDriveFiles,
		uploadDriveFile,
		downloadDriveFile,
		deleteDriveFile,
		createDriveFolder,
		moveDriveFile,
		getDriveUsage,
		reextractDriveFile,
	} from "$lib/api/client";
	import { formatDate } from "$lib/utils/dateUtils";
	import Icon from "$lib/components/Icon.svelte";
	import Modal from "$lib/components/Modal.svelte";
	import UniversalDataGrid, {
		type Column,
	} from "$lib/components/datagrid/UniversalDataGrid.svelte";
	import { onMount } from "svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { contextMenu } from "$lib/stores/contextMenu.svelte";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// State
	let files = $state<DriveFile[]>([]);
	let usage = $state<DriveUsage | null>(null);
	let currentPath = $state("");
	let loading = $state(true);
	let error = $state<string | null>(null);

	// Upload state
	let uploading = $state(false);
	let uploadProgress = $state(0);
	let dragOver = $state(false);

	// New folder modal
	let showNewFolderModal = $state(false);
	let newFolderName = $state("");
	let creatingFolder = $state(false);

	// Delete confirmation
	let fileToDelete = $state<DriveFile | null>(null);
	let deleting = $state(false);

	// Rename state
	let renamingFile = $state<DriveFile | null>(null);
	let renameValue = $state("");
	let renaming = $state(false);

	// Toast notification
	let toastMessage = $state<string | null>(null);
	let toastTimeout: ReturnType<typeof setTimeout> | null = null;

	// File input ref
	let fileInput = $state<HTMLInputElement | null>(null);

	onMount(async () => {
		await loadData();
	});

	function showToast(message: string) {
		if (toastTimeout) clearTimeout(toastTimeout);
		toastMessage = message;
		toastTimeout = setTimeout(() => {
			toastMessage = null;
		}, 3000);
	}

	async function loadData() {
		loading = true;
		error = null;
		try {
			const [filesData, usageData] = await Promise.all([
				listDriveFiles(currentPath),
				getDriveUsage().catch(() => null),
			]);
			files = filesData;
			if (usageData) {
				usage = usageData;
			}
		} catch (e) {
			error =
				e instanceof Error ? e.message : "Failed to load drive data";
		} finally {
			loading = false;
		}
	}

	// Breadcrumb navigation
	const breadcrumbs = $derived(() => {
		if (!currentPath) return [{ name: "Drive", path: "" }];
		const parts = currentPath.split("/");
		const crumbs = [{ name: "Drive", path: "" }];
		let pathSoFar = "";
		for (const part of parts) {
			pathSoFar = pathSoFar ? `${pathSoFar}/${part}` : part;
			crumbs.push({ name: part, path: pathSoFar });
		}
		return crumbs;
	});

	// Format bytes
	function formatBytes(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
		if (bytes < 1024 * 1024 * 1024)
			return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
		return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
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

	function getFileIconColor(_file: DriveFile): string {
		return "text-foreground-muted";
	}

	// Navigate to folder
	async function navigateToFolder(path: string) {
		loading = true;
		error = null;
		try {
			const newFiles = await listDriveFiles(path);
			files = newFiles;
			currentPath = path;
		} catch (e) {
			error = e instanceof Error ? e.message : "Failed to load folder";
		} finally {
			loading = false;
		}
	}

	// Handle file click
	function handleFileClick(file: DriveFile) {
		if (file.is_folder) {
			navigateToFolder(file.path);
		} else {
			windowShellStore.openRouteBeside(`/drive/${file.id}`, file.filename);
		}
	}

	// Download file
	async function handleDownload(file: DriveFile) {
		try {
			const { blob } = await downloadDriveFile(file.id);
			const url = URL.createObjectURL(blob);
			const a = document.createElement("a");
			a.href = url;
			a.download = file.filename;
			document.body.appendChild(a);
			a.click();
			document.body.removeChild(a);
			URL.revokeObjectURL(url);
		} catch (e) {
			error = e instanceof Error ? e.message : "Download failed";
		}
	}

	// Server router body limit is 260MB; the advertised per-file ceiling is 250MB.
	const MAX_UPLOAD_BYTES = 250 * 1024 * 1024;

	// Handle file upload
	async function handleUpload(fileList: FileList) {
		if (fileList.length === 0) return;

		// Preflight: reject oversize files before any bytes leave the browser.
		const oversize = Array.from(fileList).find((f) => f.size > MAX_UPLOAD_BYTES);
		if (oversize) {
			error = `${oversize.name} is ${formatBytes(oversize.size)} — the upload limit is 250 MB.`;
			return;
		}

		uploading = true;
		uploadProgress = 0;
		error = null;

		try {
			for (const file of fileList) {
				await uploadDriveFile(currentPath, file, (progress) => {
					uploadProgress = progress;
				});
			}
			// Refresh file list
			const newFiles = await listDriveFiles(currentPath);
			files = newFiles;
			// Refresh usage
			usage = await getDriveUsage().catch(() => usage);
		} catch (e) {
			error = e instanceof Error ? e.message : "Upload failed";
		} finally {
			uploading = false;
			uploadProgress = 0;
		}
	}

	// Handle drop
	function handleDrop(e: DragEvent) {
		e.preventDefault();
		dragOver = false;
		if (e.dataTransfer?.files) {
			handleUpload(e.dataTransfer.files);
		}
	}

	// Handle drag over
	function handleDragOver(e: DragEvent) {
		e.preventDefault();
		dragOver = true;
	}

	// Handle drag leave
	function handleDragLeave() {
		dragOver = false;
	}

	// Create new folder
	async function handleCreateFolder() {
		if (!newFolderName.trim()) return;

		creatingFolder = true;
		error = null;

		try {
			await createDriveFolder(currentPath, newFolderName.trim());
			const newFiles = await listDriveFiles(currentPath);
			files = newFiles;
			showNewFolderModal = false;
			newFolderName = "";
		} catch (e) {
			error = e instanceof Error ? e.message : "Failed to create folder";
		} finally {
			creatingFolder = false;
		}
	}

	// Delete file (soft delete - moves to trash)
	async function handleDelete() {
		if (!fileToDelete) return;

		deleting = true;
		error = null;

		try {
			await deleteDriveFile(fileToDelete.id);
			const newFiles = await listDriveFiles(currentPath);
			files = newFiles;
			// Refresh usage
			usage = await getDriveUsage().catch(() => usage);
			showToast(`"${fileToDelete.filename}" moved to Trash`);
			fileToDelete = null;
		} catch (e) {
			error = e instanceof Error ? e.message : "Delete failed";
		} finally {
			deleting = false;
		}
	}

	function navigateToTrash() {
		windowShellStore.openTabFromRoute("/trash");
	}

	// Context menu for files/folders
	function showFileContextMenu(e: MouseEvent, file: DriveFile) {
		e.preventDefault();
		e.stopPropagation();

		const items = file.is_folder
			? [
					{
						id: "open",
						label: "Open",
						icon: "ri:folder-open-line",
						action: () => navigateToFolder(file.path),
					},
					{
						id: "rename",
						label: "Rename",
						icon: "ri:pencil-line",
						dividerBefore: true,
						action: () => {
							renamingFile = file;
							renameValue = file.filename;
						},
					},
					{
						id: "delete",
						label: "Move to Trash",
						icon: "ri:delete-bin-line",
						variant: "destructive" as const,
						dividerBefore: true,
						action: () => {
							fileToDelete = file;
						},
					},
				]
			: [
					{
						id: "download",
						label: "Download",
						icon: "ri:download-line",
						action: () => handleDownload(file),
					},
					// Text-bearing files can re-queue extraction (retry a
					// failure, or pick up a newly-installed extractor).
					...(file.extraction_status !== "skipped"
						? [
								{
									id: "reextract",
									label: "Re-extract text",
									icon: "ri:refresh-line",
									action: () => handleReextract(file),
								},
							]
						: []),
					{
						id: "rename",
						label: "Rename",
						icon: "ri:pencil-line",
						dividerBefore: true,
						action: () => {
							renamingFile = file;
							renameValue = file.filename;
						},
					},
					{
						id: "delete",
						label: "Move to Trash",
						icon: "ri:delete-bin-line",
						variant: "destructive" as const,
						dividerBefore: true,
						action: () => {
							fileToDelete = file;
						},
					},
				];

		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}

	// Inline rename
	async function handleRename() {
		if (
			!renamingFile ||
			!renameValue.trim() ||
			renameValue.trim() === renamingFile.filename
		) {
			cancelRename();
			return;
		}

		renaming = true;
		error = null;

		try {
			const newPath = currentPath
				? `${currentPath}/${renameValue.trim()}`
				: renameValue.trim();
			await moveDriveFile(renamingFile.id, newPath);
			const newFiles = await listDriveFiles(currentPath);
			files = newFiles;
			showToast(`Renamed to "${renameValue.trim()}"`);
			renamingFile = null;
			renameValue = "";
		} catch (e) {
			error = e instanceof Error ? e.message : "Rename failed";
		} finally {
			renaming = false;
		}
	}

	function cancelRename() {
		renamingFile = null;
		renameValue = "";
	}

	// Column definitions
	const columns: Column<DriveFile>[] = [
		{
			key: "filename",
			label: "Name",
			icon: "ri:file-line",
			width: "55%",
			minWidth: "240px",
		},
		{
			key: "size_bytes",
			label: "Size",
			icon: "ri:hard-drive-2-line",
			width: "15%",
			minWidth: "90px",
			getValue: (file) =>
				file.is_folder ? null : formatBytes(file.size_bytes),
		},
		{
			key: "extraction_status",
			label: "Indexed",
			icon: "ri:search-eye-line",
			width: "12%",
			minWidth: "90px",
			hideOnMobile: true,
			// Honest per-file extraction state (researcher-plan D1): text-bearing
			// files show where they are in the corpus pipeline; others show —.
			getValue: (file) => extractionLabel(file),
		},
		{
			key: "updated_at",
			label: "Modified",
			icon: "ri:time-line",
			width: "20%",
			minWidth: "120px",
			hideOnMobile: true,
			getValue: (file) => formatDate(file.updated_at),
		},
		{
			key: "id",
			label: "",
			width: "48px",
			sortable: false,
		},
	];

	function extractionLabel(file: DriveFile): string | null {
		if (file.is_folder) return null;
		switch (file.extraction_status) {
			case "done":
				return "indexed";
			case "pending":
				return "queued";
			case "extracting":
				return "extracting…";
			case "no_text":
				return "no text layer";
			case "failed":
				return "failed";
			default:
				return "—";
		}
	}

	async function handleReextract(file: DriveFile) {
		try {
			await reextractDriveFile(file.id);
			files = await listDriveFiles(currentPath);
		} catch (e) {
			error = e instanceof Error ? e.message : "Failed to queue extraction";
		}
	}

	function handleItemClick(file: DriveFile) {
		if (renamingFile) return;
		handleFileClick(file);
	}

	// Svelte action to auto-focus an input element
	function autofocus(node: HTMLInputElement) {
		node.focus();
		// Select filename without extension for files
		const dotIndex = node.value.lastIndexOf(".");
		if (dotIndex > 0) {
			node.setSelectionRange(0, dotIndex);
		} else {
			node.select();
		}
	}
</script>

<Page title="Drive" description="Your personal file storage" maxWidth="wide">

		<!-- Usage Bar -->
		{#if usage}
			{@const drivePercent =
				usage.quota_bytes > 0
					? (usage.drive_bytes / usage.quota_bytes) * 100
					: 0}
			{@const dataLakePercent =
				usage.quota_bytes > 0
					? (usage.data_lake_bytes / usage.quota_bytes) * 100
					: 0}
			<div class="bg-surface border border-border rounded-lg p-4 mb-6">
				<div class="flex items-center justify-between mb-2">
					<span class="text-sm text-foreground-muted">
						{formatBytes(usage.total_bytes)} of {formatBytes(
							usage.quota_bytes,
						)} used
					</span>
				</div>
				<!-- Segmented progress bar -->
				<div class="h-3 bg-border rounded-full overflow-hidden flex">
					{#if drivePercent > 0}
						<div
							class="h-full bg-primary transition-all duration-300"
							style="width: {Math.min(
								drivePercent,
								100 - dataLakePercent,
							)}%"
						></div>
					{/if}
					{#if dataLakePercent > 0}
						<div
							class="h-full bg-secondary transition-all duration-300"
							style="width: {Math.min(
								dataLakePercent,
								100 - drivePercent,
							)}%"
						></div>
					{/if}
				</div>
				<!-- Legend -->
				<div
					class="flex flex-wrap gap-4 mt-3 text-xs text-foreground-muted"
				>
					<span class="flex items-center gap-1.5">
						<span class="w-2.5 h-2.5 bg-primary rounded-sm"></span>
						Drive ({formatBytes(usage.drive_bytes)})
					</span>
					<a
						href="/developers/lake"
						class="flex items-center gap-1.5 hover:text-foreground transition-colors"
					>
						<span class="w-2.5 h-2.5 bg-secondary rounded-sm"
						></span>
						Lake ({formatBytes(usage.data_lake_bytes)})
					</a>
					<span class="flex items-center gap-1.5">
						<span class="w-2.5 h-2.5 bg-border rounded-sm"></span>
						<!-- Real free space on the box's disk — other data
						     (OS, Postgres, models) lives there too, so this is
						     NOT quota minus drive usage. -->
						Available ({formatBytes(usage.available_bytes)})
					</span>
				</div>
			</div>
		{/if}

		<!-- Toolbar -->
		<div class="flex items-center justify-between mb-4">
			<!-- Breadcrumbs -->
			<nav class="flex items-center gap-1 text-sm">
				{#each breadcrumbs() as crumb, i}
					{#if i > 0}
						<Icon
							icon="ri:arrow-right-s-line"
							class="text-foreground-subtle"
						/>
					{/if}
					{#if i === breadcrumbs().length - 1}
						<span class="text-foreground font-medium"
							>{crumb.name}</span
						>
					{:else}
						<button
							class="text-foreground-muted hover:text-foreground transition-colors"
							onclick={() => navigateToFolder(crumb.path)}
						>
							{crumb.name}
						</button>
					{/if}
				{/each}
			</nav>

			<!-- Actions -->
			<div class="flex items-center gap-2">
				<button
					class="flex items-center gap-1.5 px-3 py-1.5 text-sm text-foreground-muted hover:text-foreground hover:bg-surface-elevated rounded-lg transition-colors"
					onclick={navigateToTrash}
				>
					<Icon icon="ri:delete-bin-line" />
					Trash
				</button>
				<button
					class="flex items-center gap-2 px-3 py-1.5 text-sm text-foreground-muted hover:text-foreground hover:bg-surface-elevated rounded-lg transition-colors"
					onclick={() => (showNewFolderModal = true)}
				>
					<Icon icon="ri:folder-add-line" />
					New folder
				</button>
				<button
					class="flex items-center gap-2 px-3 py-1.5 text-sm bg-foreground text-background hover:bg-foreground/90 rounded-lg transition-colors"
					onclick={() => fileInput?.click()}
					disabled={uploading}
				>
					<Icon icon="ri:upload-2-line" />
					Upload
				</button>
				<input
					bind:this={fileInput}
					type="file"
					multiple
					class="hidden"
					aria-hidden="true"
					onchange={(e) =>
						e.currentTarget.files &&
						handleUpload(e.currentTarget.files)}
				/>
			</div>
		</div>

		<!-- Error Message -->
		{#if error}
			<div
				class="bg-error/10 border border-error/20 rounded-lg p-4 mb-4"
			>
				<p class="text-sm text-error">{error}</p>
			</div>
		{/if}

		<!-- Upload Progress -->
		{#if uploading}
			<div
				class="bg-primary/10 border border-primary/20 rounded-lg p-4 mb-4"
			>
				<div class="flex items-center gap-3">
					<Icon
						icon="ri:loader-4-line"
						class="animate-spin text-primary"
					/>
					<div class="flex-1">
						<div class="text-sm text-foreground mb-1">
							Uploading...
						</div>
						<div
							class="h-1.5 bg-primary/20 rounded-full overflow-hidden"
						>
							<div
								class="h-full bg-primary rounded-full transition-all duration-150"
								style="width: {uploadProgress}%"
							></div>
						</div>
					</div>
					<span class="text-sm text-foreground-muted"
						>{uploadProgress}%</span
					>
				</div>
			</div>
		{/if}

		<!-- Files Grid (drop zone) -->
		<div
			class="border rounded-lg transition-colors"
			class:border-transparent={!dragOver}
			class:border-primary={dragOver}
			style:background-color={dragOver
				? "color-mix(in srgb, var(--color-primary) 5%, transparent)"
				: undefined}
			ondrop={handleDrop}
			ondragover={handleDragOver}
			ondragleave={handleDragLeave}
			role="region"
			aria-label="File drop zone"
		>
			{#if !loading && files.length === 0}
				<div class="p-12 text-center">
					<Icon
						icon="ri:folder-open-line"
						class="text-6xl text-foreground-subtle mb-4"
					/>
					<h3 class="text-lg font-medium text-foreground mb-2">
						{currentPath ? "This folder is empty" : "No files yet"}
					</h3>
					<p class="text-foreground-muted mb-4">
						Drag and drop files here or click Upload to get started
					</p>
					<button
						class="inline-flex items-center gap-2 px-4 py-2 bg-foreground text-background rounded-lg hover:bg-foreground/90 transition-colors"
						onclick={() => fileInput?.click()}
					>
						<Icon icon="ri:upload-2-line" />
						Upload files
					</button>
				</div>
			{:else}
				<UniversalDataGrid
					items={files}
					{columns}
					entityType="drive"
					{loading}
					emptyIcon="ri:folder-open-line"
					emptyMessage={currentPath
						? "This folder is empty"
						: "No files yet"}
					loadingMessage="Loading files..."
					searchPlaceholder="Search files..."
					onItemClick={handleItemClick}
					onItemContextMenu={(file, e) =>
						showFileContextMenu(e, file)}
					onRefresh={loadData}
				>
					{#snippet tableRow(file: DriveFile)}
						<td class="px-3 py-2.5">
							<div class="flex items-center gap-3">
								<Icon
									icon={getFileIcon(file)}
									class="text-xl {getFileIconColor(file)}"
								/>
								{#if renamingFile?.id === file.id}
									<input
										type="text"
										bind:value={renameValue}
										use:autofocus
										class="text-sm text-foreground bg-transparent border border-border rounded px-1.5 py-0.5 outline-none focus:border-primary w-full max-w-xs"
										onclick={(e) => e.stopPropagation()}
										onkeydown={(e) => {
											e.stopPropagation();
											if (e.key === "Enter")
												handleRename();
											if (e.key === "Escape")
												cancelRename();
										}}
										onblur={cancelRename}
										disabled={renaming}
									/>
								{:else}
									<span class="text-sm text-foreground"
										>{file.filename}</span
									>
								{/if}
							</div>
						</td>
						<td class="px-3 py-2.5 text-sm text-foreground-muted">
							{file.is_folder
								? "—"
								: formatBytes(file.size_bytes)}
						</td>
						<td
							class="px-3 py-2.5 text-sm text-foreground-subtle hide-mobile"
						>
							{#if extractionLabel(file) === "failed"}
								<button
									class="text-danger underline decoration-dotted"
									onclick={(e) => {
										e.stopPropagation();
										handleReextract(file);
									}}
									title="Extraction failed — click to retry"
								>
									failed — retry
								</button>
							{:else}
								{extractionLabel(file) ?? ""}
							{/if}
						</td>
						<td
							class="px-3 py-2.5 text-sm text-foreground-muted hide-mobile"
						>
							{formatDate(file.updated_at)}
						</td>
						<td class="px-3 py-2.5 text-right">
							<button
								class="p-1 text-foreground-subtle hover:text-foreground transition-colors"
								onclick={(e) => {
									e.stopPropagation();
									showFileContextMenu(e, file);
								}}
								aria-label="Actions for {file.filename}"
							>
								<Icon icon="ri:more-2-fill" />
							</button>
						</td>
					{/snippet}

					{#snippet card(file: DriveFile)}
						<div
							class="flex flex-col items-center gap-2 text-center"
						>
							<Icon
								icon={getFileIcon(file)}
								class="text-3xl {getFileIconColor(file)}"
							/>
							<span
								class="text-sm font-medium text-foreground break-all"
								>{file.filename}</span
							>
							<span class="text-xs text-foreground-muted">
								{file.is_folder
									? "Folder"
									: formatBytes(file.size_bytes)}
							</span>
						</div>
					{/snippet}
				</UniversalDataGrid>
			{/if}
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

<!-- New Folder Modal -->
<Modal
	open={showNewFolderModal}
	onClose={() => {
		showNewFolderModal = false;
		newFolderName = "";
	}}
	title="New Folder"
	width="sm"
>
	<input
		type="text"
		bind:value={newFolderName}
		placeholder="Folder name"
		class="modal-input"
		onkeydown={(e) => e.key === "Enter" && handleCreateFolder()}
	/>
	{#snippet footer()}
		<button
			class="modal-btn modal-btn-secondary"
			onclick={() => {
				showNewFolderModal = false;
				newFolderName = "";
			}}
		>
			Cancel
		</button>
		<button
			class="modal-btn modal-btn-primary"
			onclick={handleCreateFolder}
			disabled={creatingFolder || !newFolderName.trim()}
		>
			{creatingFolder ? "Creating..." : "Create"}
		</button>
	{/snippet}
</Modal>

<!-- Delete Confirmation Modal (Soft Delete) -->
<Modal
	open={!!fileToDelete}
	onClose={() => (fileToDelete = null)}
	title="Move to Trash?"
	width="sm"
>
	{#if fileToDelete}
		<p class="text-foreground-muted">
			"{fileToDelete.filename}" will be moved to Trash.
			{#if fileToDelete.is_folder}
				This includes all contents inside the folder.
			{/if}
			You can restore it within 30 days.
		</p>
	{/if}
	{#snippet footer()}
		<button
			class="modal-btn modal-btn-secondary"
			onclick={() => (fileToDelete = null)}
		>
			Cancel
		</button>
		<button
			class="modal-btn bg-error text-white hover:bg-error disabled:opacity-50"
			onclick={handleDelete}
			disabled={deleting}
		>
			{deleting ? "Moving..." : "Move to Trash"}
		</button>
	{/snippet}
</Modal>

<style>
	@media (max-width: 768px) {
		.hide-mobile {
			display: none;
		}
	}
</style>
