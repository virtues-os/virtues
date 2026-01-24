<script lang="ts">
	import { onMount } from "svelte";
	import { workspaceStore } from "$lib/stores/workspace.svelte";
	import { bookmarks } from "$lib/stores/bookmarks.svelte";
	import WorkspaceHeader from "./WorkspaceHeader.svelte";
	import ExplorerNode from "./ExplorerNode.svelte";
	import SidebarFooter from "./SidebarFooter.svelte";
	import SearchModal from "./SearchModal.svelte";
	import BookmarksModal from "$lib/components/BookmarksModal.svelte";
	import emblaCarouselSvelte from "embla-carousel-svelte";
	import type { EmblaCarouselType } from "embla-carousel";
	import WheelGesturesPlugin from "embla-carousel-wheel-gestures";

	const STORAGE_KEY = "virtues-sidebar-collapsed";

	// Collapsed state (icon-only mode)
	let isCollapsed = $state(false);

	// Search modal state
	let isSearchOpen = $state(false);

	// Bookmarks modal state
	let isBookmarksOpen = $state(false);

	// New workspace modal state
	let showNewWorkspaceModal = $state(false);
	let newWorkspaceName = $state("");

	// Track if store is ready
	let storeReady = $state(false);

	// Embla carousel API
	let emblaApi: EmblaCarouselType | undefined;

	// Scroll progress for title animation (-1 to 1 relative to current snap)
	let scrollProgress = $state(0);

	// Embla options
	const emblaOptions = {
		loop: false,
		dragFree: false,
		containScroll: 'keepSnaps' as const, // No rubberband past edges
		skipSnaps: false,
		duration: 12,
		watchDrag: true,
	};

	// Workspace info type for header transitions
	type WorkspaceInfo = {
		name: string;
		icon: string | null;
		accentColor: string | null;
		isSystem: boolean;
	};

	// Get workspace info by index
	function getWorkspaceInfo(index: number): WorkspaceInfo | null {
		const ws = workspaceStore.workspaces[index];
		if (!ws) return null;
		return {
			name: ws.is_system ? "Virtues" : ws.name,
			icon: ws.icon || null,
			accentColor: ws.accent_color || null,
			isSystem: ws.is_system,
		};
	}

	// Current transition workspaces [prev, current, next]
	const transitionWorkspaces = $derived.by((): [WorkspaceInfo | null, WorkspaceInfo, WorkspaceInfo | null] => {
		const currentIndex = workspaceStore.workspaces.findIndex(
			(w) => w.id === workspaceStore.activeWorkspaceId
		);
		return [
			getWorkspaceInfo(currentIndex - 1),
			getWorkspaceInfo(currentIndex) || { name: "Workspace", icon: null, accentColor: null, isSystem: false },
			getWorkspaceInfo(currentIndex + 1),
		];
	});

	// Handle Embla init
	function onEmblaInit(event: CustomEvent<EmblaCarouselType>) {
		emblaApi = event.detail;
		
		// Scroll to active workspace on init
		const index = workspaceStore.workspaces.findIndex(
			(w) => w.id === workspaceStore.activeWorkspaceId
		);
		if (index > 0) {
			emblaApi.scrollTo(index, true); // true = instant, no animation
		}

		// Listen for scroll to update title progress
		emblaApi.on('scroll', () => {
			if (!emblaApi) return;
			const progress = emblaApi.scrollProgress();
			const snapCount = workspaceStore.workspaces.length;
			const currentSnap = emblaApi.selectedScrollSnap();
			
			// Calculate progress relative to current snap (-1 to 1)
			// progress is 0-1 across all slides
			const progressPerSlide = 1 / (snapCount - 1 || 1);
			const currentSnapProgress = currentSnap * progressPerSlide;
			const relativeProgress = (progress - currentSnapProgress) / progressPerSlide;
			
			// Clamp to -1 to 1
			scrollProgress = Math.max(-1, Math.min(1, relativeProgress));
		});

		// Listen for slide changes
		emblaApi.on('select', () => {
			if (!emblaApi) return;
			const selectedIndex = emblaApi.selectedScrollSnap();
			const workspace = workspaceStore.workspaces[selectedIndex];
			if (workspace && workspace.id !== workspaceStore.activeWorkspaceId) {
				workspaceStore.activeWorkspaceId = workspace.id;
			}
		});

		// Reset progress when settle
		emblaApi.on('settle', () => {
			scrollProgress = 0;
		});
	}

	// Navigate to workspace (for chevron clicks)
	export function scrollToWorkspace(workspaceId: string) {
		if (!emblaApi) return;
		const index = workspaceStore.workspaces.findIndex((w) => w.id === workspaceId);
		if (index >= 0) {
			emblaApi.scrollTo(index);
		}
	}

	// Load state from localStorage and initialize
	onMount(() => {
		const storedCollapsed = localStorage.getItem(STORAGE_KEY);
		if (storedCollapsed !== null) {
			isCollapsed = storedCollapsed === "true";
		}

		// Initialize workspace store
		workspaceStore.init()
			.then(() => {
				storeReady = true;
			})
			.catch((err) => {
				console.error("[UnifiedSidebar] Failed to initialize workspace store:", err);
				storeReady = true;
			});

		// Keyboard shortcuts
		window.addEventListener("keydown", handleKeydown);
		return () => window.removeEventListener("keydown", handleKeydown);
	});

	// Persist collapsed state
	$effect(() => {
		localStorage.setItem(STORAGE_KEY, String(isCollapsed));
	});

	// Sync Embla with workspace changes from external sources (chevron clicks in WorkspaceSwitcher)
	$effect(() => {
		if (!emblaApi || !storeReady) return;
		const targetIndex = workspaceStore.workspaces.findIndex(
			(w) => w.id === workspaceStore.activeWorkspaceId
		);
		const currentIndex = emblaApi.selectedScrollSnap();
		if (targetIndex >= 0 && targetIndex !== currentIndex) {
			emblaApi.scrollTo(targetIndex);
		}
	});

	function handleKeydown(e: KeyboardEvent) {
		// Cmd+N or Ctrl+N - New chat
		if ((e.metaKey || e.ctrlKey) && e.key === "n") {
			e.preventDefault();
			handleNewChat();
		}
		// Cmd+[ or Ctrl+[ - Toggle sidebar collapse
		if ((e.metaKey || e.ctrlKey) && e.key === "[") {
			e.preventDefault();
			toggleCollapse();
		}
		// Cmd+K or Ctrl+K - Search
		if ((e.metaKey || e.ctrlKey) && e.key === "k") {
			e.preventDefault();
			handleSearch();
		}
		// Cmd+B or Ctrl+B - Open bookmarks
		if ((e.metaKey || e.ctrlKey) && e.key === "b") {
			e.preventDefault();
			handleOpenBookmarks();
		}
		// Cmd+D or Ctrl+D - Bookmark current tab
		if ((e.metaKey || e.ctrlKey) && e.key === "d") {
			e.preventDefault();
			handleBookmarkCurrentTab();
		}
	}

	function handleSearch() {
		isSearchOpen = true;
	}

	function closeSearch() {
		isSearchOpen = false;
	}

	function handleOpenBookmarks() {
		isBookmarksOpen = true;
	}

	function closeBookmarks() {
		isBookmarksOpen = false;
	}

	async function handleBookmarkCurrentTab() {
		const tab = workspaceStore.activeTab;
		if (!tab) return;

		await bookmarks.toggleRouteBookmark({
			route: tab.route,
			tab_type: tab.type,
			label: tab.label,
			icon: tab.icon,
		});
	}

	function handleNewChat() {
		// Find existing new chat tab (no conversationId) or create one
		const existingNewChat = workspaceStore
			.getAllTabs()
			.find((t) => t.type === "chat" && !t.conversationId);

		if (existingNewChat) {
			workspaceStore.setActiveTab(existingNewChat.id);
		} else {
			workspaceStore.openTabFromRoute("/", {
				label: "New Chat",
				preferEmptyPane: true,
			});
		}
	}

	function handleGoHome() {
		// Navigate based on user's fallback preference
		const pref = workspaceStore.fallbackPreference;
		switch (pref) {
			case "chat":
				handleNewChat();
				break;
			case "conway":
				workspaceStore.openTabFromRoute("/life", { preferEmptyPane: true });
				break;
			case "dog-jump":
				workspaceStore.openTabFromRoute("/jump", { preferEmptyPane: true });
				break;
			case "wiki-today": {
				const today = new Date().toISOString().split("T")[0];
				workspaceStore.openTabFromRoute(`/wiki/${today}`, { preferEmptyPane: true });
				break;
			}
			case "empty":
			default:
				handleNewChat();
				break;
		}
	}

	function toggleCollapse() {
		isCollapsed = !isCollapsed;
	}

	function handleCreateWorkspace() {
		showNewWorkspaceModal = true;
		newWorkspaceName = "";
	}

	async function createWorkspace() {
		if (!newWorkspaceName.trim()) return;
		await workspaceStore.createWorkspace(newWorkspaceName.trim());
		showNewWorkspaceModal = false;
		newWorkspaceName = "";
	}

	// Stagger delay per item
	const STAGGER_DELAY = 30;

	// Tailwind utility class strings
	const sidebarClass = $derived.by(() =>
		[
			"relative h-full overflow-hidden bg-[var(--surface-elevated)]",
			"transition-[width] ease-[cubic-bezier(0.34,1.56,0.64,1)]",
			isCollapsed
				? ["w-8", "duration-400 delay-0"].join(" ")
				: ["w-52", "duration-300 delay-100"].join(" "),
		].join(" "),
	);

	const sidebarInnerClass = $derived.by(() =>
		[
			"flex h-full min-w-52 w-52 flex-col",
			isCollapsed ? "pointer-events-none" : "",
		].join(" "),
	);
