<script lang="ts">
	import { Page } from "$lib";
	import { goto } from "$app/navigation";
	import type { DriveFile, DriveUsage } from "$lib/api/client";
	import {
		listDriveFiles,
		uploadDriveFile,
		downloadDriveFile,
		deleteDriveFile,
		createDriveFolder
	} from "$lib/api/client";
	import "iconify-icon";
	import type { PageData } from "./$types";

	let { data }: { data: PageData } = $props();

	// State
	let files = $state<DriveFile[]>(data.files || []);
	let usage = $state<DriveUsage | null>(data.usage);
	let currentPath = $state(data.currentPath || "");
	let loading = $state(false);
	let error = $state<string | null>(data.error || null);

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
		if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
		return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
	}

	// Format date
	function formatDate(dateStr: string): string {
		const date = new Date(dateStr);
		return date.toLocaleDateString(undefined, {
			month: "short",
			day: "numeric",
			year: "numeric"
		});
	}

	// Get icon for file type
	function getFileIcon(file: DriveFile): string {
		if (file.is_folder) return "ri:folder-fill";

		const ext = file.filename.split(".").pop()?.toLowerCase();
		const mime = file.mime_type?.toLowerCase() || "";

		// Images
		if (mime.startsWith("image/") || ["jpg", "jpeg", "png", "gif", "webp", "svg"].includes(ext || "")) {
			return "ri:image-fill";
		}
		// Videos
		if (mime.startsWith("video/") || ["mp4", "mov", "avi", "mkv", "webm"].includes(ext || "")) {
			return "ri:movie-fill";
		}
		// Audio
		if (mime.startsWith("audio/") || ["mp3", "wav", "ogg", "m4a", "flac"].includes(ext || "")) {
			return "ri:music-fill";
		}
		// Documents
		if (["pdf"].includes(ext || "")) return "ri:file-pdf-fill";
		if (["doc", "docx"].includes(ext || "")) return "ri:file-word-fill";
		if (["xls", "xlsx"].includes(ext || "")) return "ri:file-excel-fill";
		if (["ppt", "pptx"].includes(ext || "")) return "ri:file-ppt-fill";
		// Code
		if (["js", "ts", "jsx", "tsx", "py", "rs", "go", "java", "cpp", "c", "h"].includes(ext || "")) {
			return "ri:file-code-fill";
		}
		// Text/Markdown
		if (["txt", "md", "json", "yaml", "yml", "toml", "xml", "csv"].includes(ext || "")) {
			return "ri:file-text-fill";
		}
		// Archives
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

		if (mime.startsWith("image/") || ["jpg", "jpeg", "png", "gif", "webp", "svg"].includes(ext || "")) {
			return "text-purple-500";
		}
		if (mime.startsWith("video/") || ["mp4", "mov", "avi", "mkv", "webm"].includes(ext || "")) {
			return "text-red-500";
		}
		if (mime.startsWith("audio/") || ["mp3", "wav", "ogg", "m4a", "flac"].includes(ext || "")) {
			return "text-pink-500";
		}
		if (["pdf"].includes(ext || "")) return "text-red-600";
		if (["doc", "docx"].includes(ext || "")) return "text-blue-600";
		if (["xls", "xlsx"].includes(ext || "")) return "text-green-600";

		return "text-foreground-subtle";
	}

	// Navigate to folder
	async function navigateToFolder(path: string) {
		loading = true;
		error = null;
		try {
			const newFiles = await listDriveFiles(path);
			files = newFiles;
			currentPath = path;
			await goto(`/data/drive${path ? `?path=${encodeURIComponent(path)}` : ""}`, {
				replaceState: true,
				noScroll: true
			});
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
			handleDownload(file);
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

	// Handle file upload
	async function handleUpload(fileList: FileList) {
		if (fileList.length === 0) return;

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
			const res = await fetch("/api/drive/usage");
			if (res.ok) usage = await res.json();
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

	// Delete file
	async function handleDelete() {
		if (!fileToDelete) return;

		deleting = true;
		error = null;

		try {
			await deleteDriveFile(fileToDelete.id);
			const newFiles = await listDriveFiles(currentPath);
			files = newFiles;
			// Refresh usage
			const res = await fetch("/api/drive/usage");
			if (res.ok) usage = await res.json();
			fileToDelete = null;
		} catch (e) {
			error = e instanceof Error ? e.message : "Delete failed";
		} finally {
			deleting = false;
		}
	}

	// File input ref
	let fileInput: HTMLInputElement;
</script>

<Page>
	<div class="max-w-7xl">
		<!-- Header -->
		<div class="mb-6">
			<h1 class="text-3xl font-serif font-medium text-foreground mb-2">Drive</h1>
			<p class="text-foreground-muted">Your personal file storage</p>
		</div>

		<!-- Usage Bar -->
		{#if usage}
			<div class="bg-surface border border-border rounded-lg p-4 mb-6">
				<div class="flex items-center justify-between mb-2">
					<span class="text-sm text-foreground-muted">
						{formatBytes(usage.total_bytes)} of {formatBytes(usage.quota_bytes)} used
					</span>
					<span class="text-xs text-foreground-subtle uppercase tracking-wide">
						{usage.tier} tier
					</span>
				</div>
				<div class="h-2 bg-border rounded-full overflow-hidden">
					<div
						class="h-full transition-all duration-300 rounded-full"
						class:bg-emerald-500={usage.usage_percent < 80}
						class:bg-yellow-500={usage.usage_percent >= 80 && usage.usage_percent < 90}
						class:bg-red-500={usage.usage_percent >= 90}
						style="width: {Math.min(usage.usage_percent, 100)}%"
					></div>
				</div>
				<div class="flex gap-4 mt-3 text-xs text-foreground-subtle">
					<span>{usage.file_count} files</span>
					<span>{usage.folder_count} folders</span>
				</div>
			</div>
		{/if}

		<!-- Toolbar -->
		<div class="flex items-center justify-between mb-4">
			<!-- Breadcrumbs -->
			<nav class="flex items-center gap-1 text-sm">
				{#each breadcrumbs() as crumb, i}
					{#if i > 0}
						<iconify-icon icon="ri:arrow-right-s-line" class="text-foreground-subtle"></iconify-icon>
					{/if}
					{#if i === breadcrumbs().length - 1}
						<span class="text-foreground font-medium">{crumb.name}</span>
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
					class="flex items-center gap-2 px-3 py-1.5 text-sm text-foreground-muted hover:text-foreground hover:bg-surface-elevated rounded-lg transition-colors"
					onclick={() => (showNewFolderModal = true)}
				>
					<iconify-icon icon="ri:folder-add-line"></iconify-icon>
					New folder
				</button>
				<button
					class="flex items-center gap-2 px-3 py-1.5 text-sm bg-foreground text-background hover:bg-foreground/90 rounded-lg transition-colors"
					onclick={() => fileInput?.click()}
					disabled={uploading}
				>
					<iconify-icon icon="ri:upload-2-line"></iconify-icon>
					Upload
				</button>
				<input
					bind:this={fileInput}
					type="file"
					multiple
					class="hidden"
					onchange={(e) => e.currentTarget.files && handleUpload(e.currentTarget.files)}
				/>
			</div>
		</div>

		<!-- Error Message -->
		{#if error}
			<div class="bg-red-500/10 border border-red-500/20 rounded-lg p-4 mb-4">
				<p class="text-sm text-red-600 dark:text-red-400">{error}</p>
			</div>
		{/if}

		<!-- Upload Progress -->
		{#if uploading}
			<div class="bg-blue-500/10 border border-blue-500/20 rounded-lg p-4 mb-4">
				<div class="flex items-center gap-3">
					<iconify-icon icon="ri:loader-4-line" class="animate-spin text-blue-500"></iconify-icon>
					<div class="flex-1">
						<div class="text-sm text-foreground mb-1">Uploading...</div>
						<div class="h-1.5 bg-blue-500/20 rounded-full overflow-hidden">
							<div
								class="h-full bg-blue-500 rounded-full transition-all duration-150"
								style="width: {uploadProgress}%"
							></div>
						</div>
					</div>
					<span class="text-sm text-foreground-muted">{uploadProgress}%</span>
				</div>
			</div>
		{/if}

		<!-- Drop Zone / File List -->
		<div
			class="border rounded-lg overflow-hidden transition-colors"
			class:border-border={!dragOver}
			class:border-blue-500={dragOver}
			style:background-color={dragOver ? 'rgba(59, 130, 246, 0.05)' : undefined}
			ondrop={handleDrop}
			ondragover={handleDragOver}
			ondragleave={handleDragLeave}
			role="region"
			aria-label="File drop zone"
		>
			{#if loading}
				<div class="p-12 text-center">
					<iconify-icon icon="ri:loader-4-line" class="text-4xl text-foreground-subtle animate-spin"></iconify-icon>
					<p class="text-foreground-muted mt-2">Loading...</p>
				</div>
			{:else if files.length === 0}
				<div class="p-12 text-center">
					<iconify-icon icon="ri:folder-open-line" class="text-6xl text-foreground-subtle mb-4"></iconify-icon>
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
						<iconify-icon icon="ri:upload-2-line"></iconify-icon>
						Upload files
					</button>
				</div>
			{:else}
				<table class="w-full">
					<thead class="bg-surface-elevated border-b border-border">
						<tr>
							<th class="px-4 py-3 text-left text-xs font-medium text-foreground-subtle uppercase tracking-wide">
								Name
							</th>
							<th class="px-4 py-3 text-right text-xs font-medium text-foreground-subtle uppercase tracking-wide">
								Size
							</th>
							<th class="px-4 py-3 text-right text-xs font-medium text-foreground-subtle uppercase tracking-wide">
								Modified
							</th>
							<th class="px-4 py-3 w-12"></th>
						</tr>
					</thead>
					<tbody class="divide-y divide-border">
						{#each files as file}
							<tr
								class="group hover:bg-surface-elevated transition-colors"
								role="button"
								tabindex="0"
								onclick={() => handleFileClick(file)}
								onkeydown={(e) => e.key === "Enter" && handleFileClick(file)}
							>
								<td class="px-4 py-3">
									<div class="flex items-center gap-3">
										<iconify-icon
											icon={getFileIcon(file)}
											class="text-xl {getFileIconColor(file)}"
										></iconify-icon>
										<span class="text-sm text-foreground">{file.filename}</span>
									</div>
								</td>
								<td class="px-4 py-3 text-right text-sm text-foreground-muted">
									{file.is_folder ? "—" : formatBytes(file.size_bytes)}
								</td>
								<td class="px-4 py-3 text-right text-sm text-foreground-muted">
									{formatDate(file.updated_at)}
								</td>
								<td class="px-4 py-3 text-right">
									<button
										class="opacity-0 group-hover:opacity-100 p-1 text-foreground-subtle hover:text-red-500 transition-all"
										onclick={(e) => {
											e.stopPropagation();
											fileToDelete = file;
										}}
										aria-label="Delete {file.filename}"
									>
										<iconify-icon icon="ri:delete-bin-line"></iconify-icon>
									</button>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			{/if}
		</div>
	</div>
</Page>

<!-- New Folder Modal -->
{#if showNewFolderModal}
	<div
		class="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
		onclick={() => (showNewFolderModal = false)}
		onkeydown={(e) => e.key === "Escape" && (showNewFolderModal = false)}
		role="dialog"
		aria-modal="true"
		tabindex="-1"
	>
		<div
			class="bg-surface border border-border rounded-xl shadow-xl p-6 w-full max-w-md"
			onclick={(e) => e.stopPropagation()}
			role="document"
		>
			<h2 class="text-lg font-medium text-foreground mb-4">New Folder</h2>
			<input
				type="text"
				bind:value={newFolderName}
				placeholder="Folder name"
				class="w-full px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-foreground-subtle focus:outline-none focus:ring-2 focus:ring-foreground/20"
				onkeydown={(e) => e.key === "Enter" && handleCreateFolder()}
			/>
			<div class="flex justify-end gap-3 mt-6">
				<button
					class="px-4 py-2 text-sm text-foreground-muted hover:text-foreground transition-colors"
					onclick={() => (showNewFolderModal = false)}
				>
					Cancel
				</button>
				<button
					class="px-4 py-2 text-sm bg-foreground text-background rounded-lg hover:bg-foreground/90 transition-colors disabled:opacity-50"
					onclick={handleCreateFolder}
					disabled={creatingFolder || !newFolderName.trim()}
				>
					{creatingFolder ? "Creating..." : "Create"}
				</button>
			</div>
		</div>
	</div>
{/if}

<!-- Delete Confirmation Modal -->
{#if fileToDelete}
	<div
		class="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
		onclick={() => (fileToDelete = null)}
		onkeydown={(e) => e.key === "Escape" && (fileToDelete = null)}
		role="dialog"
		aria-modal="true"
		tabindex="-1"
	>
		<div
			class="bg-surface border border-border rounded-xl shadow-xl p-6 w-full max-w-md"
			onclick={(e) => e.stopPropagation()}
			role="document"
		>
			<h2 class="text-lg font-medium text-foreground mb-2">Delete {fileToDelete.is_folder ? "Folder" : "File"}?</h2>
			<p class="text-foreground-muted mb-6">
				Are you sure you want to delete "{fileToDelete.filename}"?
				{#if fileToDelete.is_folder}
					This will delete all contents inside the folder.
				{/if}
				This action cannot be undone.
			</p>
			<div class="flex justify-end gap-3">
				<button
					class="px-4 py-2 text-sm text-foreground-muted hover:text-foreground transition-colors"
					onclick={() => (fileToDelete = null)}
				>
					Cancel
				</button>
				<button
					class="px-4 py-2 text-sm bg-red-500 text-white rounded-lg hover:bg-red-600 transition-colors disabled:opacity-50"
					onclick={handleDelete}
					disabled={deleting}
				>
					{deleting ? "Deleting..." : "Delete"}
				</button>
			</div>
		</div>
	</div>
{/if}
