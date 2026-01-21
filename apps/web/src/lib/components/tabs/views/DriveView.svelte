<script lang="ts">
	import type { Tab } from '$lib/stores/windowTabs.svelte';
	import { Page } from '$lib';
	import type { DriveFile, DriveUsage } from '$lib/api/client';
	import {
		listDriveFiles,
		uploadDriveFile,
		downloadDriveFile,
		deleteDriveFile,
		createDriveFolder,
		listDriveTrash,
		restoreDriveFile,
		purgeDriveFile,
		emptyDriveTrash
	} from '$lib/api/client';
	import 'iconify-icon';
	import { onMount } from 'svelte';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// View mode
	type ViewMode = 'files' | 'trash';
	let viewMode = $state<ViewMode>('files');

	// State
	let files = $state<DriveFile[]>([]);
	let trashFiles = $state<DriveFile[]>([]);
	let usage = $state<DriveUsage | null>(null);
	let currentPath = $state('');
	let loading = $state(true);
	let error = $state<string | null>(null);

	// Upload state
	let uploading = $state(false);
	let uploadProgress = $state(0);
	let dragOver = $state(false);

	// New folder modal
	let showNewFolderModal = $state(false);
	let newFolderName = $state('');
	let creatingFolder = $state(false);

	// Delete confirmation
	let fileToDelete = $state<DriveFile | null>(null);
	let deleting = $state(false);

	// Trash actions
	let fileToRestore = $state<DriveFile | null>(null);
	let restoring = $state(false);
	let fileToPurge = $state<DriveFile | null>(null);
	let purging = $state(false);
	let emptyingTrash = $state(false);
	let showEmptyTrashModal = $state(false);

	// Toast notification
	let toastMessage = $state<string | null>(null);
	let toastTimeout: ReturnType<typeof setTimeout> | null = null;

	// File input ref
	let fileInput: HTMLInputElement;

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
			if (viewMode === 'files') {
				const [filesData, usageResponse] = await Promise.all([
					listDriveFiles(currentPath),
					fetch('/api/drive/usage')
				]);
				files = filesData;
				if (usageResponse.ok) {
					usage = await usageResponse.json();
				}
			} else {
				const [trashData, usageResponse] = await Promise.all([
					listDriveTrash(),
					fetch('/api/drive/usage')
				]);
				trashFiles = trashData;
				if (usageResponse.ok) {
					usage = await usageResponse.json();
				}
			}
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load drive data';
		} finally {
			loading = false;
		}
	}

	// Switch view mode
	async function switchViewMode(mode: ViewMode) {
		if (mode === viewMode) return;
		viewMode = mode;
		currentPath = '';
		await loadData();
	}

	// Calculate days remaining until permanent deletion
	function getDaysRemaining(deletedAt: string): number {
		const deleted = new Date(deletedAt);
		const now = new Date();
		const thirtyDaysAfter = new Date(deleted.getTime() + 30 * 24 * 60 * 60 * 1000);
		const remaining = thirtyDaysAfter.getTime() - now.getTime();
		return Math.max(0, Math.ceil(remaining / (24 * 60 * 60 * 1000)));
	}

	// Breadcrumb navigation
	const breadcrumbs = $derived(() => {
		if (!currentPath) return [{ name: 'Drive', path: '' }];
		const parts = currentPath.split('/');
		const crumbs = [{ name: 'Drive', path: '' }];
		let pathSoFar = '';
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
			month: 'short',
			day: 'numeric',
			year: 'numeric'
		});
	}

	// Get icon for file type
	function getFileIcon(file: DriveFile): string {
		if (file.is_folder) return 'ri:folder-fill';

		const ext = file.filename.split('.').pop()?.toLowerCase();
		const mime = file.mime_type?.toLowerCase() || '';

		if (mime.startsWith('image/') || ['jpg', 'jpeg', 'png', 'gif', 'webp', 'svg'].includes(ext || '')) {
			return 'ri:image-fill';
		}
		if (mime.startsWith('video/') || ['mp4', 'mov', 'avi', 'mkv', 'webm'].includes(ext || '')) {
			return 'ri:movie-fill';
		}
		if (mime.startsWith('audio/') || ['mp3', 'wav', 'ogg', 'm4a', 'flac'].includes(ext || '')) {
			return 'ri:music-fill';
		}
		if (['pdf'].includes(ext || '')) return 'ri:file-pdf-fill';
		if (['doc', 'docx'].includes(ext || '')) return 'ri:file-word-fill';
		if (['xls', 'xlsx'].includes(ext || '')) return 'ri:file-excel-fill';
		if (['ppt', 'pptx'].includes(ext || '')) return 'ri:file-ppt-fill';
		if (['js', 'ts', 'jsx', 'tsx', 'py', 'rs', 'go', 'java', 'cpp', 'c', 'h'].includes(ext || '')) {
			return 'ri:file-code-fill';
		}
		if (['txt', 'md', 'json', 'yaml', 'yml', 'toml', 'xml', 'csv'].includes(ext || '')) {
			return 'ri:file-text-fill';
		}
		if (['zip', 'tar', 'gz', 'rar', '7z'].includes(ext || '')) {
			return 'ri:file-zip-fill';
		}

		return 'ri:file-fill';
	}

	// Get icon color for file type
	function getFileIconColor(file: DriveFile): string {
		if (file.is_folder) return 'text-yellow-500';

		const ext = file.filename.split('.').pop()?.toLowerCase();
		const mime = file.mime_type?.toLowerCase() || '';

		if (mime.startsWith('image/') || ['jpg', 'jpeg', 'png', 'gif', 'webp', 'svg'].includes(ext || '')) {
			return 'text-purple-500';
		}
		if (mime.startsWith('video/') || ['mp4', 'mov', 'avi', 'mkv', 'webm'].includes(ext || '')) {
			return 'text-red-500';
		}
		if (mime.startsWith('audio/') || ['mp3', 'wav', 'ogg', 'm4a', 'flac'].includes(ext || '')) {
			return 'text-pink-500';
		}
		if (['pdf'].includes(ext || '')) return 'text-red-600';
		if (['doc', 'docx'].includes(ext || '')) return 'text-blue-600';
		if (['xls', 'xlsx'].includes(ext || '')) return 'text-green-600';

		return 'text-foreground-subtle';
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
			error = e instanceof Error ? e.message : 'Failed to load folder';
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
			const a = document.createElement('a');
			a.href = url;
			a.download = file.filename;
			document.body.appendChild(a);
			a.click();
			document.body.removeChild(a);
			URL.revokeObjectURL(url);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Download failed';
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
			const res = await fetch('/api/drive/usage');
			if (res.ok) usage = await res.json();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Upload failed';
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
			newFolderName = '';
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to create folder';
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
			const res = await fetch('/api/drive/usage');
			if (res.ok) usage = await res.json();
			showToast(`"${fileToDelete.filename}" moved to Trash`);
			fileToDelete = null;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Delete failed';
		} finally {
			deleting = false;
		}
	}

	// Restore file from trash
	async function handleRestore() {
		if (!fileToRestore) return;

		restoring = true;
		error = null;

		try {
			await restoreDriveFile(fileToRestore.id);
			const newTrash = await listDriveTrash();
			trashFiles = newTrash;
			// Refresh usage
			const res = await fetch('/api/drive/usage');
			if (res.ok) usage = await res.json();
			showToast(`"${fileToRestore.filename}" restored`);
			fileToRestore = null;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Restore failed';
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
			const newTrash = await listDriveTrash();
			trashFiles = newTrash;
			// Refresh usage
			const res = await fetch('/api/drive/usage');
			if (res.ok) usage = await res.json();
			showToast(`"${fileToPurge.filename}" permanently deleted`);
			fileToPurge = null;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Permanent delete failed';
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
			// Refresh usage
			const res = await fetch('/api/drive/usage');
			if (res.ok) usage = await res.json();
			showToast(`${result.deleted_count} items permanently deleted`);
			showEmptyTrashModal = false;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to empty trash';
		} finally {
			emptyingTrash = false;
		}
	}
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

		<!-- View Mode Tabs -->
		<div class="flex items-center gap-1 mb-4 border-b border-border">
			<button
				class="px-4 py-2 text-sm font-medium transition-colors relative"
				class:text-foreground={viewMode === 'files'}
				class:text-foreground-muted={viewMode !== 'files'}
				onclick={() => switchViewMode('files')}
			>
				<span class="flex items-center gap-2">
					<iconify-icon icon="ri:folder-line"></iconify-icon>
					Files
				</span>
				{#if viewMode === 'files'}
					<div class="absolute bottom-0 left-0 right-0 h-0.5 bg-foreground"></div>
				{/if}
			</button>
			<button
				class="px-4 py-2 text-sm font-medium transition-colors relative"
				class:text-foreground={viewMode === 'trash'}
				class:text-foreground-muted={viewMode !== 'trash'}
				onclick={() => switchViewMode('trash')}
			>
				<span class="flex items-center gap-2">
					<iconify-icon icon="ri:delete-bin-line"></iconify-icon>
					Trash
					{#if trashFiles.length > 0 || (viewMode === 'trash' && trashFiles.length > 0)}
						<span class="ml-1 px-1.5 py-0.5 text-xs bg-foreground-subtle/20 rounded-full">
							{trashFiles.length}
						</span>
					{/if}
				</span>
				{#if viewMode === 'trash'}
					<div class="absolute bottom-0 left-0 right-0 h-0.5 bg-foreground"></div>
				{/if}
			</button>
		</div>

		<!-- Toolbar -->
		{#if viewMode === 'files'}
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
		{:else}
			<!-- Trash Toolbar -->
			<div class="flex items-center justify-between mb-4">
				<p class="text-sm text-foreground-muted">
					Items in Trash are automatically deleted after 30 days
				</p>
				{#if trashFiles.length > 0}
					<button
						class="flex items-center gap-2 px-3 py-1.5 text-sm text-red-500 hover:bg-red-500/10 rounded-lg transition-colors"
						onclick={() => (showEmptyTrashModal = true)}
					>
						<iconify-icon icon="ri:delete-bin-line"></iconify-icon>
						Empty Trash
					</button>
				{/if}
			</div>
		{/if}

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

		<!-- Files View -->
		{#if viewMode === 'files'}
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
							{currentPath ? 'This folder is empty' : 'No files yet'}
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
									onkeydown={(e) => e.key === 'Enter' && handleFileClick(file)}
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
										{file.is_folder ? '-' : formatBytes(file.size_bytes)}
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
		{:else}
			<!-- Trash View -->
			<div class="border border-border rounded-lg overflow-hidden">
				{#if loading}
					<div class="p-12 text-center">
						<iconify-icon icon="ri:loader-4-line" class="text-4xl text-foreground-subtle animate-spin"></iconify-icon>
						<p class="text-foreground-muted mt-2">Loading...</p>
					</div>
				{:else if trashFiles.length === 0}
					<div class="p-12 text-center">
						<iconify-icon icon="ri:delete-bin-line" class="text-6xl text-foreground-subtle mb-4"></iconify-icon>
						<h3 class="text-lg font-medium text-foreground mb-2">Trash is empty</h3>
						<p class="text-foreground-muted">
							Deleted files will appear here for 30 days before being permanently removed
						</p>
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
									Deleted
								</th>
								<th class="px-4 py-3 text-right text-xs font-medium text-foreground-subtle uppercase tracking-wide">
									Expires
								</th>
								<th class="px-4 py-3 w-24"></th>
							</tr>
						</thead>
						<tbody class="divide-y divide-border">
							{#each trashFiles as file}
								{@const daysRemaining = file.deleted_at ? getDaysRemaining(file.deleted_at) : 30}
								{@const isWarning = daysRemaining <= 7}
								{@const isCritical = daysRemaining <= 3}
								<tr class="group hover:bg-surface-elevated transition-colors">
									<td class="px-4 py-3">
										<div class="flex items-center gap-3">
											<iconify-icon
												icon={getFileIcon(file)}
												class="text-xl {getFileIconColor(file)} opacity-50"
											></iconify-icon>
											<div>
												<span class="text-sm text-foreground">{file.filename}</span>
												<p class="text-xs text-foreground-subtle">{file.path}</p>
											</div>
										</div>
									</td>
									<td class="px-4 py-3 text-right text-sm text-foreground-muted">
										{file.is_folder ? '-' : formatBytes(file.size_bytes)}
									</td>
									<td class="px-4 py-3 text-right text-sm text-foreground-muted">
										{file.deleted_at ? formatDate(file.deleted_at) : '-'}
									</td>
									<td class="px-4 py-3 text-right">
										<span
											class="text-xs px-2 py-1 rounded-full {isCritical
												? 'bg-red-500/10 text-red-600 dark:text-red-400'
												: isWarning
													? 'bg-yellow-500/10 text-yellow-600 dark:text-yellow-400'
													: 'text-foreground-muted'}"
										>
											{daysRemaining} {daysRemaining === 1 ? 'day' : 'days'}
										</span>
									</td>
									<td class="px-4 py-3 text-right">
										<div class="flex items-center justify-end gap-1 opacity-0 group-hover:opacity-100 transition-all">
											<button
												class="p-1 text-foreground-subtle hover:text-emerald-500 transition-colors"
												onclick={() => (fileToRestore = file)}
												aria-label="Restore {file.filename}"
												title="Restore"
											>
												<iconify-icon icon="ri:arrow-go-back-line"></iconify-icon>
											</button>
											<button
												class="p-1 text-foreground-subtle hover:text-red-500 transition-colors"
												onclick={() => (fileToPurge = file)}
												aria-label="Delete forever {file.filename}"
												title="Delete forever"
											>
												<iconify-icon icon="ri:delete-bin-7-line"></iconify-icon>
											</button>
										</div>
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				{/if}
			</div>
		{/if}
	</div>
</Page>

<!-- Toast Notification -->
{#if toastMessage}
	<div class="fixed bottom-4 left-1/2 -translate-x-1/2 z-50 animate-in fade-in slide-in-from-bottom-2 duration-200">
		<div class="bg-foreground text-background px-4 py-2 rounded-lg shadow-lg text-sm">
			{toastMessage}
		</div>
	</div>
{/if}

<!-- New Folder Modal -->
{#if showNewFolderModal}
	<div
		class="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
		onclick={() => (showNewFolderModal = false)}
		onkeydown={(e) => e.key === 'Escape' && (showNewFolderModal = false)}
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
				onkeydown={(e) => e.key === 'Enter' && handleCreateFolder()}
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
					{creatingFolder ? 'Creating...' : 'Create'}
				</button>
			</div>
		</div>
	</div>
{/if}

<!-- Delete Confirmation Modal (Soft Delete) -->
{#if fileToDelete}
	<div
		class="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
		onclick={() => (fileToDelete = null)}
		onkeydown={(e) => e.key === 'Escape' && (fileToDelete = null)}
		role="dialog"
		aria-modal="true"
		tabindex="-1"
	>
		<div
			class="bg-surface border border-border rounded-xl shadow-xl p-6 w-full max-w-md"
			onclick={(e) => e.stopPropagation()}
			role="document"
		>
			<h2 class="text-lg font-medium text-foreground mb-2">Move to Trash?</h2>
			<p class="text-foreground-muted mb-6">
				"{fileToDelete.filename}" will be moved to Trash.
				{#if fileToDelete.is_folder}
					This includes all contents inside the folder.
				{/if}
				You can restore it within 30 days.
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
					{deleting ? 'Moving...' : 'Move to Trash'}
				</button>
			</div>
		</div>
	</div>
{/if}

<!-- Restore Confirmation Modal -->
{#if fileToRestore}
	<div
		class="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
		onclick={() => (fileToRestore = null)}
		onkeydown={(e) => e.key === 'Escape' && (fileToRestore = null)}
		role="dialog"
		aria-modal="true"
		tabindex="-1"
	>
		<div
			class="bg-surface border border-border rounded-xl shadow-xl p-6 w-full max-w-md"
			onclick={(e) => e.stopPropagation()}
			role="document"
		>
			<h2 class="text-lg font-medium text-foreground mb-2">Restore {fileToRestore.is_folder ? 'Folder' : 'File'}?</h2>
			<p class="text-foreground-muted mb-6">
				"{fileToRestore.filename}" will be restored to its original location.
				{#if fileToRestore.is_folder}
					This includes all contents inside the folder.
				{/if}
			</p>
			<div class="flex justify-end gap-3">
				<button
					class="px-4 py-2 text-sm text-foreground-muted hover:text-foreground transition-colors"
					onclick={() => (fileToRestore = null)}
				>
					Cancel
				</button>
				<button
					class="px-4 py-2 text-sm bg-emerald-500 text-white rounded-lg hover:bg-emerald-600 transition-colors disabled:opacity-50"
					onclick={handleRestore}
					disabled={restoring}
				>
					{restoring ? 'Restoring...' : 'Restore'}
				</button>
			</div>
		</div>
	</div>
{/if}

<!-- Purge Confirmation Modal (Permanent Delete) -->
{#if fileToPurge}
	<div
		class="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
		onclick={() => (fileToPurge = null)}
		onkeydown={(e) => e.key === 'Escape' && (fileToPurge = null)}
		role="dialog"
		aria-modal="true"
		tabindex="-1"
	>
		<div
			class="bg-surface border border-border rounded-xl shadow-xl p-6 w-full max-w-md"
			onclick={(e) => e.stopPropagation()}
			role="document"
		>
			<h2 class="text-lg font-medium text-foreground mb-2">Delete Forever?</h2>
			<p class="text-foreground-muted mb-6">
				"{fileToPurge.filename}" will be permanently deleted.
				{#if fileToPurge.is_folder}
					This includes all contents inside the folder.
				{/if}
				<strong class="text-red-500">This action cannot be undone.</strong>
			</p>
			<div class="flex justify-end gap-3">
				<button
					class="px-4 py-2 text-sm text-foreground-muted hover:text-foreground transition-colors"
					onclick={() => (fileToPurge = null)}
				>
					Cancel
				</button>
				<button
					class="px-4 py-2 text-sm bg-red-500 text-white rounded-lg hover:bg-red-600 transition-colors disabled:opacity-50"
					onclick={handlePurge}
					disabled={purging}
				>
					{purging ? 'Deleting...' : 'Delete Forever'}
				</button>
			</div>
		</div>
	</div>
{/if}

<!-- Empty Trash Confirmation Modal -->
{#if showEmptyTrashModal}
	<div
		class="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
		onclick={() => (showEmptyTrashModal = false)}
		onkeydown={(e) => e.key === 'Escape' && (showEmptyTrashModal = false)}
		role="dialog"
		aria-modal="true"
		tabindex="-1"
	>
		<div
			class="bg-surface border border-border rounded-xl shadow-xl p-6 w-full max-w-md"
			onclick={(e) => e.stopPropagation()}
			role="document"
		>
			<h2 class="text-lg font-medium text-foreground mb-2">Empty Trash?</h2>
			<p class="text-foreground-muted mb-6">
				All {trashFiles.length} {trashFiles.length === 1 ? 'item' : 'items'} in Trash will be permanently deleted.
				<strong class="text-red-500">This action cannot be undone.</strong>
			</p>
			<div class="flex justify-end gap-3">
				<button
					class="px-4 py-2 text-sm text-foreground-muted hover:text-foreground transition-colors"
					onclick={() => (showEmptyTrashModal = false)}
				>
					Cancel
				</button>
				<button
					class="px-4 py-2 text-sm bg-red-500 text-white rounded-lg hover:bg-red-600 transition-colors disabled:opacity-50"
					onclick={handleEmptyTrash}
					disabled={emptyingTrash}
				>
					{emptyingTrash ? 'Emptying...' : 'Empty Trash'}
				</button>
			</div>
		</div>
	</div>
{/if}
