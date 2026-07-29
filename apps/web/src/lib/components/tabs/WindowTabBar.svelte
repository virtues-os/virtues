<script lang="ts">
	import { flip } from "svelte/animate";
	import { dndzone } from "svelte-dnd-action";
	import type { DndEvent } from "svelte-dnd-action";
	import Icon from "$lib/components/Icon.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import type { Tab } from "$lib/stores/window-shell.svelte";
	import {
		dndManager,
		type DndTabItem,
		type ZoneId,
	} from "$lib/stores/dndManager.svelte";
	import { contextMenu } from "$lib/stores/contextMenu.svelte";
	import { sidebarState } from "$lib/stores/sidebarState.svelte";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";
	import { iconPickerStore } from "$lib/stores/iconPicker.svelte";
	import { getNotebookMenuItems } from "$lib/utils/contextMenuItems";
	import { updatePage, updateChat } from "$lib/api/client";
	import { pagesStore } from "$lib/stores/pages.svelte";
	import { pinsStore } from "$lib/stores/pins.svelte";
	import { paneActions } from "$lib/stores/paneActions.svelte";
	import { modifierHint } from "$lib/stores/modifierHint.svelte";
	import { chatSessions } from "$lib/stores/chatSessions.svelte";
	import { isEmoji } from "$lib/utils/iconHelpers";
	import { clothFor, textOnCloth } from "$lib/sidebar/pin-colors";
	import { pinMenuItem } from "$lib/pins/pinAction";
	import type { ContextMenuItem } from "$lib/stores/contextMenu.svelte";

	const FLIP_DURATION_MS = 150;

	/**
	 * A tab for something on the Desk wears its bookcloth.
	 *
	 * The colour belongs to the THING, not to the window — which is why this is
	 * per-tab and not a band across the pane. A pane holds tabs from several
	 * worlds at once; painting the pane would claim the whole window for
	 * whichever tab happened to be in front.
	 *
	 * Keyed on "is this route pinned?", not on what species it is: anything
	 * with a url can sit on the Desk, so the pin list is the only thing that
	 * knows. That also keeps a filled tab rare, which is the only reason it
	 * can afford to be loud.
	 *
	 * Active tabs take the solid cloth with text inverted for legibility;
	 * inactive ones drop to a wash so identity survives without competing with
	 * focus.
	 */
	function tabCloth(route: string | undefined): string | null {
		if (!route) return null;
		const pin = pinsStore.getByUrl(route);
		return pin ? clothFor(pin) : null;
	}

	function clothStyle(tab: Tab, isActive: boolean): string {
		const cloth = tabCloth(tab.route);
		if (!cloth) return "";
		if (isActive) {
			return `background:${cloth};color:${textOnCloth(cloth)};`;
		}
		return `background:color-mix(in srgb, ${cloth} 12%, transparent);color:${cloth};`;
	}

	interface Props {
		paneId?: "left" | "right"; // When set, renders as a pane tab bar in split mode
	}

	let { paneId }: Props = $props();

	// Rename state
	let renamingTabId = $state<string | null>(null);
	let renameValue = $state("");

	// DnD items state - we need mutable state for svelte-dnd-action
	let dndItems = $state<DndTabItem[]>([]);

	// Zone identifier for this tab bar instance
	const zoneId: ZoneId = $derived({
		type: "tab-bar" as const,
		paneId,
	});

	// Derive tabs and active state based on mode
	const tabs = $derived(
		paneId
			? (paneId === "left"
					? windowShellStore.leftPane?.tabs
					: windowShellStore.rightPane?.tabs) || []
			: windowShellStore.tabs,
	);

	const activeTabId = $derived(
		paneId
			? paneId === "left"
				? windowShellStore.leftPane?.activeTabId
				: windowShellStore.rightPane?.activeTabId
			: windowShellStore.activeTabId,
	);

	const isActivePane = $derived(
		paneId ? windowShellStore.activePaneId === paneId : true,
	);

	// Whatever the view showing in this pane has published (item 5's slot).
	const viewActions = $derived(paneActions.for(activeTabId));

	// Two inline, the rest behind a `···`. A pane can be dragged to a third of
	// the window, and the toolbar already carries split/merge/new-tab — without
	// a cap, a view with four actions would push the tabs out of their own bar.
	const INLINE_ACTION_LIMIT = 2;
	const inlineActions = $derived(viewActions.slice(0, INLINE_ACTION_LIMIT));
	const overflowActions = $derived(viewActions.slice(INLINE_ACTION_LIMIT));

	function showActionOverflow(e: MouseEvent) {
		e.stopPropagation();
		contextMenu.show(
			{ x: e.clientX, y: e.clientY },
			overflowActions.map((a) => ({
				id: a.id,
				label: a.label,
				icon: a.icon,
				disabled: a.disabled,
				checked: a.active,
				action: a.run,
			})),
		);
	}
	const isSplitMode = $derived(windowShellStore.isSplit);

	// ⌘1/⌘2 badge, shown only while ⌘ is held. Single-pane mode is always ⌘1.
	const paneNumber = $derived(paneId === "right" ? "2" : "1");
	const showPaneHint = $derived(
		modifierHint.visible && !mobileLayout.isMobile,
	);

	// Per-tab history (browser model): back/forward act on this pane's active tab.
	const canGoBack = $derived(windowShellStore.canGoBack(paneId));
	const canGoForward = $derived(windowShellStore.canGoForward(paneId));

	function handleBack() {
		windowShellStore.goBack(paneId);
	}

	function handleForward() {
		windowShellStore.goForward(paneId);
	}

	function handleNewTab() {
		windowShellStore.openTab(
			{ type: "home", label: "Home", route: "/home", icon: "ri:home-5-line" },
			paneId,
		);
	}

	// Build DnD items from tabs with source information
	function buildDndItems(): DndTabItem[] {
		return tabs.map((tab) => ({
			id: tab.id,
			url: tab.route,
			label: tab.label,
			icon: tab.icon,
			source: zoneId,
			tab,
		}));
	}

	// Sync DnD items when tabs change
	$effect(() => {
		// Rebuild from tabs (the source of truth)
		dndItems = buildDndItems();
	});

	function handleTabClick(id: string) {
		if (paneId) {
			windowShellStore.setActiveTabInPane(id, paneId);
		} else {
			windowShellStore.setActiveTab(id);
		}
	}

	function handleTabClose(e: MouseEvent, id: string) {
		e.stopPropagation();
		if (paneId) {
			windowShellStore.closeTabInPane(id, paneId);
		} else {
			windowShellStore.closeTab(id);
		}
	}

	function handleToggleSplit() {
		windowShellStore.toggleSplit();
	}

	function handleMergePanes() {
		windowShellStore.disableSplit();
	}

	function handleMiddleClick(e: MouseEvent, id: string) {
		if (e.button === 1) {
			e.preventDefault();
			if (paneId) {
				windowShellStore.closeTabInPane(id, paneId);
			} else {
				windowShellStore.closeTab(id);
			}
		}
	}

	function handleContextMenu(e: MouseEvent, tabId: string) {
		e.preventDefault();

		const tab = tabs.find((t) => t.id === tabId);
		if (!tab) return;

		const tabIndex = tabs.findIndex((t) => t.id === tabId);
		const hasTabsToRight = tabIndex !== -1 && tabIndex < tabs.length - 1;

		// Parse route to determine entity type for icon changes
		const routeParts = tab.route?.split('/').filter(Boolean) ?? [];
		const tabEntityType = routeParts[0]; // 'page', 'chat', etc.
		const tabEntityId = routeParts[1];
		const canChangeIcon = tabEntityType && tabEntityId && (tabEntityType === 'page' || tabEntityType === 'chat');

		// Build context menu items
		const items: ContextMenuItem[] = [
			// Compact/Expand
			{
				id: "compact",
				label: tab.pinned ? "Expand" : "Compact",
				icon: tab.pinned
					? "ri:expand-left-right-line"
					: "ri:contract-left-right-line",
				action: () => windowShellStore.togglePin(tabId),
			},
			// Rename
			{
				id: "rename",
				label: "Rename",
				icon: "ri:edit-line",
				action: () => {
					renamingTabId = tabId;
					renameValue = tab.label;
				},
			},
		];

		// Change Icon (for page/chat tabs)
		if (canChangeIcon) {
			items.push({
				id: "change-icon",
				label: "Change Icon",
				icon: "ri:emotion-line",
				action: () => {
					iconPickerStore.show(tab.icon ?? null, async (icon) => {
						try {
							if (tabEntityType === 'page') {
								await updatePage(tabEntityId, { icon });
								await pagesStore.refresh();
							} else if (tabEntityType === 'chat') {
								await updateChat(tabEntityId, { icon });
								chatSessions.updateSessionIcon(tabEntityId, icon);
							}
							windowShellStore.invalidateViewCache();
						} catch (err) {
							console.error("[WindowTabBar] Failed to change icon:", err);
						}
					});
				},
			});
		}


		// Divider + Close actions
		items.push({
			id: "close",
			label: "Close",
			dividerBefore: true,
			action: () => {
				if (paneId) {
					windowShellStore.closeTabInPane(tabId, paneId);
				} else {
					windowShellStore.closeTab(tabId);
				}
			},
		});

		// Close Others (only if more than 1 tab)
		if (tabs.length > 1) {
			items.push({
				id: "close-others",
				label: "Close Others",
				action: () => windowShellStore.closeOtherTabs(tabId, paneId),
			});
		}

		// Close to Right (only if tabs exist to the right)
		if (hasTabsToRight) {
			items.push({
				id: "close-to-right",
				label: "Close to the Right",
				action: () => windowShellStore.closeTabsToRight(tabId, paneId),
			});
		}

		// Add "Add to Folder" / "Move to Workspace" submenus if tab has a route
		if (tab.route) {
			items.push(...getNotebookMenuItems(tab.route));
			// Anything you can open, you can keep. The tab is the one surface
			// that exists for every route in the app, so wiring the pin here
			// makes the Desk reachable from everywhere by construction rather
			// than by remembering to add a menu item per view.
			items.push(
				pinMenuItem({ url: tab.route, label: tab.label, icon: tab.icon }),
			);
		}

		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}

	function handleRenameSubmit() {
		if (!renamingTabId || !renameValue.trim()) {
			handleRenameCancel();
			return;
		}
		const newLabel = renameValue.trim();
		windowShellStore.updateTab(renamingTabId, { label: newLabel });
		renamingTabId = null;
		renameValue = "";
	}

	function handleRenameCancel() {
		renamingTabId = null;
		renameValue = "";
	}

	function handleRenameKeydown(e: KeyboardEvent) {
		if (e.key === "Enter") {
			e.preventDefault();
			handleRenameSubmit();
		} else if (e.key === "Escape") {
			e.preventDefault();
			handleRenameCancel();
		}
	}

	function handleDoubleClick(e: MouseEvent, tabId: string) {
		e.preventDefault();
		const tab = tabs.find((t) => t.id === tabId);
		if (!tab || tab.pinned) return;
		renamingTabId = tabId;
		renameValue = tab.label;
	}

	// svelte-dnd-action handlers - delegate to centralized dndManager
	function handleDndConsider(e: CustomEvent<DndEvent<DndTabItem>>) {
		// Pass current dndItems as originalItems - svelte-dnd-action modifies the array
		// before firing consider, so we need the pre-modified version to find the dragged item
		dndManager.handleConsider(
			e,
			zoneId,
			(items) => {
				dndItems = items;
			},
			dndItems,
		);
	}

	function handleDndFinalize(e: CustomEvent<DndEvent<DndTabItem>>) {
		dndManager.handleFinalize(e, zoneId, (items) => {
			dndItems = items;
		});
	}

	// Show sidebar toggle on left pane (or non-split mode). Hidden on mobile —
	// the sidebar is replaced by the bottom-tab bar, so there's nothing to toggle.
	const showSidebarToggle = $derived(
		!mobileLayout.isMobile && (!paneId || paneId === "left"),
	);

	// Icon changes based on sidebar state
	const sidebarIcon = $derived(
		sidebarState.collapsed ? "ri:layout-right-line" : "ri:side-bar-line",
	);

	// Get icon for tab type
	function getDefaultIcon(type: string): string {
		switch (type) {
			case "chat":
				return "ri:chat-1-line";
			case "history":
				return "ri:history-line";
			case "wiki":
				return "ri:book-2-line";
			case "wiki-list":
				return "ri:list-check";
			case "data-sources":
				return "ri:database-2-line";
			case "data-sources-add":
				return "ri:add-circle-line";
			case "data-jobs":
				return "ri:refresh-line";
			case "storage":
				return "ri:hard-drive-2-line";
			case "usage":
				return "ri:bar-chart-line";
			case "profile":
				return "ri:user-settings-line";
			default:
				return "ri:file-line";
		}
	}
