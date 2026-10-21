<script lang="ts">
	import { flip } from "svelte/animate";
	import { dndzone } from "svelte-dnd-action";
	import type { DndEvent } from "svelte-dnd-action";
	import Icon from "$lib/components/Icon.svelte";
	import AtlasIcon from "$lib/components/sidebar/AtlasIcon.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import type { Tab } from "$lib/stores/window-shell.svelte";
	import {
		dndManager,
		type DndTabItem,
		type ZoneId,
	} from "$lib/stores/dndManager.svelte";
	import { contextMenu } from "$lib/stores/contextMenu.svelte";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";
	import { iconPickerStore } from "$lib/stores/iconPicker.svelte";
	import { projectMenuItems, targetForTab } from "$lib/utils/projectActions";
	import { identityOf, ownedRef, renameRef, setRefLook } from "$lib/refs/identity.svelte";
	import { accentCss } from "$lib/sidebar/pin-colors";
	import { paneActions } from "$lib/stores/paneActions.svelte";
	import { modifierHint } from "$lib/stores/modifierHint.svelte";
	import { isEmoji } from "$lib/utils/iconHelpers";
	import { isAppleKeyboard } from "$lib/utils/platform";
	import { pinMenuItem, pinIconMenuItem } from "$lib/pins/pinAction";
	import type { ContextMenuItem } from "$lib/stores/contextMenu.svelte";

	const FLIP_DURATION_MS = 150;

	/**
	 * A tab shows its thing as it is now (refs/identity): a chat, page or
	 * project's own name, icon and color from the stores, with the tab's
	 * stamped label and icon only as what was last known — the first frame
	 * after a restore, or a route with no record behind it.
	 */
	function tabLook(tab: Tab) {
		return identityOf(tab.route, { title: tab.label, icon: null, color: null });
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
	// the window, and the toolbar already carries back/forward/new-tab — without
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

	// ⌘N badges, shown only while ⌘ is held. Tabs number left-to-right across
	// the whole window, so the right pane's tabs continue where the left
	// pane's leave off. Only the first nine tabs get a badge — ⌘0 and beyond
	// don't exist as shortcuts.
	const ordinalOffset = $derived(
		paneId === "right" ? (windowShellStore.leftPane?.tabs.length ?? 0) : 0,
	);
	const showPaneHint = $derived(
		modifierHint.visible && !mobileLayout.isMobile,
	);

	// The word, not the glyph. ⌘1 is four pixels of symbol in a slot the width
	// of a tab label — it read as an artefact rather than as an instruction,
	// and the space was already paid for. Keyed off the keyboard layout, not
	// the OS shell: a Mac user in a browser still presses Command.
	const modKeyWord = isAppleKeyboard ? "Command" : "Ctrl";

	function handleNewTab() {
		windowShellStore.newChat(paneId);
	}

	// Build DnD items from tabs with source information
	function buildDndItems(): DndTabItem[] {
		return tabs.map((tab) => ({
			id: tab.id,
			url: tab.route,
			label: tabLook(tab).title,
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
		const hasTabsToLeft = tabIndex > 0;

		// A chat, page or project is renamed and re-iconed as itself. Any
		// other tab has no name of its own to change: a rename here used to
		// set the tab's label and nothing else, and the view put its real
		// name back on the next load.
		const owned = ownedRef(tab.route ?? "");
		const look = tabLook(tab);

		// Build context menu items
		const items: ContextMenuItem[] = [];
		if (tab.preview) {
			items.push({
				id: "keep",
				label: "Keep open",
				icon: "ri:check-line",
				action: () => windowShellStore.keepTab(tabId),
			});
		}
		items.push(
			// Compact/Expand
			{
				id: "compact",
				label: tab.pinned ? "Expand" : "Compact",
				icon: tab.pinned
					? "ri:expand-left-right-line"
					: "ri:contract-left-right-line",
				action: () => windowShellStore.togglePin(tabId),
			},
		);
		if (owned) {
			items.push({
				id: "rename",
				label: "Rename",
				icon: "ri:edit-line",
				action: () => {
					renamingTabId = tabId;
					renameValue = look.title;
				},
			});
		}

		// One entry, not two. Icon and color are the same decision and the
		// picker holds both — a menu offering "Change icon" AND "Color" made
		// them look like separate features and sent you back through the menu
		// to finish one thought.
		if (!owned) {
			// Not a page or chat, so there's no entity icon to set — but if the
			// route is on the Desk, the PIN has an icon, and that's what the
			// tab and the sidebar row both draw.
			const pinIcon = pinIconMenuItem(tab.route, {
				x: e.clientX,
				y: e.clientY,
				width: 0,
				height: 0,
			});
			if (pinIcon) items.push(pinIcon);
		}

		// Change icon: the thing's own icon and color, set everywhere it shows.
		if (owned) {
			items.push({
				id: "change-icon",
				label: "Change icon",
				icon: "ri:emotion-line",
				action: () => {
					iconPickerStore.show(
						look.icon,
						(icon) => {
							setRefLook(tab.route, { icon }).catch((err) =>
								console.error("[WindowTabBar] Failed to change icon:", err),
							);
						},
						{
							anchor: { x: e.clientX, y: e.clientY, width: 0, height: 0 },
							color: look.color,
							// Persisted on click, without closing — so you can
							// try a color against the grid before choosing the
							// glyph, which is the point of one panel.
							onColorSelect: (color) => {
								setRefLook(tab.route, { color }).catch((err) =>
									console.error("[WindowTabBar] Failed to change icon color:", err),
								);
							},
						},
					);
				},
			});
		}


		// The menu way to split: dragging a tab to a pane's edge does the same,
		// and moving a side's last tab across makes the window whole again.
		// A lone tab has nothing to sit beside, so it gets no entry.
		if (!mobileLayout.isMobile && (isSplitMode || tabs.length > 1)) {
			const toRight = paneId !== "right";
			items.push({
				id: "move-across",
				label: toRight ? "Move to right side" : "Move to left side",
				icon: toRight ? "ri:layout-right-line" : "ri:layout-left-line",
				action: () => windowShellStore.moveTabAcross(tabId),
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

		if (hasTabsToLeft) {
			items.push({
				id: "close-to-left",
				label: "Close to the Left",
				action: () => windowShellStore.closeTabsToLeft(tabId, paneId),
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

		// File it in a project (an unsent chat files its draft), and pin it
		if (tab.route) {
			items.push(...projectMenuItems(targetForTab(tab)));
			// Anything you can open, you can keep. The tab is the one surface
			// that exists for every route in the app, so wiring the pin here
			// makes the Desk reachable from everywhere by construction rather
			// than by remembering to add a menu item per view.
			items.push(
				pinMenuItem({ url: tab.route, label: look.title, icon: tab.icon }),
			);
		}

		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}

	/**
	 * Commit a tab rename — to the thing, through the one rename path
	 * (refs/identity), so the sidebar, pins and every other tab on it move
	 * together. The tab shows the new name at once because the stores do.
	 */
	function handleRenameSubmit() {
		if (!renamingTabId || !renameValue.trim()) {
			handleRenameCancel();
			return;
		}
		const newLabel = renameValue.trim();
		const tab = tabs.find((t) => t.id === renamingTabId);
		renamingTabId = null;
		renameValue = "";
		if (!tab?.route || !ownedRef(tab.route)) return;
		renameRef(tab.route, newLabel).catch((e) => console.error("[WindowTabBar] rename failed:", e));
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

	// On a preview tab a double-click keeps it, as in VS Code. On a kept tab
	// it renames the thing, when the thing has a name of its own.
	function handleDoubleClick(e: MouseEvent, tabId: string) {
		e.preventDefault();
		const tab = tabs.find((t) => t.id === tabId);
		if (tab?.preview) {
			windowShellStore.keepTab(tabId);
			return;
		}
		if (!tab || tab.pinned || !ownedRef(tab.route ?? "")) return;
		renamingTabId = tabId;
		renameValue = tabLook(tab).title;
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
	role="toolbar"
	aria-label="Tab bar"
	tabindex="0"
>


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
		{#each dndItems as item, tabIndex (item.id)}
			{@const tab = item.tab}
			{@const tabOrdinal = ordinalOffset + tabIndex + 1}
			{@const identity = tabLook(tab)}
			{@const glyph = identity.icon || item.icon || getDefaultIcon(tab.type)}
			<div
				class="tab"
				class:active={tab.id === activeTabId}
				class:active-in-active-pane={tab.id === activeTabId &&
					isActivePane}
				class:pinned={tab.pinned}
				class:preview={tab.preview}
				class:renaming={tab.id === renamingTabId}
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
				title={tab.id !== renamingTabId
					? tab.preview
						? `${identity.title} · Preview. Double-click to keep it open.`
						: identity.title
					: ""}
				role="button"
				tabindex="0"
			>
				<!-- The thing's own icon wins over the tab's stamped one, which
				     wins over the species default. A pinned tab used to be
				     filled edge to edge with its bookcloth; that read as a
				     selected state and shouted down the actual selection, so
				     identity now lives in the icon — which is where the user
				     put it. -->
				{#if glyph && isEmoji(glyph)}
					<span class="tab-emoji">{glyph}</span>
				{:else}
					<Icon
						icon={glyph}
						class="tab-icon"
						style={identity.color ? `color: ${accentCss(identity.color)}` : undefined}
					/>
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
						<!-- On a ⌘ hold each tab's own label becomes its number:
						     the title slides up and out, ⌘N slides up and in.
						     Nothing new enters the screen, which is the point —
						     the old version floated a saturated blue chip into the
						     toolbar, the loudest object in an otherwise quiet
						     window, and it announced the shortcut instead of
						     teaching which label it belongs to. Every tab flips
						     because every tab has its own number, browser-style:
						     leftmost is ⌘1, counting across both panes. -->
						<span class="tab-label" class:flipping={showPaneHint && tabOrdinal <= 9}>
							<span class="tab-label-text">{identity.title}</span>
							<span class="tab-label-hint" aria-hidden="true">{modKeyWord} + {tabOrdinal}</span>
						</span>
					{/if}
				{/if}
				{#if !tab.pinned && tab.id !== renamingTabId}
					<button
						class="tab-close"
						onclick={(e) => handleTabClose(e, tab.id)}
						aria-label="Close tab"
					>
						<AtlasIcon name="close" size={14} stroke={1.25} bare />
					</button>
				{/if}
			</div>
		{/each}
	</div>

	<button
		class="new-tab-btn"
		onclick={handleNewTab}
		aria-label="New chat"
		title="New chat"
	>
		<AtlasIcon name="plus" size={16} stroke={1.25} bare />
	</button>

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
		height: var(--chrome-row-h);
		align-items: center;
		/* The strip sits in the window header, on the desk ground above the
		   card, so it paints nothing. Pills carry the state. */
		background: transparent;
		flex-shrink: 0;
		position: relative;
		z-index: var(--z-overlay); /* Above global drag overlays */
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
	   the text color, so it is legible in all sixteen themes. */
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

	/* The `.clothed` rules that lived here are gone with the fill. A pinned
	   tab used to take its pin's bookcloth as a solid background, which meant
	   the loudest tab in the bar was whichever one you had pinned rather than
	   whichever one you were looking at — it read as permanently selected.
	   Identity moved to the icon, which the user picks and can color. */

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

	/* A preview tab: the one the next ordinary open replaces. Italic, as in
	   VS Code, so you can tell it will not stay. */
	.tab.preview .tab-label-text {
		font-style: italic;
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

	/* Same face, size AND weight as the label it replaces. It was monospace,
	   then semibold — both decoration standing in for meaning. The shortcut
	   isn't code and it isn't emphasis; it's the same label saying something
	   else for as long as you hold the key, so it should look like the label. */
	.tab-label-hint {
		position: absolute;
		inset: 0;
		transform: translateY(100%);
		font-weight: inherit;
		color: inherit;
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
		background: var(--hover-bg);
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
		background: color-mix(in srgb, var(--wash-ink) 10%, transparent);
		color: var(--color-foreground);
	}

	.tab-close:active {
		background: var(--press-bg);
	}

	.tab-close:focus-visible {
		opacity: 1;
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

	.new-tab-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		/* The same box as a tab, so the + reads as the next tab's place. */
		align-self: center;
		width: 28px;
		height: 28px;
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

	/* Hover used to be `--color-surface-elevated` over a `--color-background`
	   toolbar — #F5F4EF on #FDFCF9 in the default theme, a ~3% shift that read
	   as nothing happening. A foreground mix is legible in every theme instead
	   of depending on two surface tokens staying far enough apart. */
	.new-tab-btn:hover {
		background: var(--hover-bg);
		color: var(--color-foreground);
	}

	.new-tab-btn:active {
		background: var(--press-bg);
	}

	.new-tab-btn:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
		color: var(--color-foreground);
	}

	.new-tab-btn {
		margin-left: 4px;
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
