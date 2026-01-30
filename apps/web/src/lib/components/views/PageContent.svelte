<script lang="ts">
	/**
	 * PageContent - Platform-agnostic page editor content
	 * 
	 * Displays and edits a user page. Can be used in tabs, modals, or mobile WebViews.
	 * Uses Yjs for real-time collaborative editing with WebSocket sync.
	 */
	import { Page } from '$lib';
	import CoverImagePicker from '$lib/components/CoverImagePicker.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import IconPicker from '$lib/components/IconPicker.svelte';
	import { PageEditor, VersionHistoryPanel } from '$lib/components/pages';
	import { pagesStore } from '$lib/stores/pages.svelte';
	import { onMount, onDestroy, untrack } from 'svelte';
	import { createYjsDocument, type YjsDocument } from '$lib/yjs';

	interface Props {
		/** The page ID to display/edit */
		pageId?: string;
		/** Whether this content is currently active/visible */
		active?: boolean;
		/** Callback when the page title changes (for tab label updates) */
		onLabelChange?: (label: string) => void;
		/** Callback when the page icon changes (for tab icon updates) */
		onIconChange?: (icon: string | null) => void;
		/** Callback when navigating away (e.g., after delete) */
		onNavigate?: (route: string) => void;
	}

	let { pageId = '', active = true, onLabelChange, onIconChange, onNavigate }: Props = $props();

	interface PageData {
		id: string;
		title: string;
		content: string;
		icon: string | null;
		cover_url: string | null;
		tags: string | null;
		created_at: string;
		updated_at: string;
	}

	let pageData = $state<PageData | null>(null);
	let title = $state('');
	let content = $state('');
	let icon = $state<string | null>(null);
	let coverUrl = $state<string | null>(null);
	let showIconPicker = $state(false);
	let showCoverPicker = $state(false);
	let showVersionHistory = $state(false);
	let coverHover = $state(false);
	type WidthMode = 'small' | 'medium' | 'full';
	let widthMode = $state<WidthMode>('medium');
	let loading = $state(false);
	let saving = $state(false);
	let hasSaved = $state(false);
	let saveError = $state(false);
	let error = $state<string | null>(null);
	let saveTimeout: ReturnType<typeof setTimeout> | null = null;

	// Typing state
	let isTyping = $state(false);
	let typingTimeout: ReturnType<typeof setTimeout> | null = null;

	// Yjs document for real-time sync
	let yjsDoc = $state<YjsDocument | undefined>(undefined);
	// Track sync/connection state reactively (subscribed from Yjs stores)
	let isSynced = $state(false);
	let isConnected = $state(false);
	// Derived state: editor is disabled until Yjs is synced
	const editorDisabled = $derived(yjsDoc ? !isSynced : false);

	// Editor ref for focusing
	let editorContainerEl: HTMLDivElement;

	// Computed stats
	const wordCount = $derived(content.trim() ? content.trim().split(/\s+/).length : 0);
	const charCount = $derived(content.length);
	// Count outgoing links: [text](url) patterns, excluding images ![...]()
	const linkCount = $derived((content.match(/(?<!!)\[.+?\]\(.+?\)/g) || []).length);

	// TODO: Fetch actual backlinks from API
	const backlinks = $state(0);

	// Reset hasSaved after a delay so the checkmark fades
	$effect(() => {
		if (hasSaved) {
			const timeout = setTimeout(() => {
				hasSaved = false;
			}, 3000);
			return () => clearTimeout(timeout);
		}
	});

	// Track the last loaded pageId to avoid reloading the same page
	let lastLoadedPageId = $state<string | null>(null);

	onMount(async () => {
		console.log('[PageContent] onMount, pageId:', pageId, 'active:', active);
		if (pageId && pageId !== lastLoadedPageId) {
			lastLoadedPageId = pageId;
			await loadPage();
		}
	});

	onDestroy(() => {
		// Clean up Yjs document on component destroy
		if (yjsDoc) {
			yjsDoc.destroy();
			yjsDoc = undefined;
		}
	});

	// Reload only when pageId actually changes to a new value
	// Use untrack() to prevent infinite loops from state updates
	$effect(() => {
		const currentPageId = pageId;
		const isActive = active;
		
		// Only reload if pageId changed to a different value
		if (currentPageId && isActive) {
			untrack(() => {
				if (currentPageId !== lastLoadedPageId) {
					console.log('[PageContent] pageId changed, reloading:', currentPageId);
					lastLoadedPageId = currentPageId;
					loadPage();
				}
			});
		}
	});

	async function loadPage() {
		if (!pageId) {
			error = 'No page ID provided';
			loading = false;
			return;
		}

		loading = true;
		error = null;

		// Clean up previous Yjs document if switching pages
		if (yjsDoc) {
			yjsDoc.destroy();
			yjsDoc = undefined;
		}
		isSynced = false;
		isConnected = false;

		try {
			const response = await fetch(`/api/pages/${pageId}`);
			if (!response.ok) {
				if (response.status === 404) {
					error = 'Page not found';
				} else {
					throw new Error('Failed to load page');
				}
				return;
			}
			const data: PageData = await response.json();
			pageData = data;
			title = data.title;
			// Content will be synced via Yjs, but we set it for initial display and word count
			content = data.content;
			icon = data.icon;
			coverUrl = data.cover_url;

			// Update label
			onLabelChange?.(data.title);

			// Create Yjs document for real-time sync
			// This connects via WebSocket to /ws/yjs/{pageId}
			yjsDoc = createYjsDocument(pageId);

			// Subscribe to sync/connection state
			yjsDoc.isSynced.subscribe((synced) => {
				isSynced = synced;
			});
			yjsDoc.isConnected.subscribe((connected) => {
				isConnected = connected;
			});

			// Sync content from Yjs when it updates
			yjsDoc.ytext.observe(() => {
				content = yjsDoc?.ytext.toString() ?? '';
			});
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load page';
		} finally {
			loading = false;
		}
	}

	function scheduleSave() {
		if (saveTimeout) clearTimeout(saveTimeout);
		saveTimeout = setTimeout(() => {
			save();
		}, 1000);
	}

	function handleContentChange(newContent: string) {
		// Content changes are handled by Yjs - this is called by PageEditor
		// in non-Yjs mode. Since we always use Yjs now, this is effectively a no-op
		// for content persistence. We just update local state for word count.
		content = newContent;
	}

	// Watch for title changes and sync to stores immediately
	$effect(() => {
		if (pageData && title !== undefined) {
			const currentTitle = title.trim() || 'Untitled';
			
			// Update the tab label at the top
			untrack(() => {
				onLabelChange?.(currentTitle);

				// Update the sidebar tree item locally for instant feedback
				if (pageData) {
					pagesStore.updatePageLocally(pageData.id, { title: currentTitle });
				}
			});

			// Schedule the actual database save
			if (pageData && title !== pageData.title) {
				// Set typing state for title changes too
				isTyping = true;
				hasSaved = false;
				saveError = false;
				if (typingTimeout) clearTimeout(typingTimeout);
				typingTimeout = setTimeout(() => {
					isTyping = false;
				}, 1000);

				scheduleSave();
			}
		}
	});

	async function save() {
		if (!pageData || saving) return;

		// Clear typing state when saving starts
		isTyping = false;
		if (typingTimeout) clearTimeout(typingTimeout);

		saving = true;
		saveError = false;
		hasSaved = false;

		try {
			// Use store method - handles API call, cache invalidation, and sidebar refresh
			// NOTE: Content is NOT sent here - it's synced via Yjs WebSocket
			// The Yjs server handles debounced content persistence
			await pagesStore.savePage(pageData.id, {
				title,
				icon,
				cover_url: coverUrl
			});

			hasSaved = true;

			// Update internal state to match saved title
			if (pageData) {
				pageData.title = title;
			}
		} catch (err) {
			console.error('Failed to save page:', err);
			saveError = true;
		} finally {
			saving = false;
		}
	}

	async function deletePage() {
		if (!pageData) return;
		if (!confirm('Are you sure you want to delete this page?')) return;

		try {
			// Use store method - handles tab closing, API call, cache invalidation, sidebar refresh
			await pagesStore.removePage(pageData.id);
			// Navigate to pages list
			onNavigate?.('/pages');
		} catch (err) {
			console.error('Failed to delete page:', err);
		}
	}

	function handleBackClick() {
		onNavigate?.('/pages');
	}

	function handleTitleKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter') {
			e.preventDefault();
			// Focus the CodeMirror editor inside the container
			const cmEditor = editorContainerEl?.querySelector('.cm-content') as HTMLElement;
			cmEditor?.focus();
		}
	}
