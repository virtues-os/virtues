<script lang="ts">
	/**
	 * CoverImagePicker - Cover image selection modal
	 *
	 * A centered modal for selecting a cover image via upload, URL, or Unsplash.
	 * Follows the same self-contained modal pattern as IconPicker.
	 */
	import Icon from './Icon.svelte';
	import { uploadDriveFile } from '$lib/api/client';

	interface Props {
		/** Current cover URL */
		value?: string | null;
		/** Called when a cover is selected (url string) or removed (null) */
		onSelect: (url: string | null) => void;
		/** Called when picker is closed */
		onClose: () => void;
	}

	let { value = null, onSelect, onClose }: Props = $props();

	let activeTab = $state<'upload' | 'link' | 'unsplash'>('upload');

	// Upload tab state
	let uploading = $state(false);
	let uploadProgress = $state(0);
	let uploadError = $state<string | null>(null);
	let dragOver = $state(false);
	let fileInputEl: HTMLInputElement;

	// Link tab state
	let linkUrl = $state('');
	let linkPreviewLoaded = $state(false);
	let linkPreviewError = $state(false);

	// Unsplash tab state
	interface UnsplashPhoto {
		id: string;
		description: string | null;
		urls: { raw: string; full: string; regular: string; small: string; thumb: string };
		user: { name: string; username: string };
		width: number;
		height: number;
	}
	let unsplashQuery = $state('');
	let unsplashResults = $state<UnsplashPhoto[]>([]);
	let unsplashLoading = $state(false);
	let unsplashError = $state<string | null>(null);
	let unsplashSearchTimeout: ReturnType<typeof setTimeout> | null = null;
	let searchInputEl: HTMLInputElement;

	// Portal action - moves element to body for proper z-index stacking
	function portal(node: HTMLElement) {
		document.body.appendChild(node);
		return {
			destroy() {
				node.remove();
			}
		};
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.preventDefault();
			onClose();
		}
	}

	// Upload handlers
	async function handleFileSelect(files: FileList | null) {
		if (!files || files.length === 0) return;
		const file = files[0];

		if (!file.type.startsWith('image/')) {
			uploadError = 'Please select an image file';
			return;
		}

		uploading = true;
		uploadProgress = 0;
		uploadError = null;

		try {
			const driveFile = await uploadDriveFile('covers', file, (progress) => {
				uploadProgress = progress;
			});
			const coverUrl = `/api/drive/files/${driveFile.id}/download`;
			onSelect(coverUrl);
			onClose();
		} catch (e) {
			uploadError = e instanceof Error ? e.message : 'Upload failed';
		} finally {
			uploading = false;
			uploadProgress = 0;
		}
	}

	function handleDrop(e: DragEvent) {
		e.preventDefault();
		dragOver = false;
		if (e.dataTransfer?.files) {
			handleFileSelect(e.dataTransfer.files);
		}
	}

	function handleDragOver(e: DragEvent) {
		e.preventDefault();
		dragOver = true;
	}

	function handleDragLeave() {
		dragOver = false;
	}

	// Link handlers
	function handleLinkSubmit() {
		if (!linkUrl.trim()) return;
		onSelect(linkUrl.trim());
		onClose();
	}

	// Unsplash handlers
	async function searchUnsplash(query: string) {
		if (!query.trim()) {
			unsplashResults = [];
			return;
		}
		unsplashLoading = true;
		unsplashError = null;
		try {
			const res = await fetch('/api/unsplash/search', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ query: query.trim(), per_page: 20 }),
			});
			if (!res.ok) {
				const err = await res.json().catch(() => ({ error: res.statusText }));
				throw new Error(err.error?.message || err.error || 'Search failed');
			}
			const data = await res.json();
			unsplashResults = data.results || [];
		} catch (e) {
			unsplashError = e instanceof Error ? e.message : 'Search failed';
			unsplashResults = [];
		} finally {
			unsplashLoading = false;
		}
	}

	function handleUnsplashInput() {
		if (unsplashSearchTimeout) clearTimeout(unsplashSearchTimeout);
		unsplashSearchTimeout = setTimeout(() => {
			searchUnsplash(unsplashQuery);
		}, 400);
	}

	function selectUnsplashPhoto(photo: UnsplashPhoto) {
		// Use the regular size (1080px wide) — hotlinked per Unsplash requirements
		onSelect(photo.urls.regular);
		onClose();
	}

	// Remove handler
	function handleRemove() {
		onSelect(null);
		onClose();
	}
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="picker-backdrop" use:portal onclick={(e) => e.target === e.currentTarget && onClose()}>
	<div class="cover-picker">
		<!-- Tabs -->
		<div class="picker-tabs">
			<button
				class="tab"
				class:active={activeTab === 'upload'}
				onclick={() => activeTab = 'upload'}
			>
				<Icon icon="ri:upload-cloud-line" width="14" />
				Upload
			</button>
			<button
				class="tab"
				class:active={activeTab === 'link'}
				onclick={() => activeTab = 'link'}
			>
				<Icon icon="ri:link" width="14" />
				Link
			</button>
			<button
				class="tab"
				class:active={activeTab === 'unsplash'}
				onclick={() => activeTab = 'unsplash'}
			>
				<Icon icon="ri:unsplash-fill" width="14" />
				Unsplash
			</button>
		</div>

		<!-- Content -->
		<div class="picker-content">
			{#if activeTab === 'upload'}
				<!-- svelte-ignore a11y_no_static_element_interactions -->
				<div
					class="upload-zone"
					class:drag-over={dragOver}
					ondrop={handleDrop}
					ondragover={handleDragOver}
					ondragleave={handleDragLeave}
					role="button"
					tabindex="0"
					onclick={() => fileInputEl?.click()}
					onkeydown={(e) => e.key === 'Enter' && fileInputEl?.click()}
				>
					{#if uploading}
						<Icon icon="ri:loader-4-line" width="24" />
						<span class="upload-text">Uploading... {uploadProgress}%</span>
						<div class="progress-bar">
							<div class="progress-fill" style="width: {uploadProgress}%"></div>
						</div>
					{:else}
						<Icon icon="ri:upload-cloud-line" width="24" />
						<span class="upload-text">Drop an image here or click to browse</span>
						<span class="upload-hint">PNG, JPG, GIF, WebP</span>
					{/if}
				</div>
				<input
					type="file"
					accept="image/*"
					bind:this={fileInputEl}
					onchange={(e) => handleFileSelect(e.currentTarget.files)}
					hidden
				/>
				{#if uploadError}
					<div class="upload-error">
						<Icon icon="ri:error-warning-line" width="14" />
						{uploadError}
					</div>
				{/if}

			{:else if activeTab === 'link'}
				<div class="link-tab">
					<div class="link-input-row">
						<input
							type="url"
							bind:value={linkUrl}
							placeholder="Paste an image URL..."
							class="link-input"
							onkeydown={(e) => e.key === 'Enter' && handleLinkSubmit()}
						/>
						<button
							class="link-submit-btn"
							onclick={handleLinkSubmit}
							disabled={!linkUrl.trim()}
						>
							Apply
						</button>
					</div>
					{#if linkUrl.trim()}
						<div class="link-preview">
							<img
								src={linkUrl}
								alt="Preview"
								onload={() => { linkPreviewLoaded = true; linkPreviewError = false; }}
								onerror={() => { linkPreviewError = true; linkPreviewLoaded = false; }}
								class="preview-img"
								class:loaded={linkPreviewLoaded}
							/>
							{#if linkPreviewError}
								<div class="preview-error">Could not load image from this URL</div>
							{/if}
						</div>
					{/if}
				</div>

			{:else if activeTab === 'unsplash'}
				<div class="unsplash-tab">
					<input
						type="text"
						bind:value={unsplashQuery}
						bind:this={searchInputEl}
						placeholder="Search photos..."
						class="unsplash-search"
						oninput={handleUnsplashInput}
					/>

					{#if unsplashLoading}
						<div class="unsplash-status">
							<Icon icon="ri:loader-4-line" width="20" />
							<span>Searching...</span>
						</div>
					{:else if unsplashError}
						<div class="unsplash-status error">
							<Icon icon="ri:error-warning-line" width="16" />
							<span>{unsplashError}</span>
						</div>
					{:else if unsplashResults.length > 0}
						<div class="photo-grid">
							{#each unsplashResults as photo (photo.id)}
								<button
									class="photo-item"
									onclick={() => selectUnsplashPhoto(photo)}
									title={photo.description || `Photo by ${photo.user.name}`}
								>
									<img
										src={photo.urls.small}
										alt={photo.description || `Photo by ${photo.user.name}`}
										loading="lazy"
									/>
									<span class="photo-credit">
										{photo.user.name}
									</span>
								</button>
							{/each}
						</div>
					{:else if unsplashQuery.trim()}
						<div class="unsplash-status">
							<span>No results for "{unsplashQuery}"</span>
						</div>
					{:else}
						<div class="unsplash-status empty">
							<Icon icon="ri:search-line" width="20" />
							<span>Search Unsplash for free photos</span>
						</div>
					{/if}
				</div>
			{/if}
		</div>

		<!-- Footer with remove option -->
		{#if value}
			<div class="picker-footer">
				<button class="remove-btn" onclick={handleRemove}>
					<Icon icon="ri:delete-bin-line" width="14" />
					Remove cover
				</button>
			</div>
		{/if}
	</div>
</div>

<style>
	.picker-backdrop {
		position: fixed;
		inset: 0;
		z-index: 10000;
		background: rgba(0, 0, 0, 0.4);
		display: flex;
		align-items: flex-start;
		justify-content: center;
		padding-top: 12vh;
		animation: backdrop-in 150ms ease-out;
	}

	@keyframes backdrop-in {
		from { opacity: 0; }
		to { opacity: 1; }
	}

	.cover-picker {
		width: 420px;
		max-height: 480px;
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border);
		border-radius: 12px;
		box-shadow: 0 16px 48px rgba(0, 0, 0, 0.24);
		display: flex;
		flex-direction: column;
		overflow: hidden;
		animation: picker-in 150ms ease-out;
	}

	@keyframes picker-in {
		from {
			opacity: 0;
			transform: translateY(-8px) scale(0.96);
		}
		to {
			opacity: 1;
			transform: translateY(0) scale(1);
		}
	}

	.picker-tabs {
		display: flex;
		border-bottom: 1px solid var(--color-border);
		padding: 0 8px;
	}

	.tab {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 6px;
		padding: 10px 12px;
		font-size: 13px;
		font-weight: 500;
		color: var(--color-foreground-muted);
		background: none;
		border: none;
		border-bottom: 2px solid transparent;
		cursor: pointer;
		transition: all 150ms;
	}

	.tab:hover {
		color: var(--color-foreground);
	}

	.tab.active {
		color: var(--color-primary);
		border-bottom-color: var(--color-primary);
	}

	.picker-content {
		flex: 1;
		overflow-y: auto;
		padding: 16px;
		min-height: 200px;
	}

	/* Upload tab */
	.upload-zone {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 8px;
		padding: 32px 16px;
		border: 2px dashed var(--color-border);
		border-radius: 8px;
		cursor: pointer;
		transition: all 150ms;
		color: var(--color-foreground-muted);
		outline: none;
	}

	.upload-zone:hover,
	.upload-zone:focus-visible,
	.upload-zone.drag-over {
		border-color: var(--color-primary);
		background: color-mix(in srgb, var(--color-primary) 5%, transparent);
		color: var(--color-primary);
	}

	.upload-text {
		font-size: 14px;
		font-weight: 500;
	}

	.upload-hint {
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}

	.progress-bar {
		width: 100%;
		max-width: 200px;
		height: 4px;
		background: var(--color-surface-overlay);
		border-radius: 2px;
		overflow: hidden;
		margin-top: 4px;
	}

	.progress-fill {
		height: 100%;
		background: var(--color-primary);
		transition: width 200ms ease;
	}

	.upload-error {
		display: flex;
		align-items: center;
		gap: 6px;
		margin-top: 12px;
		padding: 8px 12px;
		font-size: 13px;
		color: var(--color-error);
		background: color-mix(in srgb, var(--color-error) 10%, transparent);
		border-radius: 6px;
	}

	/* Link tab */
	.link-tab {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.link-input-row {
		display: flex;
		gap: 8px;
	}

	.link-input {
		flex: 1;
		padding: 10px 12px;
		font-size: 14px;
		border: 1px solid var(--color-border);
		border-radius: 8px;
		background: var(--color-background);
		color: var(--color-foreground);
		outline: none;
	}

	.link-input::placeholder {
		color: var(--color-foreground-subtle);
	}

	.link-input:focus {
		border-color: var(--color-primary);
	}

	.link-submit-btn {
		padding: 10px 16px;
		font-size: 13px;
		font-weight: 500;
		color: white;
		background: var(--color-primary);
		border: none;
		border-radius: 8px;
		cursor: pointer;
		transition: opacity 150ms;
		white-space: nowrap;
	}

	.link-submit-btn:hover {
		opacity: 0.9;
	}

	.link-submit-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.link-preview {
		border-radius: 8px;
		overflow: hidden;
		background: var(--color-surface-overlay);
	}

	.preview-img {
		width: 100%;
		height: auto;
		object-fit: cover;
		max-height: 160px;
		opacity: 0;
		transition: opacity 200ms;
		display: block;
	}

	.preview-img.loaded {
		opacity: 1;
	}

	.preview-error {
		padding: 16px;
		text-align: center;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	/* Unsplash tab */
	.unsplash-tab {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.unsplash-search {
		width: 100%;
		padding: 10px 12px;
		font-size: 14px;
		border: 1px solid var(--color-border);
		border-radius: 8px;
		background: var(--color-background);
		color: var(--color-foreground);
		outline: none;
		box-sizing: border-box;
	}

	.unsplash-search::placeholder {
		color: var(--color-foreground-subtle);
	}

	.unsplash-search:focus {
		border-color: var(--color-primary);
	}

	.unsplash-status {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 8px;
		padding: 32px 16px;
		color: var(--color-foreground-muted);
		font-size: 14px;
	}

	.unsplash-status.error {
		color: var(--color-error);
	}

	.unsplash-status.empty {
		color: var(--color-foreground-subtle);
	}

	.photo-grid {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 4px;
		max-height: 280px;
		overflow-y: auto;
		border-radius: 6px;
	}

	.photo-item {
		position: relative;
		aspect-ratio: 4 / 3;
		overflow: hidden;
		border: none;
		padding: 0;
		cursor: pointer;
		background: var(--color-surface-overlay);
	}

	.photo-item img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
		transition: transform 150ms ease;
	}

	.photo-item:hover img {
		transform: scale(1.05);
	}

	.photo-credit {
		position: absolute;
		bottom: 0;
		left: 0;
		right: 0;
		padding: 4px 6px;
		font-size: 10px;
		color: white;
		background: linear-gradient(transparent, rgba(0, 0, 0, 0.6));
		opacity: 0;
		transition: opacity 150ms;
		text-align: left;
		pointer-events: none;
	}

	.photo-item:hover .photo-credit {
		opacity: 1;
	}

	/* Footer */
	.picker-footer {
		padding: 8px 12px;
		border-top: 1px solid var(--color-border);
	}

	.remove-btn {
		display: flex;
		align-items: center;
		gap: 6px;
		width: 100%;
		padding: 8px 12px;
		font-size: 13px;
		color: var(--color-error);
		background: none;
		border: none;
		border-radius: 6px;
		cursor: pointer;
		transition: background 100ms;
	}

	.remove-btn:hover {
		background: color-mix(in srgb, var(--color-error) 10%, transparent);
	}

	/* Spinner */
	:global(.spin) {
		animation: spin 1s linear;
	}

	@keyframes spin {
		from { transform: rotate(0deg); }
		to { transform: rotate(360deg); }
	}
</style>
