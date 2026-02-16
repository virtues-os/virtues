<script lang="ts">
	/**
	 * PageContent - Platform-agnostic page editor content
	 *
	 * Displays and edits a user page. Can be used in tabs, modals, or mobile WebViews.
	 * Uses Yjs for real-time collaborative editing with WebSocket sync.
	 */
	import { Page } from "$lib";
	import Icon from "$lib/components/Icon.svelte";
	import IconPicker from "$lib/components/IconPicker.svelte";
	import CodeMirrorEditor from "$lib/components/pages/CodeMirrorEditor.svelte";
	import type { DocStats } from "$lib/components/pages/CodeMirrorEditor.svelte";
	import PageCoverImage from "$lib/components/pages/PageCoverImage.svelte";
	import PageStatusBar from "$lib/components/pages/PageStatusBar.svelte";
	import PageToolbar from "$lib/components/pages/PageToolbar.svelte";
	import { Popover } from "$lib/floating";
	import { createPageShare, getPageShare, deletePageShare } from "$lib/api/client";
	import { pagesStore } from "$lib/stores/pages.svelte";
	import { createYjsDocument, type YjsDocument } from "$lib/yjs";
	import { saveVersion } from "$lib/yjs/versions";
	import { onDestroy, onMount, untrack } from "svelte";

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
	let showCoverPicker = $state(false);
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

	// Auto-snapshot state: idle timer (5 min) + blur trigger
	const AUTO_SNAPSHOT_IDLE_MS = 5 * 60 * 1000;
	let hasEditsSinceSnapshot = false;
	let autoSnapshotTimeout: ReturnType<typeof setTimeout> | null = null;

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
	// Sync fallback: force-show editor if sync hasn't completed after timeout.
	// Prevents permanent black screen if IndexedDB + WebSocket both stall.
	let syncFallbackRef: ReturnType<typeof setTimeout> | null = null;

	// AbortController for cancelling in-flight loadPage() fetches
	let loadAbortController: AbortController | null = null;

	// Track whether there are unsaved changes (for beforeunload warning)
	const hasUnsavedChanges = $derived(!!saveTimeout || saving || isTyping);
	// Don't mount the editor until Yjs has synced (from IndexedDB or server).
	// This prevents yCollab from rendering an empty document before the real
	// content arrives via Y.Text sync.

	// Editor ref for focusing
	let editorContainerEl: HTMLDivElement;

	// Doc stats from CodeMirror (updated via onDocChange callback)
	let wordCount = $state(0);
	let charCount = $state(0);
	let linkCount = $state(0);

	// TODO: Fetch actual backlinks from API
	const backlinks = $state(0);

	// Copy state
	let copied = $state(false);

	// Share state
	let shareToken = $state<string | null>(null);

	async function autoSnapshot(description: string, keepalive = false) {
		if (!hasEditsSinceSnapshot || !yjsDoc) return;
		hasEditsSinceSnapshot = false;
		await saveVersion(yjsDoc.ydoc, pageId, description, 'auto', { keepalive });
	}

	function resetIdleTimer() {
		if (autoSnapshotTimeout) clearTimeout(autoSnapshotTimeout);
		autoSnapshotTimeout = setTimeout(() => {
			autoSnapshot('Auto-saved (idle)');
		}, AUTO_SNAPSHOT_IDLE_MS);
	}

	function handleDocChange(stats: DocStats) {
		wordCount = stats.wordCount;
		charCount = stats.charCount;
		linkCount = stats.linkCount;

		// Track edits for auto-snapshot deduplication
		hasEditsSinceSnapshot = true;
		resetIdleTimer();

		// Show typing/syncing indicator
		isTyping = true;
		if (typingTimeout) clearTimeout(typingTimeout);
		typingTimeout = setTimeout(() => {
			isTyping = false;
		}, 1000);
	}

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

	// visibilitychange: flush pending saves + auto-snapshot when tab is backgrounded
	function handleVisibilityChange() {
		if (document.hidden) {
			if (saveTimeout) {
				clearTimeout(saveTimeout);
				saveTimeout = null;
				save();
			}
			// Auto-snapshot on blur with keepalive so the request survives tab switch
			autoSnapshot('Auto-saved (background)', true);
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
		if (syncFallbackRef) clearTimeout(syncFallbackRef);
		if (autoSnapshotTimeout) clearTimeout(autoSnapshotTimeout);
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

		// Reset auto-snapshot state for the new page
		if (autoSnapshotTimeout) clearTimeout(autoSnapshotTimeout);
		autoSnapshotTimeout = null;
		hasEditsSinceSnapshot = false;

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

			// Sync fallback: if neither IndexedDB nor WebSocket sync within 4s,
			// force-show the editor so the user never gets a permanent black screen.
			if (syncFallbackRef) clearTimeout(syncFallbackRef);
			syncFallbackRef = setTimeout(() => {
				if (!isSynced) {
					console.warn("[PageContent] Sync timeout — force-showing editor");
					isSynced = true;
				}
			}, 4000);

			// Subscribe to sync/connection state (store unsubscribe handles)
			unsubSynced = yjsDoc.isSynced.subscribe((synced) => {
				isSynced = synced;
				// Clear fallback timer once synced normally
				if (synced && syncFallbackRef) {
					clearTimeout(syncFallbackRef);
					syncFallbackRef = null;
				}
			});
			unsubConnected = yjsDoc.isConnected.subscribe((connected) => {
				isConnected = connected;
				// End grace period early once connected
				if (connected) {
					connectionGracePeriod = false;
					if (graceTimerRef) clearTimeout(graceTimerRef);
				}
			});

			// Content sync happens via Yjs. Doc stats are pushed via onDocChange callback.

			// Fetch share state (non-blocking)
			getPageShare(pageId).then((share) => {
				shareToken = share?.token ?? null;
			}).catch(() => {
				shareToken = null;
			});
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

	// Note: handleContentChange removed — stats are now computed from CodeMirror
	// directly via onDocChange callback. Content sync happens through Yjs.

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
			// Read directly from the local Yjs document (always up-to-date)
			// The API content field can be stale due to the 2s debounced save queue
			const text = yjsDoc?.ytext?.toString() || content;
			await navigator.clipboard.writeText(text);
			copied = true;
		} catch (err) {
			console.error("Failed to copy markdown:", err);
		}
	}

	async function handleShare() {
		try {
			if (shareToken) {
				// Already shared — copy link and offer revoke
				const url = `${window.location.origin}/s/${shareToken}`;
				await navigator.clipboard.writeText(url);
				// Import toast dynamically to avoid adding dependency to this file's top-level
				const { toast } = await import("svelte-sonner");
				toast.success("Share link copied!", {
					description: url,
					action: {
						label: "Revoke",
						onClick: async () => {
							try {
								await deletePageShare(pageId);
								shareToken = null;
								toast.info("Share link revoked");
							} catch (err) {
								console.error("Failed to revoke share:", err);
							}
						},
					},
				});
			} else {
				// Create new share
				const share = await createPageShare(pageId);
				shareToken = share.token;
				const url = `${window.location.origin}/s/${share.token}`;
				await navigator.clipboard.writeText(url);
				const { toast } = await import("svelte-sonner");
				toast.success("Share link copied to clipboard!", {
					description: url,
				});
			}
		} catch (err) {
			console.error("Failed to share page:", err);
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
			<PageToolbar
				{icon}
				{coverUrl}
				{widthMode}
				{copied}
				{pageId}
				{yjsDoc}
				bind:showCoverPicker
				isShared={!!shareToken}
				onShare={handleShare}
				onIconSelect={(value) => {
					icon = value;
					if (pageData) {
						pagesStore.updatePageLocally(pageData.id, { icon: value });
					}
					onIconChange?.(value);
					save();
				}}
				onCoverSelect={(url) => {
					coverUrl = url;
					save();
				}}
				onWidthCycle={() => {
					const modes: WidthMode[] = ["small", "medium", "full"];
					const currentIndex = modes.indexOf(widthMode);
					widthMode = modes[(currentIndex + 1) % modes.length];
				}}
				onCopyMarkdown={copyMarkdown}
				onDelete={deletePage}
			/>

			<!-- Main Content Area -->
			<div class="page-content">
				<!-- Cover Image - above title, full bleed -->
				{#if coverUrl}
					<PageCoverImage
						{coverUrl}
						{widthMode}
						onChangeCover={() => (showCoverPicker = true)}
						onRemoveCover={() => {
							coverUrl = null;
							save();
						}}
					/>
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

					<!-- Editor area: overlay pattern to avoid destroying CodeMirror -->
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
						{#if yjsDoc && isSynced}
							{#key pageId}
								<CodeMirrorEditor
									initialContent={content}
									onDocChange={handleDocChange}
									placeholder="Type / for commands, @ for entities..."
									{yjsDoc}
									{isConnected}
									{isSynced}
									{showDragHandles}
									{pageId}
								/>
							{/key}
						{/if}
					</div>
				</div>
			</div>

			<!-- Bottom Status Bar -->
			<PageStatusBar
				{linkCount}
				{wordCount}
				{charCount}
				{saving}
				{isTyping}
				{hasSaved}
				{isConnected}
				{isSynced}
				{saveError}
				{connectionGracePeriod}
			/>
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
		position: relative;
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

</style>