</script>

<Page className="p-0">
	{#if loading}
		<div class="text-center py-12 text-foreground-muted">Loading...</div>
	{:else if error}
		<div class="max-w-2xl mx-auto p-4">
			<div class="p-4 bg-error-subtle border border-error rounded-lg text-error">
				{error}
			</div>
			<button onclick={handleBackClick} class="text-primary hover:underline mt-4 inline-block">
				Back to Pages
			</button>
		</div>
	{:else if pageData}
		<div class="page-layout">
			<!-- Top Action Bar - flush to window -->
			<div class="page-toolbar">
				<div class="toolbar-spacer"></div>
				<button
					onclick={() => showIconPicker = true}
					class="toolbar-action"
					title={icon ? "Change icon" : "Add icon"}
				>
					{#if icon}
						{#if icon.includes(':')}
							<Icon icon={icon} width="15"/>
						{:else}
							<span class="toolbar-emoji">{icon}</span>
						{/if}
					{:else}
						<Icon icon="ri:emotion-line" width="15"/>
					{/if}
				</button>
				<button
					onclick={() => showCoverPicker = true}
					class="toolbar-action"
					title={coverUrl ? "Change cover" : "Add cover"}
				>
					<Icon icon={coverUrl ? "ri:image-edit-line" : "ri:image-line"} width="15"/>
				</button>
				<button
					onclick={() => {
						const modes: WidthMode[] = ['small', 'medium', 'full'];
						const currentIndex = modes.indexOf(widthMode);
						widthMode = modes[(currentIndex + 1) % modes.length];
					}}
					class="toolbar-action"
					title={widthMode === 'small' ? 'Small width' : widthMode === 'medium' ? 'Medium width' : 'Full width'}
				>
					<Icon icon={widthMode === 'small' ? 'ri:contract-left-right-line' : widthMode === 'medium' ? 'ri:pause-line' : 'ri:expand-left-right-line'} width="15"/>
				</button>
				<button
					onclick={() => showVersionHistory = true}
					class="toolbar-action"
					title="Version history"
				>
					<Icon icon="ri:history-line" width="15"/>
				</button>
				<div class="toolbar-divider"></div>
				<button
					onclick={deletePage}
					class="toolbar-action toolbar-action-danger"
					title="Delete page"
				>
					<Icon icon="ri:delete-bin-line" width="15"/>
				</button>
			</div>

			<!-- Main Content Area -->
			<div class="page-content">
				<!-- Cover Image - above title, full bleed -->
				{#if coverUrl}
					<!-- svelte-ignore a11y_no_static_element_interactions -->
					<div
						class="cover-image-wrapper"
						class:width-small={widthMode === 'small'}
						class:width-medium={widthMode === 'medium'}
						class:width-full={widthMode === 'full'}
						onmouseenter={() => coverHover = true}
						onmouseleave={() => coverHover = false}
					>
						<div class="cover-image" style="background-image: url({coverUrl})"></div>
						{#if coverHover}
							<div class="cover-overlay">
								<button
									class="cover-overlay-btn"
									onclick={() => showCoverPicker = true}
								>
									<Icon icon="ri:image-edit-line" width="14"/>
									Change cover
								</button>
								<button
									class="cover-overlay-btn cover-overlay-btn-danger"
									onclick={() => { coverUrl = null; scheduleSave(); }}
								>
									<Icon icon="ri:close-line" width="14"/>
									Remove
								</button>
							</div>
						{/if}
					</div>
				{/if}

				<div
					class="page-inner"
					class:width-small={widthMode === 'small'}
					class:width-medium={widthMode === 'medium'}
					class:width-full={widthMode === 'full'}
				>
					<!-- Header -->
					<div class="page-header">
					{#if icon}
						<button
							onclick={() => showIconPicker = true}
							class="page-icon-btn"
							title="Change icon"
						>
							{#if icon.includes(':')}
								<Icon icon={icon} width="28"/>
							{:else}
								<span class="page-icon-emoji">{icon}</span>
							{/if}
						</button>
					{/if}
					<textarea
						bind:value={title}
						placeholder="Untitled"
						onkeydown={handleTitleKeydown}
						rows="1"
						class="page-title-input"
					></textarea>
					</div>

					<!-- Editor -->
					<div class="page-editor-area" bind:this={editorContainerEl}>
						<PageEditor
							bind:content
							onSave={handleContentChange}
							placeholder="Start writing... Use @ to link pages, people, places, and files."
							{yjsDoc}
							disabled={editorDisabled}
							{isConnected}
							{isSynced}
						/>
					</div>
				</div>
			</div>

			<!-- Bottom Status Bar -->
			<div class="page-status-bar">
				<div class="status-item" title="Outgoing links">
					<Icon icon="ri:links-line" width="12"/>
					<span>{linkCount}</span>
				</div>
				<div class="status-divider"></div>
				<div class="status-item" title="Word count">
					<span>{wordCount.toLocaleString()} words</span>
				</div>
				<div class="status-divider"></div>
				<div class="status-item" title="Character count">
					<span>{charCount.toLocaleString()} chars</span>
				</div>
				<div class="status-spacer"></div>
				<!-- Yjs sync status -->
				{#if yjsDoc}
					<div
						class="status-item status-sync"
						class:connected={isConnected && isSynced}
						class:syncing={isConnected && !isSynced}
						class:offline={!isConnected}
					>
						{#if !isConnected}
							<Icon icon="ri:wifi-off-line" width="12"/>
							<span>Offline</span>
						{:else if !isSynced}
							<Icon icon="ri:loader-4-line" width="12" class="spin"/>
							<span>Syncing</span>
						{:else}
							<Icon icon="ri:cloud-line" width="12"/>
							<span>Synced</span>
						{/if}
					</div>
					<div class="status-divider"></div>
				{/if}
				<!-- Metadata save status -->
				<div
					class="status-item status-save"
					class:saving
					class:saved={hasSaved}
					class:error={saveError}
				>
					{#if saving}
						<Icon icon="ri:loader-4-line" width="12" class="spin"/>
						<span>Saving</span>
					{:else if saveError}
						<Icon icon="ri:error-warning-line" width="12"/>
						<span>Error</span>
					{:else if hasSaved}
						<Icon icon="ri:check-line" width="12"/>
						<span>Saved</span>
					{:else}
						<Icon icon="ri:circle-fill" width="6"/>
						<span>Ready</span>
					{/if}
				</div>
			</div>
		</div>

		<!-- Icon Picker Modal -->
		{#if showIconPicker}
			<IconPicker
				value={icon}
				onSelect={(value) => {
					icon = value;
					// Update registry for immediate feedback
					if (pageData) {
						pagesStore.updatePageLocally(pageData.id, { icon: value });
					}
					// Notify parent to update tab bar icon
					onIconChange?.(value);
					// Schedule save - pagesStore.savePage() handles cache invalidation + sidebar refresh
					scheduleSave();
				}}
				onClose={() => showIconPicker = false}
			/>
		{/if}

		<!-- Cover Image Picker Modal -->
		{#if showCoverPicker}
			<CoverImagePicker
				value={coverUrl}
				onSelect={(url) => {
					coverUrl = url;
					scheduleSave();
				}}
				onClose={() => showCoverPicker = false}
			/>
		{/if}

		<!-- Version History Modal -->
		<VersionHistoryPanel
			open={showVersionHistory}
			onClose={() => showVersionHistory = false}
			pageId={pageId}
			{yjsDoc}
		/>
	{/if}
</Page>

<style>
	/* Page Layout - flex column to pin status bar at bottom */
	.page-layout {
		display: flex;
		flex-direction: column;
		height: 100%;
		min-height: 0;
	}

	/* Top Toolbar - flush to top, minimal */
	.page-toolbar {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 4px 12px;
		background: var(--color-background);
		border-bottom: 1px solid var(--color-border);
		flex-shrink: 0;
	}

	.toolbar-spacer {
		flex: 1;
	}

	.toolbar-action {
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 6px;
		border: none;
		background: none;
		color: var(--color-foreground-muted);
		cursor: pointer;
		border-radius: 4px;
		transition: all 0.15s ease;
	}

	.toolbar-action:hover {
		color: var(--color-foreground);
		background: var(--color-surface-elevated);
	}

	.toolbar-action.active {
		color: var(--color-primary);
	}

	.toolbar-action-danger:hover {
		color: var(--color-error);
	}

	.toolbar-emoji {
		font-size: 14px;
		line-height: 1;
	}

	.toolbar-divider {
		width: 1px;
		height: 16px;
		background: var(--color-border);
		margin: 0 2px;
	}

	/* Main Content Area - scrollable */
	.page-content {
		flex: 1;
		overflow-y: auto;
		padding: 2rem 1.5rem;
		padding-bottom: 4rem;
	}

	.page-inner {
		margin: 0 auto;
		transition: max-width 0.2s ease-out;
	}

	.page-inner.width-small {
		max-width: 32rem; /* ~512px */
	}

	.page-inner.width-medium {
		max-width: 42rem; /* ~672px, similar to max-w-2xl */
	}

	.page-inner.width-full {
		max-width: 100%;
	}

	/* Page Header */
	.page-header {
		display: flex;
		align-items: flex-start;
		gap: 12px;
		margin-bottom: 1.5rem;
	}

	.page-title-input {
		flex: 1;
		font-family: var(--font-serif, Georgia, serif);
		font-size: 2rem;
		font-weight: 500;
		line-height: 1.2;
		color: var(--color-foreground);
		background: transparent;
		border: none;
		outline: none;
		padding: 0;
		resize: none;
		field-sizing: content;
	}

	.page-title-input::placeholder {
		color: var(--color-foreground-subtle);
	}

	/* Page Icon Button */
	.page-icon-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 0;
		border: none;
		background: none;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: all 150ms;
		flex-shrink: 0;
	}

	.page-icon-btn:hover {
		color: var(--color-foreground);
	}

	.page-icon-emoji {
		font-size: 1.75rem;
		line-height: 1;
	}

	/* Editor Area */
	.page-editor-area {
		min-height: 300px;
	}

	/* Bottom Status Bar */
	.page-status-bar {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 6px 16px;
		background: var(--color-background);
		border-top: 1px solid var(--color-border);
		font-size: 11px;
		color: var(--color-foreground-muted);
		flex-shrink: 0;
	}

	.status-item {
		display: flex;
		align-items: center;
		gap: 4px;
	}

	.status-divider {
		width: 1px;
		height: 10px;
		background: var(--color-border);
	}

	.status-spacer {
		flex: 1;
	}

	/* Sync status states (Yjs connection) */
	.status-sync {
		transition: color 0.2s ease;
	}

	.status-sync.connected {
		color: var(--color-success);
	}

	.status-sync.syncing {
		color: var(--color-primary);
	}

	.status-sync.offline {
		color: var(--color-warning);
	}

	/* Save status states (metadata) */
	.status-save {
		transition: color 0.2s ease;
	}

	.status-save.saving {
		color: var(--color-foreground-muted);
	}

	.status-save.saved {
		color: var(--color-success);
	}

	.status-save.error {
		color: var(--color-error);
	}

	/* Spinning animation */
	:global(.spin) {
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		from { transform: rotate(0deg); }
		to { transform: rotate(360deg); }
	}

	/* Cover Image - matches .page-inner width (small / medium / full) */
	.cover-image-wrapper {
		position: relative;
		margin-left: auto;
		margin-right: auto;
		margin-bottom: 1.5rem;
		border-radius: 0.5rem;
		overflow: hidden;
		transition: max-width 0.2s ease-out;
	}

	.cover-image-wrapper.width-small {
		max-width: 32rem;
	}

	.cover-image-wrapper.width-medium {
		max-width: 42rem;
	}

	.cover-image-wrapper.width-full {
		max-width: 100%;
	}

	.cover-image {
		width: 100%;
		aspect-ratio: 3 / 1;
		background-size: cover;
		background-position: center;
	}

	.cover-overlay {
		position: absolute;
		bottom: 0;
		right: 0;
		display: flex;
		gap: 4px;
		padding: 8px;
		animation: cover-fade-in 100ms ease-out;
	}

	@keyframes cover-fade-in {
		from { opacity: 0; }
		to { opacity: 1; }
	}

	.cover-overlay-btn {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 10px;
		font-size: 12px;
		font-weight: 500;
		color: white;
		background: rgba(0, 0, 0, 0.6);
		backdrop-filter: blur(4px);
		border: none;
		border-radius: 6px;
		cursor: pointer;
		transition: background 150ms;
	}

	.cover-overlay-btn:hover {
		background: rgba(0, 0, 0, 0.8);
	}

	.cover-overlay-btn-danger:hover {
		background: rgba(220, 38, 38, 0.8);
	}
</style>
