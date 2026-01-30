<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { Page } from '$lib';
	import type { DriveFile, DriveUsage } from '$lib/api/client';
	import {
		listDriveFiles,
		uploadDriveFile,
		downloadDriveFile,
		deleteDriveFile,
		createDriveFolder,
		moveDriveFile
	} from '$lib/api/client';
	import Icon from '$lib/components/Icon.svelte';
	import Modal from '$lib/components/Modal.svelte';
	import { onMount } from 'svelte';
	import { spaceStore } from '$lib/stores/space.svelte';
	import { contextMenu } from '$lib/stores/contextMenu.svelte';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// State
	let files = $state<DriveFile[]>([]);
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

	// Rename state
	let renamingFile = $state<DriveFile | null>(null);
	let renameValue = $state('');
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
			const [filesData, usageResponse] = await Promise.all([
				listDriveFiles(currentPath),
				fetch('/api/drive/usage')
			]);
			files = filesData;
			if (usageResponse.ok) {
				usage = await usageResponse.json();
			}
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load drive data';
		} finally {
			loading = false;
		}
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

	function navigateToTrash() {
		spaceStore.openTabFromRoute('/trash');
	}

	// Context menu for files/folders
	function showFileContextMenu(e: MouseEvent, file: DriveFile) {
		e.preventDefault();
		e.stopPropagation();

		const items = file.is_folder
			? [
					{
						id: 'open',
						label: 'Open',
						icon: 'ri:folder-open-line',
						action: () => navigateToFolder(file.path)
					},
					{
						id: 'rename',
						label: 'Rename',
						icon: 'ri:pencil-line',
						dividerBefore: true,
						action: () => {
							renamingFile = file;
							renameValue = file.filename;
						}
					},
					{
						id: 'delete',
						label: 'Move to Trash',
						icon: 'ri:delete-bin-line',
						variant: 'destructive' as const,
						dividerBefore: true,
						action: () => {
							fileToDelete = file;
						}
					}
				]
			: [
					{
						id: 'download',
						label: 'Download',
						icon: 'ri:download-line',
						action: () => handleDownload(file)
					},
					{
						id: 'rename',
						label: 'Rename',
						icon: 'ri:pencil-line',
						dividerBefore: true,
						action: () => {
							renamingFile = file;
							renameValue = file.filename;
						}
					},
					{
						id: 'delete',
						label: 'Move to Trash',
						icon: 'ri:delete-bin-line',
						variant: 'destructive' as const,
						dividerBefore: true,
						action: () => {
							fileToDelete = file;
						}
					}
				];

		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}

	// Inline rename
	async function handleRename() {
		if (!renamingFile || !renameValue.trim() || renameValue.trim() === renamingFile.filename) {
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
			renameValue = '';
		} catch (e) {
			error = e instanceof Error ? e.message : 'Rename failed';
		} finally {
			renaming = false;
		}
	}

	function cancelRename() {
		renamingFile = null;
		renameValue = '';
	}

	// Svelte action to auto-focus an input element
	function autofocus(node: HTMLInputElement) {
		node.focus();
		// Select filename without extension for files
		const dotIndex = node.value.lastIndexOf('.');
		if (dotIndex > 0) {
			node.setSelectionRange(0, dotIndex);
		} else {
			node.select();
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
			{@const drivePercent = usage.quota_bytes > 0 ? (usage.drive_bytes / usage.quota_bytes) * 100 : 0}
			{@const dataLakePercent = usage.quota_bytes > 0 ? (usage.data_lake_bytes / usage.quota_bytes) * 100 : 0}
			<div class="bg-surface border border-border rounded-lg p-4 mb-6">
				<div class="flex items-center justify-between mb-2">
					<span class="text-sm text-foreground-muted">
						{formatBytes(usage.total_bytes)} of {formatBytes(usage.quota_bytes)} used
					</span>
					<span class="text-xs text-foreground-subtle uppercase tracking-wide">
						{usage.tier} tier
					</span>
				</div>
				<!-- Segmented progress bar -->
				<div class="h-3 bg-border rounded-full overflow-hidden flex">
					{#if drivePercent > 0}
						<div
							class="h-full bg-blue-500 transition-all duration-300"
							style="width: {Math.min(drivePercent, 100 - dataLakePercent)}%"
						></div>
					{/if}
					{#if dataLakePercent > 0}
						<div
							class="h-full bg-purple-500 transition-all duration-300"
							style="width: {Math.min(dataLakePercent, 100 - drivePercent)}%"
						></div>
					{/if}
				</div>
				<!-- Legend -->
				<div class="flex flex-wrap gap-4 mt-3 text-xs text-foreground-muted">
					<span class="flex items-center gap-1.5">
						<span class="w-2.5 h-2.5 bg-blue-500 rounded-sm"></span>
						Drive ({formatBytes(usage.drive_bytes)})
					</span>
					<a href="/virtues/lake" class="flex items-center gap-1.5 hover:text-foreground transition-colors">
						<span class="w-2.5 h-2.5 bg-purple-500 rounded-sm"></span>
						Lake ({formatBytes(usage.data_lake_bytes)})
					</a>
					<span class="flex items-center gap-1.5">
						<span class="w-2.5 h-2.5 bg-border rounded-sm"></span>
						Available ({formatBytes(usage.quota_bytes - usage.total_bytes)})
					</span>
				</div>
				<div class="flex gap-4 mt-2 text-xs text-foreground-subtle">
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
						<Icon icon="ri:arrow-right-s-line" class="text-foreground-subtle"/>
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
					class="flex items-center gap-1.5 px-3 py-1.5 text-sm text-foreground-muted hover:text-foreground hover:bg-surface-elevated rounded-lg transition-colors"
					onclick={navigateToTrash}
				>
					<Icon icon="ri:delete-bin-line"/>
					Trash
				</button>
				<button
					class="flex items-center gap-2 px-3 py-1.5 text-sm text-foreground-muted hover:text-foreground hover:bg-surface-elevated rounded-lg transition-colors"
					onclick={() => (showNewFolderModal = true)}
				>
					<Icon icon="ri:folder-add-line"/>
					New folder
				</button>
				<button
					class="flex items-center gap-2 px-3 py-1.5 text-sm bg-foreground text-background hover:bg-foreground/90 rounded-lg transition-colors"
					onclick={() => fileInput?.click()}
					disabled={uploading}
				>
					<Icon icon="ri:upload-2-line"/>
					Upload
				</button>
				<input
					bind:this={fileInput}
					type="file"
					multiple
					class="hidden"
					aria-hidden="true"
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
					<Icon icon="ri:loader-4-line" class="animate-spin text-blue-500"/>
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

		<!-- Files Table -->
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
					<Icon icon="ri:loader-4-line" class="text-4xl text-foreground-subtle animate-spin"/>
					<p class="text-foreground-muted mt-2">Loading...</p>
				</div>
			{:else if files.length === 0}
				<div class="p-12 text-center">
					<Icon icon="ri:folder-open-line" class="text-6xl text-foreground-subtle mb-4"/>
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
						<Icon icon="ri:upload-2-line"/>
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
								class="group hover:bg-surface-elevated transition-colors cursor-pointer"
								onclick={() => !renamingFile && handleFileClick(file)}
								onkeydown={(e) => e.key === 'Enter' && !renamingFile && handleFileClick(file)}
								oncontextmenu={(e) => showFileContextMenu(e, file)}
							>
								<td class="px-4 py-3">
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
												class="text-sm text-foreground bg-transparent border border-border rounded px-1.5 py-0.5 outline-none focus:border-blue-500 w-full max-w-xs"
												onclick={(e) => e.stopPropagation()}
												onkeydown={(e) => {
													e.stopPropagation();
													if (e.key === 'Enter') handleRename();
													if (e.key === 'Escape') cancelRename();
												}}
												onblur={cancelRename}
												disabled={renaming}
											/>
										{:else}
											<span class="text-sm text-foreground">{file.filename}</span>
										{/if}
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
										class="opacity-0 group-hover:opacity-100 p-1 text-foreground-subtle hover:text-foreground transition-all"
										onclick={(e) => {
											e.stopPropagation();
											showFileContextMenu(e, file);
										}}
										aria-label="Actions for {file.filename}"
									>
										<Icon icon="ri:more-2-fill"/>
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

<!-- Toast Notification -->
{#if toastMessage}
	<div class="fixed bottom-4 left-1/2 -translate-x-1/2 z-50 animate-in fade-in slide-in-from-bottom-2 duration-200">
		<div class="bg-foreground text-background px-4 py-2 rounded-lg shadow-lg text-sm">
			{toastMessage}
		</div>
	</div>
{/if}

<!-- New Folder Modal -->
<Modal open={showNewFolderModal} onClose={() => { showNewFolderModal = false; newFolderName = ''; }} title="New Folder" width="sm">
	<input
		type="text"
		bind:value={newFolderName}
		placeholder="Folder name"
		class="modal-input"
		onkeydown={(e) => e.key === 'Enter' && handleCreateFolder()}
	/>
	{#snippet footer()}
		<button class="modal-btn modal-btn-secondary" onclick={() => { showNewFolderModal = false; newFolderName = ''; }}>
			Cancel
		</button>
		<button
			class="modal-btn modal-btn-primary"
			onclick={handleCreateFolder}
			disabled={creatingFolder || !newFolderName.trim()}
		>
			{creatingFolder ? 'Creating...' : 'Create'}
		</button>
	{/snippet}
</Modal>

<!-- Delete Confirmation Modal (Soft Delete) -->
<Modal open={!!fileToDelete} onClose={() => (fileToDelete = null)} title="Move to Trash?" width="sm">
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
		<button class="modal-btn modal-btn-secondary" onclick={() => (fileToDelete = null)}>
			Cancel
		</button>
		<button
			class="modal-btn bg-red-500 text-white hover:bg-red-600 disabled:opacity-50"
			onclick={handleDelete}
			disabled={deleting}
		>
			{deleting ? 'Moving...' : 'Move to Trash'}
		</button>
	{/snippet}
</Modal>
