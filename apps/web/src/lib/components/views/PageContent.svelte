<script lang="ts">
	/**
	 * PageContent - Platform-agnostic page editor content
	 *
	 * Displays and edits a user page. Can be used in tabs, modals, or mobile WebViews.
	 * Uses Yjs for real-time collaborative editing with WebSocket sync.
	 */
	import { Page } from "$lib";
	import CoverImagePicker from "$lib/components/CoverImagePicker.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import IconPicker from "$lib/components/IconPicker.svelte";
	import { PageEditor, VersionHistoryPanel } from "$lib/components/pages";
	import { Popover } from "$lib/floating";
	import { pagesStore } from "$lib/stores/pages.svelte";
	import { onMount, onDestroy, untrack } from "svelte";
	import { createYjsDocument, type YjsDocument } from "$lib/yjs";

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

	let {
		pageId = "",
		active = true,
		onLabelChange,
		onIconChange,
		onNavigate,
	}: Props = $props();

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
	let title = $state("");
	let content = $state("");
	let icon = $state<string | null>(null);
	let coverUrl = $state<string | null>(null);
	let showIconPicker = $state(false);
	let showCoverPicker = $state(false);
	let showVersionHistory = $state(false);
	let showDeleteConfirm = $state(false);
	let coverHover = $state(false);
	type WidthMode = "small" | "medium" | "full";
	let widthMode = $state<WidthMode>("medium");
	let showDragHandles = $state(true);
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
	// Unsubscribe handles for Yjs store subscriptions
	let unsubSynced: (() => void) | null = null;
	let unsubConnected: (() => void) | null = null;
	// Track sync/connection state reactively (subscribed from Yjs stores)
	let isSynced = $state(false);
	let isConnected = $state(false);
	// Grace period: don't show "Offline" during initial connection attempt
	let connectionGracePeriod = $state(true);
	let graceTimerRef: ReturnType<typeof setTimeout> | null = null;

	// AbortController for cancelling in-flight loadPage() fetches
	let loadAbortController: AbortController | null = null;

	// Track whether there are unsaved changes (for beforeunload warning)
	const hasUnsavedChanges = $derived(!!saveTimeout || saving || isTyping);
	// Don't mount the editor until Yjs has synced (from IndexedDB or server).
	// This prevents ySyncPlugin from writing an empty paragraph to the XmlFragment
	// before the real content arrives, which corrupts the document.

	// Editor ref for focusing
	let editorContainerEl: HTMLDivElement;

	// Computed stats
	const wordCount = $derived(
		content.trim() ? content.trim().split(/\s+/).length : 0,
	);
	const charCount = $derived(content.length);
	// Count outgoing links: [text](url) patterns, excluding images ![...]()
	const linkCount = $derived(
		(content.match(/(?<!!)\[.+?\]\(.+?\)/g) || []).length,
	);

	// TODO: Fetch actual backlinks from API
	const backlinks = $state(0);

	// Copy state
	let copied = $state(false);

	// Track body content edits for the save status indicator
	// Skip the initial value — only react to changes after mount
	let contentInitialized = false;
	$effect(() => {
		content; // Track content changes
		if (!contentInitialized) {
			contentInitialized = true;
			return;
		}
		// Content changed from user edit — show typing/syncing indicator
		isTyping = true;
		if (typingTimeout) clearTimeout(typingTimeout);
		typingTimeout = setTimeout(() => {
			isTyping = false;
		}, 1000);
	});

	// Reset hasSaved after a delay so the checkmark fades
	$effect(() => {
		if (hasSaved) {
			const timeout = setTimeout(() => {
				hasSaved = false;
			}, 3000);
			return () => clearTimeout(timeout);
		}
	});

	// Reset copied state after a delay
	$effect(() => {
		if (copied) {
			const timeout = setTimeout(() => {
				copied = false;
			}, 2000);
			return () => clearTimeout(timeout);
		}
	});

	// Track the last loaded pageId to avoid reloading the same page
	let lastLoadedPageId = $state<string | null>(null);

	// Load drag handles preference from localStorage
	const DRAG_HANDLES_KEY = "virtues-show-drag-handles";

	// beforeunload: warn user about unsaved changes
	function handleBeforeUnload(e: BeforeUnloadEvent) {
		if (hasUnsavedChanges) {
			e.preventDefault();
		}
	}

	// visibilitychange: flush pending saves when tab is backgrounded
	function handleVisibilityChange() {
		if (document.hidden && saveTimeout) {
			clearTimeout(saveTimeout);
			saveTimeout = null;
			save();
		}
	}

	onMount(async () => {
		// Load drag handles preference
		try {
			const saved = localStorage.getItem(DRAG_HANDLES_KEY);
			if (saved !== null) showDragHandles = JSON.parse(saved);
		} catch {}

		// Register global handlers for content protection
		window.addEventListener("beforeunload", handleBeforeUnload);
		document.addEventListener("visibilitychange", handleVisibilityChange);

		console.log(
			"[PageContent] onMount, pageId:",
			pageId,
			"active:",
			active,
		);
		if (pageId && pageId !== lastLoadedPageId) {
			lastLoadedPageId = pageId;
			await loadPage();
		}
	});

	// Persist drag handles preference
	$effect(() => {
		localStorage.setItem(DRAG_HANDLES_KEY, JSON.stringify(showDragHandles));
	});

	onDestroy(() => {
		// Remove global handlers
		window.removeEventListener("beforeunload", handleBeforeUnload);
		document.removeEventListener(
			"visibilitychange",
			handleVisibilityChange,
		);
		// Cancel in-flight fetch
		loadAbortController?.abort();
		// Clear all pending timers
		if (saveTimeout) clearTimeout(saveTimeout);
		if (typingTimeout) clearTimeout(typingTimeout);
		if (graceTimerRef) clearTimeout(graceTimerRef);
		// Unsubscribe from Yjs stores
		unsubSynced?.();
		unsubConnected?.();
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
					console.log(
						"[PageContent] pageId changed, reloading:",
						currentPageId,
					);
					lastLoadedPageId = currentPageId;
					loadPage();
				}
			});
		}
	});

	async function loadPage() {
		if (!pageId) {
			error = "No page ID provided";
			loading = false;
			return;
		}

		// Cancel any in-flight fetch
		loadAbortController?.abort();
		loadAbortController = new AbortController();

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
			const response = await fetch(`/api/pages/${pageId}`, {
				signal: loadAbortController.signal,
			});
			if (!response.ok) {
				if (response.status === 404) {
					error = "Page not found";
				} else {
					throw new Error("Failed to load page");
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

			// Unsubscribe from previous doc stores
			unsubSynced?.();
			unsubConnected?.();

			// Create Yjs document for real-time sync
			// This connects via WebSocket to /ws/yjs/{pageId}
			yjsDoc = createYjsDocument(pageId);

			// Grace period: suppress "Offline" during initial connection
			connectionGracePeriod = true;
			if (graceTimerRef) clearTimeout(graceTimerRef);
			graceTimerRef = setTimeout(() => {
				connectionGracePeriod = false;
			}, 1500);

			// Subscribe to sync/connection state (store unsubscribe handles)
			unsubSynced = yjsDoc.isSynced.subscribe((synced) => {
				isSynced = synced;
			});
			unsubConnected = yjsDoc.isConnected.subscribe((connected) => {
				isConnected = connected;
				// End grace period early once connected
				if (connected) {
					connectionGracePeriod = false;
					if (graceTimerRef) clearTimeout(graceTimerRef);
				}
			});

			// Content is synced via bind:content from PageEditor, which properly
			// serializes ProseMirror doc to markdown via serializeMarkdown()
		} catch (e) {
			// Ignore aborted fetches (cancelled by a newer loadPage call)
			if (e instanceof DOMException && e.name === "AbortError") return;
			error = e instanceof Error ? e.message : "Failed to load page";
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
		// Update local state for word count and stats
		// Typing indicator is handled by the $effect watching content
		content = newContent;
	}

	// Watch for title changes and sync to stores immediately
	$effect(() => {
		if (pageData && title !== undefined) {
			const currentTitle = title.trim() || "Untitled";

			// Update the tab label at the top
			untrack(() => {
				onLabelChange?.(currentTitle);

				// Update the sidebar tree item locally for instant feedback
				if (pageData) {
					pagesStore.updatePageLocally(pageData.id, {
						title: currentTitle,
					});
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
				cover_url: coverUrl,
			});

			hasSaved = true;

			// Update internal state to match saved title
			if (pageData) {
				pageData.title = title;
			}
		} catch (err) {
			console.error("Failed to save page:", err);
			saveError = true;
		} finally {
			saving = false;
		}
	}

	async function deletePage() {
		if (!pageData) return;

		try {
			// Use store method - handles tab closing, API call, cache invalidation, sidebar refresh
			await pagesStore.removePage(pageData.id);
			// Navigate to pages list
			onNavigate?.("/pages");
		} catch (err) {
			console.error("Failed to delete page:", err);
		} finally {
			showDeleteConfirm = false;
		}
	}

	function handleBackClick() {
		onNavigate?.("/pages");
	}

	function handleTitleKeydown(e: KeyboardEvent) {
		if (e.key === "Enter") {
			e.preventDefault();
			// Focus the actual contenteditable element (not the container)
			const pmEditor = editorContainerEl?.querySelector(
				'[contenteditable="true"]',
			) as HTMLElement;
			pmEditor?.focus();
		}
	}

	async function copyMarkdown() {
		try {
			await navigator.clipboard.writeText(content);
			copied = true;
		} catch (err) {
			console.error("Failed to copy markdown:", err);
		}
	}
</script>

<Page className="p-0">
	{#if loading}
		<div class="flex items-center justify-center h-full">
			<Icon icon="ri:loader-4-line" width="20" class="spin" />
		</div>
	{:else if error}
		<div class="max-w-2xl mx-auto p-4">
			<div
				class="p-4 bg-error-subtle border border-error rounded-lg text-error"
			>
				{error}
			</div>
			<button
				onclick={handleBackClick}
				class="text-primary hover:underline mt-4 inline-block"
			>
				Back to Pages
			</button>
		</div>
	{:else if pageData}
		<div class="page-layout">
			<!-- Top Action Bar - flush to window -->
			<div class="page-toolbar">
				<div class="toolbar-spacer"></div>
				<Popover bind:open={showIconPicker} placement="bottom-start">
					{#snippet trigger({ toggle })}
						<button
							onclick={toggle}
							class="toolbar-action"
							title={icon ? "Change icon" : "Add icon"}
						>
							{#if icon}
								{#if icon.includes(":")}
									<Icon {icon} width="15" />
								{:else}
									<span class="toolbar-emoji">{icon}</span>
								{/if}
							{:else}
								<Icon icon="ri:emotion-line" width="15" />
							{/if}
						</button>
					{/snippet}
					{#snippet children({ close })}
						<IconPicker
							value={icon}
							onSelect={(value) => {
								icon = value;
								if (pageData) {
									pagesStore.updatePageLocally(pageData.id, {
										icon: value,
									});
								}
								onIconChange?.(value);
								save();
							}}
							{close}
						/>
					{/snippet}
				</Popover>
				<Popover bind:open={showCoverPicker} placement="bottom-start">
					{#snippet trigger({ toggle })}
						<button
							onclick={toggle}
							class="toolbar-action"
							title={coverUrl ? "Change cover" : "Add cover"}
						>
							<Icon
								icon={coverUrl
									? "ri:image-edit-line"
									: "ri:image-line"}
								width="15"
							/>
						</button>
					{/snippet}
					{#snippet children({ close })}
						<CoverImagePicker
							value={coverUrl}
							onSelect={(url) => {
								coverUrl = url;
								save();
							}}
							{close}
						/>
					{/snippet}
				</Popover>
				<button
					onclick={() => {
						const modes: WidthMode[] = ["small", "medium", "full"];
						const currentIndex = modes.indexOf(widthMode);
						widthMode = modes[(currentIndex + 1) % modes.length];
					}}
					class="toolbar-action"
					title={widthMode === "small"
						? "Small width"
						: widthMode === "medium"
							? "Medium width"
							: "Full width"}
				>
					<Icon
						icon={widthMode === "small"
							? "ri:contract-left-right-line"
							: widthMode === "medium"
								? "ri:pause-line"
								: "ri:expand-left-right-line"}
						width="15"
					/>
				</button>
				<button
					onclick={() => (showDragHandles = !showDragHandles)}
					class="toolbar-action"
					class:active={showDragHandles}
					title={showDragHandles
						? "Hide line numbers"
						: "Show line numbers"}
				>
					<Icon icon="ri:hashtag" width="15" />
				</button>
				<button
					onclick={copyMarkdown}
					class="toolbar-action"
					class:active={copied}
					title={copied ? "Copied!" : "Copy as Markdown"}
				>
					<Icon
						icon={copied ? "ri:check-line" : "ri:file-copy-line"}
						width="15"
					/>
				</button>
				<Popover bind:open={showVersionHistory} placement="bottom-end">
					{#snippet trigger({ toggle })}
						<button
							onclick={toggle}
							class="toolbar-action"
							title="Version history"
						>
							<Icon icon="ri:history-line" width="15" />
						</button>
					{/snippet}
					{#snippet children({ close })}
						<VersionHistoryPanel {close} {pageId} {yjsDoc} />
					{/snippet}
				</Popover>
				<div class="toolbar-divider"></div>
				<Popover bind:open={showDeleteConfirm} placement="bottom-end">
					{#snippet trigger({ toggle })}
						<button
							onclick={toggle}
							class="toolbar-action toolbar-action-danger"
							title="Delete page"
						>
							<Icon icon="ri:delete-bin-line" width="15" />
						</button>
					{/snippet}
					{#snippet children({ close })}
						<div class="delete-confirm">
							<p class="delete-confirm-text">Delete this page?</p>
							<div class="delete-confirm-actions">
								<button
									class="delete-confirm-btn delete-confirm-cancel"
									onclick={close}
								>
									Cancel
								</button>
								<button
									class="delete-confirm-btn delete-confirm-delete"
									onclick={deletePage}
								>
									Delete
								</button>
							</div>
						</div>
					{/snippet}
				</Popover>
			</div>

			<!-- Main Content Area -->
			<div class="page-content">
				<!-- Cover Image - above title, full bleed -->
				{#if coverUrl}
					<!-- svelte-ignore a11y_no_static_element_interactions -->
					<div
						class="cover-image-wrapper"
						class:width-small={widthMode === "small"}
						class:width-medium={widthMode === "medium"}
						class:width-full={widthMode === "full"}
						onmouseenter={() => (coverHover = true)}
						onmouseleave={() => (coverHover = false)}
					>
						<div
							class="cover-image"
							style="background-image: url({coverUrl})"
						></div>
						{#if coverHover}
							<div class="cover-overlay">
								<button
									class="cover-overlay-btn"
									onclick={() => (showCoverPicker = true)}
								>
									<Icon
										icon="ri:image-edit-line"
										width="14"
									/>
									Change cover
								</button>
								<button
									class="cover-overlay-btn cover-overlay-btn-danger"
									onclick={() => {
										coverUrl = null;
										save();
									}}
								>
									<Icon icon="ri:close-line" width="14" />
									Remove
								</button>
							</div>
						{/if}
					</div>
				{/if}

				<div
					class="page-inner"
					class:width-small={widthMode === "small"}
					class:width-medium={widthMode === "medium"}
					class:width-full={widthMode === "full"}
					class:has-line-numbers={showDragHandles}
				>
					<!-- Header -->
					<div class="page-header">
						{#if icon}
							<Popover placement="bottom-start">
								{#snippet trigger({ toggle })}
									<button
										onclick={toggle}
										class="page-icon-btn"
										title="Change icon"
									>
										{#if icon && icon.includes(":")}
											<Icon {icon} width="28" />
										{:else if icon}
											<span class="page-icon-emoji"
												>{icon}</span
											>
										{/if}
									</button>
								{/snippet}
								{#snippet children({ close })}
									<IconPicker
										value={icon}
										onSelect={(value) => {
											icon = value;
											if (pageData) {
												pagesStore.updatePageLocally(
													pageData.id,
													{ icon: value },
												);
											}
											onIconChange?.(value);
											save();
										}}
										{close}
									/>
								{/snippet}
							</Popover>
						{/if}
						<textarea
							bind:value={title}
							placeholder="Untitled"
							onkeydown={handleTitleKeydown}
							rows="1"
							class="page-title-input"
						></textarea>
					</div>

					<!-- Editor area: overlay pattern to avoid destroying ProseMirror -->
					<div class="page-editor-area" bind:this={editorContainerEl}>
						{#if !yjsDoc || !isSynced}
							<div class="editor-loading">
								<Icon
									icon="ri:loader-4-line"
									width="16"
									class="spin"
								/>
								<span>Loading document...</span>
							</div>
						{/if}
						{#if yjsDoc}
							{#key pageId}
								<div class:editor-hidden={!isSynced}>
									<PageEditor
										bind:content
										onSave={handleContentChange}
										placeholder="Type / for styling and @ for entities"
										{yjsDoc}
										disabled={!isSynced}
										{isConnected}
										{isSynced}
										{showDragHandles}
										{pageId}
									/>
								</div>
							{/key}
						{/if}
					</div>
				</div>
			</div>

			<!-- Bottom Status Bar -->
			<div class="page-status-bar">
				<div class="status-item" title="Outgoing links">
					<Icon icon="ri:links-line" width="12" />
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
				<!-- Unified save status -->
				<div
					class="status-item status-save"
					class:saving={saving || isTyping}
					class:saved={hasSaved ||
						(isConnected &&
							isSynced &&
							!saving &&
							!isTyping &&
							!saveError)}
					class:error={saveError}
					class:offline={!isConnected && !connectionGracePeriod}
				>
					{#if !isConnected && !connectionGracePeriod}
						<Icon icon="ri:wifi-off-line" width="12" />
						<span>Offline</span>
					{:else if saving || isTyping || connectionGracePeriod || !isSynced}
						{#if isTyping}
							<svg
								width="14"
								height="14"
								viewBox="0 0 16 16"
								fill="none"
							>
								<text
									x="1"
									y="12"
									font-family="var(--font-serif)"
									font-size="11"
									font-weight="500"
									fill="currentColor">Aa</text
								>
							</svg>
						{:else}
							<Icon icon="ri:cloud-line" width="12" />
						{/if}
						<span
							>{saving
								? "Saving"
								: isTyping
									? "Typing"
									: "Syncing"}</span
						>
					{:else if saveError}
						<Icon icon="ri:error-warning-line" width="12" />
						<span>Error</span>
					{:else}
						<Icon icon="ri:cloud-line" width="12" />
						<span>Saved</span>
					{/if}
				</div>
			</div>
		</div>
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
		transition: padding-left 0.15s ease;
	}

	/* Match header padding to editor gutter */
	.has-line-numbers .page-header {
		padding-left: 2rem;
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
		/* Offset to align icon with first line of title (title: 2rem * 1.2 line-height = 38.4px, icon: 28px) */
		margin-top: 5px;
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

	/* Hidden editor - keeps ProseMirror DOM attached but invisible during sync */
	.editor-hidden {
		visibility: hidden;
		height: 0;
		overflow: hidden;
		pointer-events: none;
	}

	/* Editor Loading State - shown while Yjs syncs */
	.editor-loading {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 2rem 0;
		color: var(--color-foreground-muted);
		font-size: 13px;
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

	/* Unified save status states */
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

	.status-save.offline {
		color: var(--color-warning);
	}

	/* Spinning animation */
	:global(.spin) {
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		from {
			transform: rotate(0deg);
		}
		to {
			transform: rotate(360deg);
		}
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
		from {
			opacity: 0;
		}
		to {
			opacity: 1;
		}
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

	/* Delete Confirmation Popover */
	.delete-confirm {
		padding: 12px;
		min-width: 180px;
	}

	.delete-confirm-text {
		margin: 0 0 12px 0;
		font-size: 13px;
		font-weight: 500;
		color: var(--color-foreground);
	}

	.delete-confirm-actions {
		display: flex;
		gap: 8px;
		justify-content: flex-end;
	}

	.delete-confirm-btn {
		padding: 6px 12px;
		font-size: 12px;
		font-weight: 500;
		border: none;
		border-radius: 6px;
		cursor: pointer;
		transition: all 0.15s ease;
	}

	.delete-confirm-cancel {
		background: var(--color-surface-elevated);
		color: var(--color-foreground-muted);
	}

	.delete-confirm-cancel:hover {
		background: var(--color-border);
		color: var(--color-foreground);
	}

	.delete-confirm-delete {
		background: var(--color-error);
		color: white;
	}

	.delete-confirm-delete:hover {
		filter: brightness(1.1);
	}
</style>
