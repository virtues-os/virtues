<script lang="ts">
	/**
	 * PageContent - Platform-agnostic page editor content
	 *
	 * Displays and edits a user page. Can be used in tabs, modals, or mobile WebViews.
	 * Uses Yjs for real-time collaborative editing with WebSocket sync.
	 *
	 * The page's format picks the editor (`pageDocument.ts`): a block page
	 * opens in `DocumentEditor` on its Yjs tree, a markdown page in
	 * `CodeMirrorEditor` on its Y.Text. A block page's history, Copy markdown
	 * and View as markdown go through the server, which holds its tree.
	 */
	import { Page } from "$lib";
	import Icon from "$lib/components/Icon.svelte";
	import Button from "$lib/components/Button.svelte";
	import IconPicker from "$lib/components/IconPicker.svelte";
	import CodeMirrorEditor from "$lib/components/pages/CodeMirrorEditor.svelte";
	import DocumentEditor from "$lib/components/pages/DocumentEditor.svelte";
	import MarkdownView from "$lib/components/pages/MarkdownView.svelte";
	import Modal from "$lib/components/Modal.svelte";
	import type { DocStats } from "$lib/components/pages/stats";
	import type { OutlineNav } from "$lib/components/pages/outline";
	import PageCoverImage from "$lib/components/pages/PageCoverImage.svelte";
	import PageStatusBar from "$lib/components/pages/PageStatusBar.svelte";
	import PageToolbar from "$lib/components/pages/PageToolbar.svelte";
	import PageProjects from "$lib/components/pages/PageProjects.svelte";
	import PageOutline from "$lib/components/pages/PageOutline.svelte";
	import ReferencesPanel from "$lib/components/pages/ReferencesPanel.svelte";
	import { cmOutlineNav, type PageHeading } from "$lib/codemirror/outline";
	import type { EditorView } from "@codemirror/view";
	import { Popover } from "$lib/floating";
	import {
		getPageBacklinks,
		getPageMarkdown,
		listPublications,
		request,
		ApiError,
		type Backlink,
		type PageFormat,
	} from "$lib/api/client";
	import { toast } from "svelte-sonner";
	import ShareSheet from "$lib/components/applets/ShareSheet.svelte";
	import { pagesStore } from "$lib/stores/pages.svelte";
	import { untitled } from "$lib/refs/identity.svelte";
	import { pageDisplay } from "$lib/stores/pageDisplay.svelte";
	import type { TreeDocument, YjsDocument } from "$lib/yjs";
	import { cutServerVersion, saveVersion } from "$lib/yjs/versions";
	import { canPrint, printAlone, printThis } from "$lib/document/print";
	import { treeAiDriver } from "$lib/ai/treeAiSession";
	import {
		mayShowKeptCopy,
		openPageDocument,
		stillLoading,
		type OpenedPage,
	} from "./pageDocument";
	import { onDestroy, onMount, tick, untrack } from "svelte";

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
		icon_color: string | null;
		cover_url: string | null;
		tags: string | null;
		created_at: string;
		updated_at: string;
		/**
		 * The document contract the page is written under; null when the
		 * server could not read it. A box from before the contract does not
		 * send it, and every page on such a box is markdown.
		 */
		contract?: number | null;
		/** How the page holds its text; absent from a box from before block pages. */
		format?: PageFormat;
		/** The newest contract the server reads; absent from a box from before block pages. */
		box_contract?: number;
	}

	let pageData = $state<PageData | null>(null);
	let title = $state("");
	let content = $state("");
	let icon = $state<string | null>(null);
	/** `--cat-*` token key for the icon, or null. Migration 0079. */
	let iconColor = $state<string | null>(null);
	let coverUrl = $state<string | null>(null);
	let showCoverPicker = $state(false);
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

	// Yjs document for real-time sync: a markdown page's Y.Text, or a block
	// page's tree. One of the two at a time; `opened` holds whichever it is.
	let yjsDoc = $state<YjsDocument | undefined>(undefined);
	// Raw: the bindings hold live Yjs objects, never to be proxied.
	let treeDoc = $state.raw<TreeDocument | undefined>(undefined);
	let opened = $state.raw<OpenedPage | null>(null);
	let format = $state<PageFormat>("markdown");
	// Unsubscribe handles for Yjs store subscriptions
	let unsubSynced: (() => void) | null = null;
	let unsubConnected: (() => void) | null = null;
	let unsubRefused: (() => void) | null = null;
	// Why the server will not sync this page with this editor, once it says so.
	let refusedReason = $state<string | null>(null);
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
	let editorContainerEl = $state<HTMLDivElement>();

	// Doc stats from CodeMirror (updated via onDocChange callback)
	let wordCount = $state(0);
	let charCount = $state(0);
	let linkCount = $state(0);

	// References (backlinks) — pages that link to this one. Loaded lazily; the
	// panel is a quiet, summonable right-hand rail, not always-on chrome.
	let showReferences = $state(false);
	let outline = $state<PageHeading[]>([]);
	let editorView = $state<EditorView | null>(null);
	/** The block editor's outline navigation, while one is mounted. */
	let treeNav = $state<OutlineNav | null>(null);
	/** View as markdown (block pages). */
	let viewingMarkdown = $state(false);
	/** The scroller holding the cover, title and text: what Print prints. */
	let printRootEl = $state<HTMLDivElement>();
	let backlinks = $state<Backlink[]>([]);
	let backlinksLoading = $state(false);
	let backlinksLoadedFor = $state<string | null>(null);

	async function loadBacklinks() {
		if (!pageId) return;
		// Avoid refetching the same page's backlinks on repeat toggles.
		if (backlinksLoadedFor === pageId) return;
		backlinksLoading = true;
		try {
			const refs = await getPageBacklinks(pageId);
			// Guard against a page switch mid-flight.
			if (pageId === lastLoadedPageId) {
				backlinks = refs;
				backlinksLoadedFor = pageId;
			}
		} catch (e) {
			console.error("Failed to load backlinks:", e);
		} finally {
			backlinksLoading = false;
		}
	}

	function toggleReferences() {
		showReferences = !showReferences;
		if (showReferences) loadBacklinks();
	}

	function openReference(refPageId: string, title: string) {
		onNavigate?.(`/page/${refPageId}`);
	}

	// Copy state
	let copied = $state(false);

	// Share: the Share modal (api::publications), and whether a link to this
	// page is working right now, which the toolbar shows.
	let sharing = $state(false);
	let isShared = $state(false);

	async function refreshShared() {
		try {
			const links = await listPublications();
			isShared = links.some(
				(l) =>
					l.producer_kind === 'page' &&
					l.producer_id === pageId &&
					!l.revoked_at &&
					(!l.expires_at || new Date(l.expires_at).getTime() > Date.now())
			);
		} catch {
			// Not knowing reads as "not shared"; the toolbar only marks it.
			isShared = false;
		}
	}

	async function autoSnapshot(description: string, keepalive = false) {
		if (!hasEditsSinceSnapshot) return;
		if (treeDoc) {
			// The server reads its own copy of the tree, which every edit reaches first.
			hasEditsSinceSnapshot = false;
			await cutServerVersion(pageId, description, 'auto', { keepalive });
			return;
		}
		if (!yjsDoc) return;
		hasEditsSinceSnapshot = false;
		await saveVersion(yjsDoc.ydoc, pageId, description, 'auto', { keepalive });
	}

	function resetIdleTimer() {
		if (autoSnapshotTimeout) clearTimeout(autoSnapshotTimeout);
		autoSnapshotTimeout = setTimeout(() => {
			autoSnapshot('Auto-saved (idle)');
		}, AUTO_SNAPSHOT_IDLE_MS);
	}

	/**
	 * The editor's counts changed. `local` (the block editor says) is whether
	 * any of the change was typed here: only that is this device's edit to keep
	 * a version of, or to show as typing. CodeMirror reports its own edits.
	 */
	function handleDocChange(stats: DocStats, local = true) {
		wordCount = stats.wordCount;
		charCount = stats.charCount;
		linkCount = stats.linkCount;
		if (!local) return;

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

	// Flush a pending debounced save immediately (e.g. on unmount/unload).
	// Without this, closing or switching the in-app tab within the 1s debounce
	// window drops the title change and the recents list shows the stale title.
	function flushSave() {
		if (saveTimeout) {
			clearTimeout(saveTimeout);
			saveTimeout = null;
			save();
		}
	}

	// beforeunload: warn user about unsaved changes
	function handleBeforeUnload(e: BeforeUnloadEvent) {
		// Flush any pending title save before the page goes away.
		flushSave();
		if (hasUnsavedChanges) {
			e.preventDefault();
		}
	}

	// visibilitychange: flush pending saves + auto-snapshot when tab is backgrounded
	function handleVisibilityChange() {
		if (document.hidden) {
			flushSave();
			// Auto-snapshot on blur with keepalive so the request survives tab switch
			autoSnapshot('Auto-saved (background)', true);
		}
	}

	onMount(async () => {
		// Register global handlers for content protection
		window.addEventListener("beforeunload", handleBeforeUnload);
		document.addEventListener("visibilitychange", handleVisibilityChange);

		if (pageId && pageId !== lastLoadedPageId) {
			lastLoadedPageId = pageId;
			await loadPage();
		}
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
		// Flush any pending title save before tearing down (SPA tab switch/close).
		// The app stays alive, so the fire-and-forget save() completes.
		flushSave();
		// Clear all pending timers
		if (saveTimeout) clearTimeout(saveTimeout);
		if (typingTimeout) clearTimeout(typingTimeout);
		if (graceTimerRef) clearTimeout(graceTimerRef);
		if (syncFallbackRef) clearTimeout(syncFallbackRef);
		if (autoSnapshotTimeout) clearTimeout(autoSnapshotTimeout);
		// Unsubscribe from Yjs stores
		unsubSynced?.();
		unsubConnected?.();
		unsubRefused?.();
		// Clean up Yjs document on component destroy
		closeDocument();
	});

	/**
	 * Let go of the page's document. The editor bound to it unmounts first:
	 * the document is destroyed a tick later, never under a live editor.
	 */
	function closeDocument() {
		const old = opened;
		opened = null;
		yjsDoc = undefined;
		treeDoc = undefined;
		treeNav = null;
		viewingMarkdown = false;
		if (old) void tick().then(() => old.doc.destroy());
	}

	// Reload only when pageId actually changes to a new value
	// Use untrack() to prevent infinite loops from state updates
	$effect(() => {
		const currentPageId = pageId;
		const isActive = active;

		// Only reload if pageId changed to a different value
		if (currentPageId && isActive) {
			untrack(() => {
				if (currentPageId !== lastLoadedPageId) {
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
		closeDocument();
		isSynced = false;
		isConnected = false;
		refusedReason = null;

		// Reset references for the new page (refetched on next panel open)
		backlinks = [];
		backlinksLoadedFor = null;
		if (showReferences) loadBacklinks();

		try {
			// Uses the underlying typed `request` (not the getPage wrapper) so we
			// can carry the abort signal that cancels a stale load on page switch.
			const data = await request<PageData>(`/pages/${encodeURIComponent(pageId)}`, {
				signal: loadAbortController.signal,
			});
			pageData = data;
			title = data.title;
			// Content will be synced via Yjs, but we set it for initial display and word count
			content = data.content;
			icon = data.icon;
			iconColor = data.icon_color ?? null;
			coverUrl = data.cover_url;

			// Update label
			onLabelChange?.(data.title);

			// Unsubscribe from previous doc stores
			unsubSynced?.();
			unsubConnected?.();
			unsubRefused?.();

			// Create Yjs document for real-time sync, in the editor the page's
			// format calls for. This connects via WebSocket to /ws/yjs/{pageId}
			const page = openPageDocument(pageId, data);
			opened = page;
			format = page.format;
			if (page.format === "tree") treeDoc = page.doc;
			else yjsDoc = page.doc;
			const bound = page.doc;

			// Grace period: suppress "Offline" during initial connection
			connectionGracePeriod = true;
			if (graceTimerRef) clearTimeout(graceTimerRef);
			graceTimerRef = setTimeout(() => {
				connectionGracePeriod = false;
			}, 1500);

			// Sync fallback: if neither IndexedDB nor WebSocket sync within 4s,
			// force-show the editor so the user never gets a permanent black screen,
			// on a page the server said this editor reads: the local copy of any
			// other may be from before the page was rewritten as a tree. A block
			// page's copy must also hold a block: an editor bound to an empty
			// tree writes its empty paragraph into the shared document.
			if (syncFallbackRef) clearTimeout(syncFallbackRef);
			syncFallbackRef = setTimeout(() => {
				if (!isSynced && opened === page && mayShowKeptCopy(page, refusedReason)) {
					console.warn("[PageContent] Sync timeout — force-showing editor");
					isSynced = true;
				}
			}, 4000);

			// Subscribe to sync/connection state (store unsubscribe handles)
			unsubSynced = bound.isSynced.subscribe((synced) => {
				isSynced = synced;
				// Clear fallback timer once synced normally
				if (synced && syncFallbackRef) {
					clearTimeout(syncFallbackRef);
					syncFallbackRef = null;
				}
			});
			unsubRefused = bound.refused.subscribe((reason) => {
				refusedReason = reason;
			});
			unsubConnected = bound.isConnected.subscribe((connected) => {
				isConnected = connected;
				// End grace period early once connected
				if (connected) {
					connectionGracePeriod = false;
					if (graceTimerRef) clearTimeout(graceTimerRef);
				}
			});

			// Content sync happens via Yjs. Doc stats are pushed via onDocChange callback.

			void refreshShared();
		} catch (e) {
			// Ignore aborted fetches (cancelled by a newer loadPage call)
			if (e instanceof DOMException && e.name === "AbortError") return;
			if (e instanceof ApiError && e.status === 404) {
				error = "Page not found";
				return;
			}
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
			const currentTitle = title.trim() || untitled("page");

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

	// A rename made elsewhere — the sidebar, a pin, a tab — reaches the open
	// page. Without this the heading kept the old title, and the next save
	// (an icon, a cover) sent it back and undid the rename. An unsaved edit
	// in the heading itself wins: the person is typing there.
	$effect(() => {
		const id = pageData?.id;
		if (!id) return;
		const stored = pagesStore.pages.find((p) => p.id === id)?.title;
		if (stored === undefined) return;
		untrack(() => {
			if (!pageData) return;
			if (stored === (title.trim() || untitled("page"))) return;
			if (title !== pageData.title) return;
			pageData.title = stored;
			title = stored;
		});
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
			// Never the API's `content`, which can trail the page by the 2s
			// debounced save. A block page's export is the server's live copy;
			// a markdown page's text is the local Yjs document.
			const text =
				format === "tree"
					? (await getPageMarkdown(pageId)).markdown
					: yjsDoc?.ytext?.toString() || content;
			await navigator.clipboard.writeText(text);
			copied = true;
		} catch (err) {
			console.error("Failed to copy markdown:", err);
			if (format === "tree") {
				toast.error("Your server couldn't give this page as markdown. Try again.");
			}
		}
	}

	/**
	 * Print the page alone (`lib/document/print.ts`): the marks that set the
	 * rest of the app aside go on at `beforeprint`, which the browser's own
	 * Print fires too, so ⌘P and File > Print print the page as this does.
	 */
	async function printPage() {
		// Menus closing first, so none prints over the page.
		await tick();
		if (printRootEl) printThis(printRootEl);
		else window.print();
	}

	// A block page prints alone however it is printed.
	$effect(() => {
		if (!treeDoc) return;
		return printAlone(() => printRootEl ?? null);
	});
</script>

<Page padding="none" scrollable={false}>
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
			<div class="mt-4">
				<Button variant="secondary" onclick={handleBackClick}>Back to Pages</Button>
			</div>
		</div>
	{:else if pageData}
		<div class="page-layout">
			<!-- Top bar: TOC (left) + page actions (right), one classic row -->
			<div class="page-topbar" data-print="hide">
			<PageOutline
				headings={outline}
				nav={treeDoc ? treeNav : editorView ? cmOutlineNav(editorView) : null}
			/>
			<!-- The projects this page is in, beside the outline: the page's
			     "where it lives", in the corner a chat says it. -->
			{#if pageId}
				<PageProjects url={`/page/${pageId}`} />
			{/if}
			<PageToolbar
				{icon}
				{coverUrl}
				{copied}
				{pageId}
				{format}
				{yjsDoc}
				bind:showCoverPicker
				{isShared}
				referencesActive={showReferences}
				onToggleReferences={toggleReferences}
				onShare={() => (sharing = true)}
				onIconColorSelect={(value) => {
				iconColor = value;
				if (pageData) {
					void pagesStore.savePage(pageData.id, { icon_color: value });
				}
			}}
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
				onCopyMarkdown={copyMarkdown}
				onViewMarkdown={format === "tree" ? () => (viewingMarkdown = true) : undefined}
				onPrint={format === "tree" && canPrint ? printPage : undefined}
				onDelete={deletePage}
			/>
			</div>

			<!-- Body: scrollable editor + optional References rail -->
			<div class="page-body">
			<!-- Main Content Area -->
			<div
				class="page-content"
				bind:this={printRootEl}
				style:--editor-font-family={pageDisplay.fontFamily}
				style:--editor-font-size={pageDisplay.fontSize}
				style:--editor-line-height={pageDisplay.lineHeight}
			>
				<!-- Cover Image - above title, full bleed -->
				{#if coverUrl}
					<PageCoverImage
						{coverUrl}
						widthMode={pageDisplay.widthMode}
						onChangeCover={() => (showCoverPicker = true)}
						onRemoveCover={() => {
							coverUrl = null;
							save();
						}}
					/>
				{/if}

				<div
					class="page-inner"
					class:width-small={pageDisplay.widthMode === "small"}
					class:width-medium={pageDisplay.widthMode === "medium"}
					class:width-full={pageDisplay.widthMode === "full"}
					class:width-page={pageDisplay.widthMode === "page"}
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
										color={iconColor}
										onColorSelect={(value) => {
											// Saved on its own rather than through
											// `save()`: that path sends title, icon
											// and cover together, and picking a
											// color shouldn't drag a half-typed
											// title to the server with it.
											iconColor = value;
											if (pageData) {
												void pagesStore.savePage(pageData.id, {
													icon_color: value,
												});
											}
										}}
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

					<!-- Editor area: overlay pattern to avoid destroying the editor -->
					<div class="page-editor-area" bind:this={editorContainerEl}>
						{#if refusedReason}
							<p class="editor-refused" role="alert" data-print="hide">{refusedReason}</p>
						{/if}
						{#if !opened || stillLoading(opened, isSynced, refusedReason)}
							<div class="editor-loading" data-print="hide">
								<Icon
									icon="ri:loader-4-line"
									width="16"
									class="spin"
								/>
								<span>Loading document...</span>
							</div>
						{/if}
						{#if treeDoc && isSynced}
							{#key pageId}
								<DocumentEditor
									doc={treeDoc}
									{pageId}
									pageTitle={title}
									editable={!treeDoc.readOnly}
									ai={treeAiDriver}
									placeholder="Start writing, or press / for commands…"
									onDocChange={handleDocChange}
									onOutline={(h) => (outline = h)}
									onOutlineNav={(nav) => (treeNav = nav)}
								/>
							{/key}
						{:else if yjsDoc && isSynced}
							{#key pageId}
								<CodeMirrorEditor
									initialContent={content}
									onDocChange={handleDocChange}
									onOutline={(h) => (outline = h)}
									onViewReady={(v) => (editorView = v)}
									placeholder="Start writing, or press / for commands…"
									{yjsDoc}
									{isConnected}
									{isSynced}
									{pageId}
									pageTitle={title}
								/>
							{/key}
						{/if}
					</div>
				</div>
			</div>

			{#if showReferences}
				<ReferencesPanel
					{backlinks}
					loading={backlinksLoading}
					onOpen={openReference}
					onClose={() => (showReferences = false)}
				/>
			{/if}
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
	{#if pageId && treeDoc}
		<Modal open={viewingMarkdown} onClose={() => (viewingMarkdown = false)} title="Markdown" width="lg">
			<MarkdownView {pageId} ydoc={treeDoc.ydoc} />
		</Modal>
	{/if}
	{#if pageId}
		<ShareSheet
			open={sharing}
			producer={{ kind: "page", id: pageId }}
			onClose={() => {
				sharing = false;
				void refreshShared();
			}}
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

	/* One classic top bar: TOC trigger (left) + page actions (right). Relative so
	   the outline drawer anchors to it and spans the full width. */
	.page-topbar {
		position: relative;
		display: flex;
		align-items: center;
		z-index: var(--z-sticky);
	}
	.page-topbar :global(.page-toolbar) {
		flex: 1;
	}

	/* Body - editor + optional references rail, side by side */
	.page-body {
		flex: 1;
		display: flex;
		min-height: 0;
		overflow: hidden;
	}

	/* Main Content Area - scrollable */
	.page-content {
		flex: 1;
		min-width: 0;
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

	/* The A4 sheet, title and all: 210 mm wide with 20 mm margins, as it
	   prints. The block editor's own sheet (`.doc-page-view`) folds into it,
	   so the title is not left outside the page it heads. */
	.page-inner.width-page {
		box-sizing: border-box;
		max-width: 210mm;
		min-height: 297mm;
		padding: 20mm;
		border: 1px solid var(--color-border-subtle, var(--color-border));
		background: var(--color-background);
	}

	.page-inner.width-page :global(.doc-page-view) {
		width: auto;
		min-height: 0;
		padding: 0;
		border: none;
	}

	@media (max-width: 230mm) {
		.page-inner.width-page {
			padding: 16px;
			border: none;
			min-height: 0;
		}
	}

	/* Page Header — title hugs the body so it reads as one document
	   (the editor's own .cm-content adds ~8px on top of this). */
	.page-header {
		display: flex;
		align-items: flex-start;
		gap: 12px;
		margin-bottom: 0.5rem;
	}

	/* 400, and it must stay equal to the shared copy's title (normal weight,
	   in api/publish_page.rs) or the same page reads differently to its author
	   and its reader.
	   The 500 was never drawn either way: the serif ships one cut and the request
	   resolves back to the regular in silence (agents/build/typography.md). At
	   32px in full ink above the body the title already leads the document. */
	.page-title-input {
		flex: 1;
		font-family: var(--font-serif, Georgia, serif);
		font-size: 2rem;
		font-weight: 400;
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

	.editor-refused {
		margin: 0;
		padding: 1rem 0;
		color: var(--color-foreground-muted);
		font-size: 13px;
	}

	/* Print (block pages, `lib/document/print.ts`): the page alone. What is not the page's
	   scroller or a box around it is set aside, and each box around it lets
	   it run onto as many sheets as it needs instead of clipping it to one. */
	@media print {
		[data-print="hide"],
		:global([data-print-off]) {
			display: none !important;
		}

		:global([data-print-chain]),
		.page-content:global([data-print-root]) {
			display: block !important;
			position: static !important;
			overflow: visible !important;
			height: auto !important;
			min-height: 0 !important;
			max-height: none !important;
			width: auto !important;
			max-width: none !important;
			margin: 0 !important;
			padding: 0 !important;
			border: none !important;
			background: none !important;
			transform: none !important;
		}

		.page-title-input {
			color: black;
		}

		/* Paper has its own margins (`@page`, document.css). */
		.page-inner.width-page {
			max-width: none;
			min-height: 0;
			padding: 0;
			border: none;
		}
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