</script>

<div
	class="tab-bar"
	class:split-pane={!!paneId}
	class:active-pane={isActivePane && !!paneId}
	role="toolbar"
	aria-label="Tab bar"
	tabindex="0"
>
	{#if showSidebarToggle}
		<button
			class="sidebar-toggle"
			onclick={() => sidebarState.toggle()}
			aria-label="Toggle sidebar"
			title="Toggle sidebar (⌘S)"
		>
			<Icon icon={sidebarIcon} />
		</button>
	{/if}

	<div class="nav-cluster">
		<button
			class="nav-btn"
			onclick={handleBack}
			disabled={!canGoBack}
			aria-label="Back"
			title="Back"
		>
			<Icon icon="ri:arrow-left-s-line" />
		</button>
		<button
			class="nav-btn"
			onclick={handleForward}
			disabled={!canGoForward}
			aria-label="Forward"
			title="Forward"
		>
			<Icon icon="ri:arrow-right-s-line" />
		</button>
	</div>

	<div
		class="tabs-scroll"
		role="tablist"
		tabindex="0"
		use:dndzone={{
			items: dndItems,
			type: "tab",
			flipDurationMs: FLIP_DURATION_MS,
			dropTargetStyle: {},
			dragDisabled: renamingTabId !== null,
		}}
		onconsider={handleDndConsider}
		onfinalize={handleDndFinalize}
	>
		{#each dndItems as item (item.id)}
			{@const tab = item.tab}
			<div
				class="tab"
				class:active={tab.id === activeTabId}
				class:active-in-active-pane={tab.id === activeTabId &&
					isActivePane}
				class:pinned={tab.pinned}
				class:renaming={tab.id === renamingTabId}
				class:clothed={!!tabCloth(tab.route)}
				style={clothStyle(tab, tab.id === activeTabId && isActivePane)}
				animate:flip={{ duration: FLIP_DURATION_MS }}
				onclick={() =>
					tab.id !== renamingTabId && handleTabClick(tab.id)}
				ondblclick={(e) => handleDoubleClick(e, tab.id)}
				onauxclick={(e) => handleMiddleClick(e, tab.id)}
				oncontextmenu={(e) => handleContextMenu(e, tab.id)}
				onkeydown={(e) =>
					e.key === "Enter" &&
					tab.id !== renamingTabId &&
					handleTabClick(tab.id)}
				title={tab.id !== renamingTabId ? tab.label : ""}
				role="button"
				tabindex="0"
			>
				{#if item.icon && isEmoji(item.icon)}
					<span class="tab-emoji">{item.icon}</span>
				{:else}
					<Icon icon={item.icon || getDefaultIcon(tab.type)} class="tab-icon" />
				{/if}
				{#if !tab.pinned}
					{#if tab.id === renamingTabId}
						<!-- svelte-ignore a11y_autofocus -->
						<input
							type="text"
							class="tab-rename-input"
							bind:value={renameValue}
							onkeydown={handleRenameKeydown}
							onblur={handleRenameSubmit}
							onclick={(e) => e.stopPropagation()}
							autofocus
						/>
					{:else}
						<!-- On a ⌘ hold the ACTIVE tab's own label becomes its pane
						     number: the title slides up and out, ⌘1 slides up and
						     in. Nothing new enters the screen, which is the point —
						     the old version floated a saturated blue chip into the
						     toolbar, the loudest object in an otherwise quiet
						     window, and it announced the shortcut instead of
						     teaching which label it belongs to.
						     Only the active tab flips: every tab in a pane shares
						     the pane's number, so flipping all of them would say
						     the same thing five times. -->
						<span class="tab-label" class:flipping={showPaneHint && tab.id === activeTabId}>
							<span class="tab-label-text">{tab.label}</span>
							<span class="tab-label-hint" aria-hidden="true">⌘{paneNumber}</span>
						</span>
					{/if}
				{/if}
				{#if !tab.pinned && tab.id !== renamingTabId}
					<button
						class="tab-close"
						onclick={(e) => handleTabClose(e, tab.id)}
						aria-label="Close tab"
					>
						<Icon icon="ri:close-line" />
					</button>
				{/if}
			</div>
		{/each}
	</div>

	<button
		class="new-tab-btn"
		onclick={handleNewTab}
		aria-label="New tab"
		title="New tab"
	>
		<Icon icon="ri:add-line" />
	</button>

	{#if !paneId && !mobileLayout.isMobile}
		<button
			class="split-toggle"
			onclick={handleToggleSplit}
			aria-label="Split view"
			title="Split view"
		>
			<Icon icon="ri:layout-column-line" />
		</button>
	{/if}

	{#if paneId === "right" && isSplitMode}
		<button
			class="merge-toggle"
			onclick={handleMergePanes}
			aria-label="Merge panes"
			title="Merge panes"
		>
			<Icon icon="ri:layout-right-line" />
		</button>
	{/if}

	<!-- The view's own actions, published into the slot rather than rendered
	     wherever each view felt like putting them. Last in the row, after the
	     window controls, so the shell's controls stay in one place as views
	     come and go beneath them. -->
	{#if viewActions.length > 0}
		<div class="pane-actions">
			{#each inlineActions as action (action.id)}
				<button
					class="pane-action"
					class:primary={action.primary}
					class:toggled={action.active}
					aria-pressed={action.active !== undefined ? action.active : undefined}
					disabled={action.disabled}
					onclick={action.run}
					aria-label={action.label}
					title={action.label}
				>
					<Icon icon={action.icon} />
					{#if action.primary}<span class="pane-action-label">{action.label}</span>{/if}
				</button>
			{/each}

			{#if overflowActions.length > 0}
				<button
					class="pane-action"
					onclick={showActionOverflow}
					aria-label="More actions"
					title="More actions"
				>
					<Icon icon="ri:more-line" />
				</button>
			{/if}
		</div>
	{/if}
</div>

<style>
	.tab-bar {
		container-type: inline-size;
		container-name: tabbar;
		display: flex;
		align-items: stretch;
		gap: 4px;
		padding: 0 8px;
		min-height: var(--chrome-row-h);
		align-items: center;
		border-bottom: 1px solid var(--color-border);
		/* Page colour, not a recessed strip. The browser model needs the strip
		   to be darker so the active tab can read as a hole cut in it — but a
		   recessed band next to a large recessed sidebar made the whole left
		   half of the window a slab. Pills carry the state instead, so the strip
		   has no reason to be a different colour from what it sits on. */
		background: var(--color-surface);
		flex-shrink: 0;
		position: relative;
		z-index: var(--z-overlay); /* Above global drag overlays */
	}

	/* Card top rounding in split mode */
	.tab-bar.split-pane {
		border-top-left-radius: var(--card-radius, 6px);
		border-top-right-radius: var(--card-radius, 6px);
	}

	/* The focused pane's strip lifts a touch, so "which pane am I in" is legible
	   from the chrome and not only from the tab. */
	.tab-bar.active-pane {
		background: var(--color-surface-elevated);
	}

	.tabs-scroll {
		display: flex;
		align-items: center;
		gap: 2px;
		overflow-x: auto;
		flex: 1;
		scrollbar-width: none;
		height: var(--chrome-tab-h);
	}

	.tabs-scroll::-webkit-scrollbar {
		display: none;
	}

	/* A pill. Inset, rounded on all four corners, centred in the strip with real
	   air above and below. Both alternatives were tried and rejected: the
	   full-height container needed a recessed strip it never had, and the
	   bottom-anchored browser tab needed one badly enough to turn the sidebar
	   into a slab. A pill states what it is and costs nothing around it. */
	.tab {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 0 10px;
		align-self: center;
		position: relative;
		height: 28px;
		border: none;
		border-radius: 6px;
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: 12px;
		cursor: pointer;
		white-space: nowrap;
		max-width: 160px;
		min-width: 80px;
		flex-shrink: 0;
		transition:
			background-color 150ms ease,
			color 150ms ease,
			opacity 150ms ease;
	}

	.tab:hover {
		background: var(--hover-bg);
		color: var(--color-foreground);
	}

	/* Fill from the interaction ramp, not a surface token. A surface token is
	   what made the active tab #FFFFFF on #FFFFFF; the ramp is defined against
	   the text colour, so it is legible in all sixteen themes. */
	.tab.active {
		background: var(--tab-active-bg-focused);
		color: var(--color-foreground);
		font-weight: 500;
	}

	/* In split view two tabs are "active" at once and only one takes your
	   typing. The unfocused pane's sits at the lighter step. */
	.tab.active:not(.active-in-active-pane) {
		background: var(--tab-active-bg);
		color: var(--color-foreground-muted);
	}

	/* A clothed tab supplies its own fill and text colour inline, so the hover
	   and active rules — which are written against the neutral ramp — must not
	   paint over it. Hover reads as a slight lift instead. */
	.tab.clothed:hover {
		background: inherit;
		filter: brightness(1.08);
	}

	.tab.clothed :global(.tab-icon),
	.tab.clothed .tab-close {
		color: inherit;
		opacity: 0.85;
	}

	/* Dragging state - svelte-dnd-action applies aria-grabbed */
	:global(.tab[aria-grabbed="true"]) {
		opacity: 0.5;
	}

	/* A compacted tab — the "Compact / Expand" context action, which shrinks a
	   tab to its icon. Nothing to do with sidebar pins despite the class name.

	   It used to be filled with --color-primary at 15% and its icon set to the
	   accent outright, which made a merely-narrow tab the most saturated object
	   in the window — a theme accent asserting meaning where the only fact is
	   "this tab is short". Narrow is legible from being narrow. */
	.tab.pinned {
		min-width: auto;
		max-width: none;
		padding: 0 8px;
		gap: 0;
	}

	:global(.tab-icon) {
		flex-shrink: 0;
		font-size: 13px;
		opacity: 0.7;
	}

	.tab.active :global(.tab-icon) {
		opacity: 1;
	}

	/* An odometer: two lines stacked in a one-line window, shifted by 100% to
	   swap which one shows. The hint is neutral text, not an accent chip —
	   nothing in this shell should be more saturated than the work. */
	.tab-label {
		position: relative;
		overflow: hidden;
		text-overflow: ellipsis;
		flex: 1;
		text-align: left;
		height: 1.35em;
		line-height: 1.35em;
	}

	.tab-label-text,
	.tab-label-hint {
		display: block;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		transition: transform 140ms cubic-bezier(0.2, 0, 0, 1);
	}

	/* Same face and size as the label it replaces. It was monospace, which is
	   decoration standing in for meaning — the shortcut isn't code, and a
	   different typeface for one word makes the swap read as a glitch rather
	   than as the same label saying something else. */
	.tab-label-hint {
		position: absolute;
		inset: 0;
		transform: translateY(100%);
		font-weight: 600;
		color: var(--color-foreground);
	}

	.tab-label.flipping .tab-label-text {
		transform: translateY(-100%);
	}

	.tab-label.flipping .tab-label-hint {
		transform: translateY(0);
	}

	/* Reduced motion still swaps — the information matters — it just doesn't
	   travel to get there. */
	@media (prefers-reduced-motion: reduce) {
		.tab-label-text,
		.tab-label-hint {
			transition: none;
		}
	}

	.tab-rename-input {
		flex: 1;
		min-width: 60px;
		padding: 0;
		border: none;
		background: transparent;
		color: var(--color-foreground);
		font-size: 12px;
		font-family: inherit;
		outline: none;
		caret-color: var(--color-primary);
	}

	.tab.renaming {
		background: color-mix(in srgb, var(--color-primary) 20%, transparent);
		cursor: text;
	}

	.tab-emoji {
		font-size: 12px;
		line-height: 14px;
		width: 14px;
		text-align: center;
		flex-shrink: 0;
	}



	.pane-actions {
		display: flex;
		align-items: center;
		align-self: center;
		gap: 2px;
		margin-left: 2px;
		padding-left: 6px;
		border-left: 1px solid var(--color-border);
	}

	.pane-action {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		padding: 0;
		border: none;
		border-radius: 6px;
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: 15px;
		cursor: pointer;
		flex-shrink: 0;
		transition: background-color 150ms ease, color 150ms ease;
	}

	.pane-action:hover:not(:disabled) {
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
		color: var(--color-foreground);
	}

	.pane-action.primary {
		color: var(--color-primary);
		width: auto;
		gap: 5px;
		padding: 0 8px;
	}

	.pane-action-label {
		font-size: 12px;
		white-space: nowrap;
	}

	/* The primary action keeps its label while there's room and drops to an
	   icon when there isn't. A container query, not a viewport one: what runs
	   out of space is the pane, and in split view a pane is nothing like the
	   window. */
	@container tabbar (max-width: 520px) {
		.pane-action-label {
			display: none;
		}
		.pane-action.primary {
			width: 24px;
			padding: 0;
		}
	}

	/* A toggle that's on reads as held down, not as merely hovered. */
	.pane-action.toggled {
		background: var(--active-bg);
		color: var(--color-foreground);
	}

	.pane-action:disabled {
		opacity: 0.4;
		cursor: default;
	}

	.pane-action:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

	.tab-close {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 20px;
		height: 20px;
		padding: 0;
		border: none;
		border-radius: 4px;
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: 13px;
		cursor: pointer;
		opacity: 0;
		transition:
			opacity 150ms ease,
			background-color 150ms ease;
		flex-shrink: 0;
	}

	.tab:hover .tab-close,
	.tab.active .tab-close {
		opacity: 1;
	}

	/* Neutral, not red. Red is for destructive-with-consequence; closing a tab
	   loses nothing and is one ⌘⇧T away from undone. The old error-tinted
	   treatment (plus a red wash over the whole tab) read as a warning for an
	   action that doesn't warrant one. */
	.tab-close:hover {
		background: color-mix(in srgb, var(--color-foreground) 10%, transparent);
		color: var(--color-foreground);
	}

	.tab-close:active {
		background: color-mix(in srgb, var(--color-foreground) 16%, transparent);
	}

	.tab-close:focus-visible {
		opacity: 1;
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

	.sidebar-toggle,
	.split-toggle,
	.merge-toggle,
	.nav-btn,
	.new-tab-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		/* The bar bottom-aligns its children so tabs meet the pane; the window
		   controls opt back out and centre themselves in the strip. */
		align-self: center;
		width: 24px;
		height: 24px;
		padding: 0;
		border: none;
		border-radius: 6px;
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: 16px;
		cursor: pointer;
		flex-shrink: 0;
		transition:
			background-color 150ms ease,
			color 150ms ease;
	}

	.sidebar-toggle {
		margin-right: 2px;
	}

	/* Hover used to be `--color-surface-elevated` over a `--color-background`
	   toolbar — #F5F4EF on #FDFCF9 in the default theme, a ~3% shift that read
	   as nothing happening. A foreground mix is legible in every theme instead
	   of depending on two surface tokens staying far enough apart. */
	.sidebar-toggle:hover,
	.split-toggle:hover,
	.merge-toggle:hover,
	.nav-btn:hover:not(:disabled),
	.new-tab-btn:hover {
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
		color: var(--color-foreground);
	}

	.sidebar-toggle:active,
	.split-toggle:active,
	.merge-toggle:active,
	.nav-btn:active:not(:disabled),
	.new-tab-btn:active {
		background: color-mix(in srgb, var(--color-foreground) 14%, transparent);
	}

	.sidebar-toggle:focus-visible,
	.split-toggle:focus-visible,
	.merge-toggle:focus-visible,
	.nav-btn:focus-visible,
	.new-tab-btn:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
		color: var(--color-foreground);
	}

	/* Back/forward cluster: tight grouping, muted, disabled at stack ends */
	.nav-cluster {
		display: flex;
		align-items: center;
		align-self: center;
		gap: 0;
		flex-shrink: 0;
	}

	.nav-btn {
		width: 20px;
	}

	.nav-btn:disabled {
		opacity: 0.3;
		cursor: default;
	}

	.new-tab-btn {
		margin-left: 2px;
	}

	/* svelte-dnd-action drop indicator */
	:global(.tabs-scroll > [data-is-dnd-shadow-item-hint="true"]) {
		width: 2px !important;
		min-width: 2px !important;
		max-width: 2px !important;
		height: 20px !important;
		padding: 0 !important;
		margin: 0 2px;
		background: var(--color-primary) !important;
		border-radius: 1px;
		opacity: 1;
		animation: pulse 0.8s ease-in-out infinite;
	}

	@keyframes pulse {
		0%,
		100% {
			opacity: 1;
		}
		50% {
			opacity: 0.5;
		}
	}
</style>