</script>

<aside class={sidebarClass}>
	<!-- Book Spine: When collapsed, the entire sidebar IS the clickable spine -->
	{#if isCollapsed}
		<button
			class="group absolute inset-0 z-10 flex h-full w-full cursor-pointer items-center justify-center border-none bg-transparent"
			onclick={toggleCollapse}
			aria-label="Expand sidebar"
		>
			<svg
				class="h-4 w-4 -translate-x-1 opacity-0 transition-all duration-200 ease-[cubic-bezier(0.2,0,0,1)] group-hover:translate-x-0 group-hover:opacity-100 group-active:scale-95"
				style="color: var(--color-foreground-subtle)"
				viewBox="0 0 16 16"
				fill="currentColor"
			>
				<path
					d="M2 2.5A2.5 2.5 0 0 1 4.5 0h7A2.5 2.5 0 0 1 14 2.5v11a2.5 2.5 0 0 1-2.5 2.5h-7A2.5 2.5 0 0 1 2 13.5v-11zM4.5 1A1.5 1.5 0 0 0 3 2.5v11A1.5 1.5 0 0 0 4.5 15h7a1.5 1.5 0 0 0 1.5-1.5v-11A1.5 1.5 0 0 0 11.5 1h-7z"
				/>
				<path d="M5 4h6v1H5V4zm0 2h6v1H5V6zm0 2h3v1H5V8z" />
			</svg>
		</button>
	{/if}

	<div class={sidebarInnerClass}>
		<WorkspaceHeader
			collapsed={isCollapsed}
			onNewChat={handleNewChat}
			onGoHome={handleGoHome}
			onToggleCollapse={toggleCollapse}
			onSearch={handleSearch}
			logoAnimationDelay={0}
			actionsAnimationDelay={STAGGER_DELAY}
			{scrollProgress}
			{transitionWorkspaces}
		/>

		<!-- Embla Carousel for workspace swiping -->
		<div 
			class="embla"
			class:collapsed={isCollapsed}
			use:emblaCarouselSvelte={{ options: emblaOptions, plugins: [WheelGesturesPlugin({ forceWheelAxis: 'x' })] }}
			onemblaInit={onEmblaInit}
		>
			<div class="embla__container">
				{#each workspaceStore.workspaces as workspace (workspace.id)}
					{@const wsTree = workspaceStore.getTreeForWorkspace(workspace.id)}
					{@const isActive = workspace.id === workspaceStore.activeWorkspaceId}
					{@const isSystem = workspace.is_system}
					<nav class="embla__slide" class:collapsed={isCollapsed}>
						{#if !storeReady}
							<div class="loading-state">
								<iconify-icon icon="ri:loader-4-line" width="16" class="spinner"></iconify-icon>
								<span>Loading...</span>
							</div>
						{:else if wsTree.length === 0}
							<div class="empty-state">
								{#if isSystem}
									<p>No items yet</p>
								{:else}
									<p>This workspace is empty</p>
									{#if isActive}
										<button class="add-btn" onclick={() => workspaceStore.createFolder("New Folder")}>
											<iconify-icon icon="ri:folder-add-line" width="14"></iconify-icon>
											New Folder
										</button>
									{/if}
								{/if}
							</div>
						{:else}
							{#each wsTree as node (node.id)}
								{#if node}
									<ExplorerNode
										{node}
										collapsed={isCollapsed}
									/>
								{/if}
							{/each}
						{/if}
					</nav>
				{/each}
			</div>
		</div>

		<SidebarFooter
			collapsed={isCollapsed}
			animationDelay={10 * STAGGER_DELAY}
		/>
	</div>
</aside>

<SearchModal open={isSearchOpen} onClose={closeSearch} />
<BookmarksModal open={isBookmarksOpen} onClose={closeBookmarks} />

<!-- New Workspace Modal -->
{#if showNewWorkspaceModal}
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<div class="modal-backdrop" role="presentation" onclick={() => (showNewWorkspaceModal = false)}>
		<div class="modal" role="dialog" aria-modal="true" tabindex="-1" onclick={(e) => e.stopPropagation()}>
			<h3>New Workspace</h3>
			<input
				type="text"
				placeholder="Workspace name"
				bind:value={newWorkspaceName}
				onkeydown={(e) => e.key === "Enter" && createWorkspace()}
			/>
			<div class="modal-actions">
				<button class="cancel-btn" onclick={() => (showNewWorkspaceModal = false)}>
					Cancel
				</button>
				<button class="create-btn" onclick={createWorkspace}>Create</button>
			</div>
		</div>
	</div>
{/if}

<style>
	@reference "../../../app.css";

	:root {
		--ease-premium: cubic-bezier(0.2, 0, 0, 1);
	}

	@keyframes fadeSlideIn {
		from {
			opacity: 0;
			transform: translateX(-8px);
		}
		to {
			opacity: 1;
			transform: translateX(0);
		}
	}

	@keyframes spin {
		from {
			transform: rotate(0deg);
		}
		to {
			transform: rotate(360deg);
		}
	}

	/* Embla Carousel styles */
	.embla {
		flex: 1;
		overflow: hidden;
		min-height: 0; /* Allow flex shrinking */
	}

	.embla.collapsed {
		pointer-events: none;
	}

	.embla__container {
		display: flex;
		height: 100%;
		touch-action: pan-y pinch-zoom; /* Allow vertical scroll within slides */
	}

	.embla__slide {
		flex: 0 0 100%;
		min-width: 0;
		overflow-y: auto;
		overflow-x: hidden;
		padding: 12px 8px;
	}

	.embla__slide.collapsed {
		display: flex;
		flex-direction: column;
		align-items: center;
	}

	.loading-state {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 12px;
		color: var(--color-foreground-subtle);
		font-size: 13px;
	}

	.spinner {
		animation: spin 1s linear infinite;
	}

	.empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 12px;
		padding: 24px 12px;
		color: var(--color-foreground-subtle);
		font-size: 13px;
		text-align: center;
	}

	.empty-state .add-btn {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 12px;
		border-radius: 6px;
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
		color: var(--color-foreground-muted);
		font-size: 12px;
		border: none;
		cursor: pointer;
		transition: all 150ms var(--ease-premium);
	}

	.empty-state .add-btn:hover {
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
		color: var(--color-foreground);
	}

	/* Modal styles */
	.modal-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.5);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 10000;
	}

	.modal {
		background: var(--surface);
		border-radius: 12px;
		padding: 20px;
		min-width: 300px;
		box-shadow: 0 8px 32px rgba(0, 0, 0, 0.2);
	}

	.modal h3 {
		margin: 0 0 16px 0;
		font-size: 16px;
		font-weight: 600;
		color: var(--foreground);
	}

	.modal input {
		width: 100%;
		padding: 10px 12px;
		border: 1px solid var(--border);
		border-radius: 8px;
		background: var(--surface-elevated);
		color: var(--foreground);
		font-size: 14px;
		margin-bottom: 16px;
	}

	.modal input:focus {
		outline: none;
		border-color: var(--primary);
	}

	.modal-actions {
		display: flex;
		gap: 8px;
		justify-content: flex-end;
	}

	.modal-actions button {
		padding: 8px 16px;
		border-radius: 6px;
		font-size: 13px;
		cursor: pointer;
		transition: all 150ms ease;
	}

	.cancel-btn {
		background: transparent;
		border: 1px solid var(--border);
		color: var(--foreground-muted);
	}

	.cancel-btn:hover {
		background: var(--surface-elevated);
	}

	.create-btn {
		background: var(--primary);
		border: none;
		color: white;
	}

	.create-btn:hover {
		opacity: 0.9;
	}
</style>
