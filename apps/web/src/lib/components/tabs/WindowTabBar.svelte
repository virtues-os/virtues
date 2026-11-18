<script lang="ts">
	import { flip } from "$lib/utils/transitions";
	import { reducedMotion } from "$lib/stores/motion.svelte";
	import { onMount, untrack } from "svelte";
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
	import { accentCss, clothFor } from "$lib/sidebar/pin-colors";
	import { modifierHint } from "$lib/stores/modifierHint.svelte";
	import { isEmoji } from "$lib/utils/iconHelpers";
	import { isAppleKeyboard } from "$lib/utils/platform";
	import { pinMenuItem, pinIconMenuItem } from "$lib/pins/pinAction";
	import type { ContextMenuItem } from "$lib/stores/contextMenu.svelte";
	import { newTabMenuItems } from "$lib/utils/newActions";
	import { canBrowserPane } from "$lib/tauri/bridge";
	import { chatActivity } from "$lib/stores/chatActivity.svelte";
	import { routeToEntityId } from "$lib/utils/urlUtils";
	import ThinkingMark from "$lib/components/ThinkingMark.svelte";
	import TabHoverCard, { type TabCardAction } from "./TabHoverCard.svelte";
	import { isPinned, togglePin } from "$lib/pins/pinAction";
	import { cubicBezier, easePremium, motionMs } from "$lib/utils/motion";

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
	// Tabs shrink together as they fill the strip. Once each would get less
	// than COMPACT_TAB_PX, the tabs you are not on drop their labels and show
	// only their icon, as a browser's do.
	const COMPACT_TAB_PX = 80;
	/** The strip's room besides its tabs: the + and the gaps. */
	const STRIP_RESERVE_PX = 120;
	let barWidth = $state(0);
	const tabsCompact = $derived(
		barWidth > 0 && dndItems.length > 1 && (barWidth - STRIP_RESERVE_PX) / dndItems.length < COMPACT_TAB_PX,
	);

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

	// The + starts a chat. When the pane already has an empty one, that would
	// only bring it to the front, so the + offers every kind of new tab
	// instead. A right-click on the + always offers them.
	const hasEmptyChat = $derived(!!windowShellStore.emptyChatIn(paneId));
	let browserAvailable = $state(false);
	onMount(() => {
		void canBrowserPane().then((ok) => (browserAvailable = ok));
	});

	let newTabMenuOpen = $state(false);
	$effect(() => {
		if (!contextMenu.visible) newTabMenuOpen = false;
	});

	function handleNewTab(e: MouseEvent) {
		if (hasEmptyChat) showNewTabMenu(e);
		else windowShellStore.newChat(paneId);
	}

	function showNewTabMenu(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
		contextMenu.show({ x: rect.left, y: rect.bottom }, newTabMenuItems(paneId, { browser: browserAvailable }), {
			anchor: { x: rect.left, y: rect.top, width: rect.width, height: rect.height },
			placement: "bottom-start",
		});
		newTabMenuOpen = true;
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

	// Sync DnD items when tabs change. Not while a tab is in the air: the
	// library's placeholder lives in this list, and a chat naming itself mid
	// drag would wipe it. The drop rebuilds the list anyway.
	$effect(() => {
		const items = buildDndItems();
		if (untrack(() => dndManager.session)) return;
		dndItems = items;
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
		items.push(
			// Compact/Expand
			{
				id: "compact",
				label: tab.pinned ? "Expand" : "Compact",
				icon: tab.pinned
					? "mynaui:move-horizontal"
					: "mynaui:move-horizontal",
				action: () => windowShellStore.togglePin(tabId),
			},
		);
		if (owned) {
			items.push({
				id: "rename",
				label: "Rename",
				icon: "mynaui:edit",
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
				icon: "mynaui:smile",
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
							label: look.title,
							autoColor: clothFor({ url: tab.route }),
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
				icon: toRight ? "mynaui:panel-right" : "mynaui:panel-left",
				action: () => windowShellStore.moveTabAcross(tabId),
			});
		}

		// Divider + Close actions. The window's only tab, an empty new chat,
		// has nothing to close to (canClose).
		if (windowShellStore.canClose(tabId)) {
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
		}

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
		if (newLabel === tabLook(tab).title) return;
		void renameRef(tab.route, newLabel);
	}

	// `autofocus` only applies on page load, so a field mounted later has to
	// be focused by hand. Selecting the title lets you type over it.
	function focusAndSelect(node: HTMLInputElement) {
		node.focus();
		node.select();
	}


	/** `--ease-in-out-quad`. */
	const easeInOut = cubicBezier(0.45, 0, 0.55, 1);

	/** Progress `p` re-mapped onto the stretch [from, to], eased. */
	function phase(p: number, from: number, to: number, ease = easePremium): number {
		return ease(Math.min(1, Math.max(0, (p - from) / (to - from))));
	}

	/**
	 * A tab's box at its full size, for the open and close to run between:
	 * its width, its padding, and the strip gap on the side it takes (the
	 * first tab has none before it, so it takes the one after).
	 */
	function tabBox(node: HTMLElement) {
		const style = getComputedStyle(node);
		return {
			width: node.getBoundingClientRect().width,
			padLeft: parseFloat(style.paddingLeft),
			padRight: parseFloat(style.paddingRight),
			gap: parseFloat(getComputedStyle(node.parentElement!).columnGap) || 0,
			side: node.previousElementSibling ? "left" : "right",
		};
	}

	/** The box at `size` of its full width (0..1). */
	function boxCss(box: ReturnType<typeof tabBox>, size: number): string {
		return (
			`max-width: ${size * box.width}px; padding-left: ${size * box.padLeft}px; ` +
			`padding-right: ${size * box.padRight}px; margin-${box.side}: ${(size - 1) * box.gap}px;`
		);
	}

	/**
	 * Opening and closing are one motion run both ways, on the scale's
	 * layout timing (300ms on --ease-premium), in two beats that overlap a
	 * little, so one hands off to the other. Opening: the space opens first, so the tabs to the right
	 * and the + glide aside, then the tab rises one radius out of the card
	 * and fades in. The active tab is the card's color, so it reads as the
	 * page lifting into a tab. Closing: the tab sinks back toward the card
	 * and fades, then the space closes and its neighbours glide in. Nothing
	 * is squeezed while it can be seen: the tab is invisible before its box
	 * gets narrow, and the box is full size before it becomes visible.
	 */
	const TAB_MOTION_MS = motionMs("--motion-layout", 300);
	/** Tabs still opening, which the strip waits for before scrolling to one. */
	const unfolding = new WeakSet<Element>();
	/**
	 * Instant while a tab is in the air: the drag library adds and removes
	 * its placeholder as the pointer crosses zones, and it measures the
	 * strip as it goes.
	 */
	const tabMotionMs = () => (reducedMotion() || dndManager.session ? 0 : TAB_MOTION_MS);
	const RISE_PX = 8;
	const SINK_PX = 4;

	function unfold(node: HTMLElement) {
		const box = tabBox(node);
		unfolding.add(node);
		node.addEventListener("introend", () => unfolding.delete(node), { once: true });
		return {
			duration: tabMotionMs(),
			css: (t: number) => {
				// The fade is shorter than the rise: half see-through, the
				// tab showed the card's top edge through it.
				const size = phase(t, 0, 0.55);
				const show = phase(t, 0.3, 0.75, easeInOut);
				const rise = phase(t, 0.3, 1);
				return (
					boxCss(box, size) +
					` min-width: 0; overflow: ${t < 0.3 ? "hidden" : "visible"};` +
					` opacity: ${show}; translate: 0 ${(1 - rise) * RISE_PX}px;`
				);
			},
		};
	}

	/** Every way to close a tab ends here: it runs when the tab leaves the list. */
	function foldAway(node: HTMLElement) {
		// For a list with animate:, Svelte takes a leaving item out of the
		// flow (position: absolute) just before this runs, so the others can
		// slide over it. The tab closes its own space instead, so it goes
		// back in the flow.
		if (node.style.position === "absolute") {
			node.style.position = "";
			node.style.width = "";
			node.style.height = "";
			node.style.transform = "";
		}
		node.classList.add("closing");
		const box = tabBox(node);
		return {
			duration: tabMotionMs(),
			css: (t: number) => {
				// Svelte runs an outro from t = 1 down to 0.
				const p = 1 - t;
				// The fade eases in and out: --ease-premium front-loads it, and
				// a third of the tab vanished in the first frame or two. The
				// space starts closing as the fade ends, so no frame holds
				// an invisible tab at full width.
				const hide = phase(p, 0, 0.4, easeInOut);
				const size = 1 - phase(p, 0.2, 1);
				return (
					boxCss(box, size) +
					` min-width: 0; overflow: ${p > 0.35 ? "hidden" : "visible"};` +
					` opacity: ${1 - hide}; translate: 0 ${hide * SINK_PX}px;`
				);
			},
		};
	}

	/**
	 * svelte-dnd-action maps each child element to an item by its index,
	 * and it last counted while the closed tab was still folding in the
	 * strip, so every tab after it was one off. A fresh list makes the
	 * library count again.
	 */
	function recountAfterFold() {
		if (!dndManager.session) dndItems = [...dndItems];
	}

	/**
	 * A strip that scrolls brings the tab you switch to into view (⌘9, a
	 * link that opens an existing tab), as a browser's does, clear of the
	 * strip's edges (its scroll-padding). A tab that is still opening
	 * is 0px wide, so the strip waits for it to finish opening first.
	 */
	let scroller = $state<HTMLElement | null>(null);
	$effect(() => {
		const id = activeTabId;
		if (!scroller || !id) return;
		requestAnimationFrame(() => {
			const el = scroller?.querySelector<HTMLElement>(`[data-tab-id="${CSS.escape(id)}"]`);
			if (!el) return;
			const reveal = () =>
				el.scrollIntoView({ block: "nearest", inline: "nearest", behavior: reducedMotion() ? "auto" : "smooth" });
			if (unfolding.has(el)) el.addEventListener("introend", reveal, { once: true });
			else reveal();
		});
	});

	/** The chat a tab shows, for its spinner and unread dot. */
	function chatOf(tab: Tab): string | null {
		return tab.type === "chat" ? routeToEntityId(tab.route) : null;
	}

	/**
	 * Marks a label whose title does not fit, so it fades out at its end as
	 * a browser tab's does. An ellipsis spends three letters' room saying
	 * the same thing. Re-measured when the tab resizes or its title changes.
	 */
	function fadeWhenClipped(node: HTMLElement, _title: string) {
		// The title's own line clips itself, so it is the one that overflows.
		const measure = () => {
			const text = node.firstElementChild as HTMLElement | null;
			node.classList.toggle("clipped", !!text && text.scrollWidth > text.clientWidth + 1);
		};
		const observer = new ResizeObserver(measure);
		observer.observe(node);
		measure();
		return {
			update: () => requestAnimationFrame(measure),
			destroy: () => observer.disconnect(),
		};
	}

	/**
	 * The card under a tab (TabHoverCard): the whole title and the tab's
	 * actions. It waits CARD_DELAY_MS so passing along the strip shows
	 * nothing. Once one has shown, the next tab's opens at once, as the
	 * tooltip does. Leaving the tab gives you CARD_GRACE_MS to reach the card.
	 */
	const CARD_DELAY_MS = 700;
	const CARD_GRACE_MS = 150;
	let cardTabId = $state<string | null>(null);
	let cardAnchor = $state<HTMLElement | null>(null);
	let cardTimer: ReturnType<typeof setTimeout> | undefined;

	function armCard(e: PointerEvent, tabId: string) {
		if (e.pointerType === "touch" || mobileLayout.isMobile) return;
		if (dndManager.session || renamingTabId || contextMenu.visible) return;
		const node = e.currentTarget as HTMLElement;
		clearTimeout(cardTimer);
		const open = () => {
			cardTabId = tabId;
			cardAnchor = node;
		};
		if (cardTabId) open();
		else cardTimer = setTimeout(open, CARD_DELAY_MS);
	}

	function leaveCard() {
		clearTimeout(cardTimer);
		cardTimer = setTimeout(closeCard, cardTabId ? CARD_GRACE_MS : 0);
	}

	function keepCard() {
		clearTimeout(cardTimer);
	}

	function closeCard() {
		clearTimeout(cardTimer);
		cardTimer = undefined;
		cardTabId = null;
		cardAnchor = null;
	}

	// A drag, a rename, a menu, or the tab closing takes the card away.
	$effect(() => {
		if (dndManager.session || renamingTabId || contextMenu.visible) closeCard();
	});
	$effect(() => {
		if (cardTabId && !tabs.some((t) => t.id === cardTabId)) closeCard();
	});
	onMount(() => () => clearTimeout(cardTimer));

	const cardTab = $derived(cardTabId ? tabs.find((t) => t.id === cardTabId) : undefined);

	function cardActions(tab: Tab): TabCardAction[] {
		const actions: TabCardAction[] = [];
		const look = tabLook(tab);
		const act = (run: () => void) => () => {
			closeCard();
			run();
		};
		if (tab.route) {
			const pinned = isPinned(tab.route);
			actions.push({
				id: "pin",
				label: pinned ? "Unpin" : "Pin",
				icon: pinned ? "mynaui:pin-solid" : "mynaui:pin",
				on: pinned,
				run: act(() => {
					togglePin({ url: tab.route, label: look.title, icon: tab.icon }).catch((err) =>
						console.error("[WindowTabBar] pin failed:", err),
					);
				}),
			});
		}
		if (!tab.pinned && ownedRef(tab.route ?? "")) {
			actions.push({
				id: "rename",
				label: "Rename",
				icon: "mynaui:edit",
				run: act(() => {
					renamingTabId = tab.id;
					renameValue = look.title;
				}),
			});
		}
		actions.push({
			id: "compact",
			label: tab.pinned ? "Expand" : "Compact",
			icon: "mynaui:move-horizontal",
			run: act(() => windowShellStore.togglePin(tab.id)),
		});
		if (isSplitMode || tabs.length > 1) {
			const toRight = paneId !== "right";
			actions.push({
				id: "move-across",
				label: toRight ? "Move to right side" : "Move to left side",
				icon: toRight ? "mynaui:panel-right" : "mynaui:panel-left",
				run: act(() => windowShellStore.moveTabAcross(tab.id)),
			});
		}
		return actions;
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

	// A double-click renames the thing, when the thing has a name of its own.
	function handleDoubleClick(e: MouseEvent, tabId: string) {
		e.preventDefault();
		const tab = tabs.find((t) => t.id === tabId);
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

	async function handleDndFinalize(e: CustomEvent<DndEvent<DndTabItem>>) {
		await dndManager.handleFinalize(e, zoneId, (items) => {
			dndItems = items;
		});
		// The library took the tab out of this strip's list while it was
		// dragged. A drop that changed nothing leaves the store as it was, so
		// rebuild from the store or the tab stays missing from the strip.
		dndItems = buildDndItems();
	}

	/** The dragged copy of a tab, lifted off the strip (styles below). */
	function liftDraggedTab(el: HTMLElement | undefined) {
		el?.classList.add("tab-lifted");
	}

	// Get icon for tab type
	function getDefaultIcon(type: string): string {
		switch (type) {
			case "chat":
				return "mynaui:chat";
			case "history":
				return "mynaui:clock-circle";
			case "wiki":
				return "mynaui:book";
			case "wiki-list":
				return "mynaui:list-check";
			case "data-sources":
				return "mynaui:database";
			case "data-sources-add":
				return "mynaui:plus-circle";
			case "data-jobs":
				return "mynaui:refresh";
			case "storage":
				return "mynaui:hard-drive";
			case "usage":
				return "mynaui:chart-bar";
			case "profile":
				return "mynaui:user-settings";
			default:
				return "mynaui:file";
		}
	}
</script>

<div
	data-tauri-drag-region
	class="tab-bar"
	class:compact={tabsCompact}
	bind:clientWidth={barWidth}
	role="toolbar"
	aria-label="Tab bar"
	tabindex="0"
>


	<div
		data-tauri-drag-region
		class="tabs-scroll"
		bind:this={scroller}
		role="tablist"
		tabindex="0"
		use:dndzone={{
			items: dndItems,
			type: "tab",
			flipDurationMs: FLIP_DURATION_MS,
			dropTargetStyle: {},
			dragDisabled: renamingTabId !== null,
			// The dragged tab keeps its own shape. Morphing copies the
			// styles of the hidden placeholder it is over, which left only
			// the label showing.
			morphDisabled: true,
			transformDraggedElement: liftDraggedTab,
		}}
		onconsider={handleDndConsider}
		onfinalize={handleDndFinalize}
	>
		{#each dndItems as item, tabIndex (item.id)}
			{@const tab = item.tab}
			{@const tabOrdinal = ordinalOffset + tabIndex + 1}
			{@const identity = tabLook(tab)}
			{@const glyph = identity.icon || item.icon || getDefaultIcon(tab.type)}
			{@const chatId = chatOf(tab)}
			<div
				class="tab"
				class:active={tab.id === activeTabId}
				class:lq={tab.id === activeTabId}
				class:lq-selected={tab.id === activeTabId}
				class:only={dndItems.length === 1}
				class:active-in-active-pane={tab.id === activeTabId &&
					isActivePane}
				class:pinned={tab.pinned}
				class:renaming={tab.id === renamingTabId}
				data-tab-id={tab.id}
				animate:flip={{ duration: FLIP_DURATION_MS }}
				in:unfold
				out:foldAway
				onoutroend={recountAfterFold}
				onclick={() =>
					tab.id !== renamingTabId && handleTabClick(tab.id)}
				onpointerenter={(e) => armCard(e, tab.id)}
				onpointerleave={leaveCard}
				onpointerdown={closeCard}
				ondblclick={(e) => handleDoubleClick(e, tab.id)}
				onauxclick={(e) => handleMiddleClick(e, tab.id)}
				oncontextmenu={(e) => handleContextMenu(e, tab.id)}
				onkeydown={(e) =>
					e.key === "Enter" &&
					tab.id !== renamingTabId &&
					handleTabClick(tab.id)}
				role="button"
				tabindex="0"
			>
				<!-- The thing's own icon wins over the tab's stamped one, which
				     wins over the species default. A pinned tab used to be
				     filled edge to edge with its bookcloth; that read as a
				     selected state and shouted down the actual selection, so
				     identity now lives in the icon — which is where the user
				     put it. -->
				<!-- A chat that is replying shows the ∴ working in place of
				     its icon, where a browser tab shows its page loading. A
				     reply that landed while the tab was in the background
				     leaves the sidebar's unread dot on the icon until you
				     open it. -->
				<span class="tab-glyph">
					{#if chatId && chatActivity.running(chatId)}
						<span class="tab-running" role="img" aria-label="Replying">
							<ThinkingMark depth={3} size={14} />
						</span>
					{:else if glyph && isEmoji(glyph)}
						<span class="tab-emoji">{glyph}</span>
					{:else}
						<Icon
							icon={glyph}
							class="tab-icon"
							style={identity.color ? `color: ${accentCss(identity.color)}` : undefined}
						/>
					{/if}
					{#if chatId && tab.id !== activeTabId && chatActivity.unread(chatId)}
						<span class="tab-unread" role="img" aria-label="New reply"></span>
					{/if}
				</span>
				{#if !tab.pinned}
					{#if tab.id === renamingTabId}
						<span class="rename-sizer" data-value={renameValue}>
							<input
								type="text"
								class="rename-input"
								size="1"
								aria-label="Name"
								bind:value={renameValue}
								onkeydown={handleRenameKeydown}
								onblur={handleRenameSubmit}
								onclick={(e) => e.stopPropagation()}
								use:focusAndSelect
							/>
						</span>
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
						<span
							class="tab-label"
							class:flipping={showPaneHint && tabOrdinal <= 9}
							use:fadeWhenClipped={identity.title}
						>
							<span class="tab-label-text" data-tip-text>{identity.title}</span>
							<span class="tab-label-hint" aria-hidden="true">{modKeyWord} + {tabOrdinal}</span>
						</span>
					{/if}
				{/if}
				{#if !tab.pinned && windowShellStore.canClose(tab.id)}
					<button
						class="tab-close"
						onclick={(e) => handleTabClose(e, tab.id)}
						aria-label="Close tab"
					>
						<AtlasIcon name="close" size={14} bare />
					</button>
				{/if}
			</div>
		{/each}
	</div>

	<button
		class="new-tab-btn"
		class:open={newTabMenuOpen}
		onclick={handleNewTab}
		oncontextmenu={showNewTabMenu}
		aria-label={hasEmptyChat ? "New tab" : "New chat"}
		aria-haspopup="menu"
		aria-expanded={newTabMenuOpen}
		data-tip={newTabMenuOpen ? undefined : hasEmptyChat ? "New tab" : "New chat"}
		data-tip-key={hasEmptyChat ? undefined : "mod+n"}
	>
		<AtlasIcon name="plus" size={16} bare />
	</button>

	{#if cardTab && cardAnchor}
		<TabHoverCard
			anchor={cardAnchor}
			title={tabLook(cardTab).title}
			actions={cardActions(cardTab)}
			onenter={keepCard}
			onleave={leaveCard}
		/>
	{/if}


</div>

<style>
	.tab-bar {
		container-type: inline-size;
		container-name: tabbar;
		display: flex;
		gap: 4px;
		/* The first tab starts on the pane's left edge, so the strip lines
		   up with the pane below it, and the tabs sit on the row's centre
		   line, level with the ∴ and the corner. The strip is on the
		   painting and paints nothing. */
		padding: 0 8px 0 0;
		height: var(--chrome-row-h);
		align-items: center;
		background: transparent;
		flex-shrink: 0;
		position: relative;
		z-index: var(--z-overlay); /* Above global drag overlays */
	}

	.tabs-scroll {
		display: flex;
		align-items: center;
		gap: 4px;
		overflow-x: auto;
		/* Only as wide as its tabs, so the + sits right after the last one,
		   where the next tab will appear (Brave, Safari). It shrinks and
		   scrolls once the tabs outgrow the strip, and the + stays in view. */
		flex: 0 1 auto;
		min-width: 0;
		scrollbar-width: none;
		/* A scroller clips on both axes, and the selected tab's glass casts a
		   soft lift past its edges. The padding is that room, and the
		   negative margin gives it back to the layout. */
		align-self: stretch;
		padding: 0 4px;
		margin: 0 -4px;
		scroll-padding-inline: 4px;
		/* Only the tabs take the pointer, so the gaps between tabs still move
		   the window. */
		pointer-events: none;
	}

	.tabs-scroll > :global(*) {
		pointer-events: auto;
	}

	.tabs-scroll::-webkit-scrollbar {
		display: none;
	}

	/* A tab is a pill on the painting. The one you are on is glass
	   (`.lq.lq-selected`, app.css); the others are their icon and label,
	   with a wash under the pointer. */
	.tab {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 0 12px;
		position: relative;
		height: var(--chrome-tab-h);
		border: none;
		border-radius: var(--radius-lg);
		/* No background of its own: the tab you are on takes the glass's
		   (`.lq`), which a background here would override. */
		color: var(--color-foreground-muted);
		/* The sidebar rows' size, so the header and the column read as one. */
		font-size: 13px;
		cursor: pointer;
		white-space: nowrap;
		/* Every tab is the same width whatever its title, so the strip reads
		   as a row of equal slots and a tab does not jump when its chat names
		   itself. The title fades where it is cut and the hover card shows it
		   whole. In a crowded strip the tabs shrink together, down to their
		   icon. */
		flex: 0 1 200px;
		width: 200px;
		min-width: 36px;
		/* Switching tabs moves the glass at once, as in a browser: the glass,
		   its edge and the ink change in one frame (`.tab.active`). Only the
		   hover wash fades. Opening and closing a tab are transitions
		   (unfold, foldAway), so the tabs already open when the window loads
		   do not animate in. */
		transition:
			background-color var(--motion-hover-out) var(--ease-premium),
			color var(--motion-hover-out) var(--ease-premium),
			transform var(--motion-release) var(--ease-glass);
	}

	.tab:hover {
		color: var(--color-foreground);
	}

	.tab:not(.active):hover {
		background-color: var(--hover-bg);
		transition-duration: var(--motion-hover-in), var(--motion-hover-in), var(--motion-release);
	}

	/* Rows and tabs take the press fill (design.md, "Grays"). The glass of
	   the tab you are on sinks a little instead, and springs back. */
	.tab:not(.active):active {
		background-color: var(--press-bg);
		transition-duration: var(--motion-press);
	}

	.tab.active:active {
		transform: scale(0.97);
		transition-duration: 0s, 0s, var(--motion-press);
	}

	/* The tab you are on: full ink on glass. Never marked by weight: a
	   bolder label is a wider one, and every tab to its right moved when you
	   switched. */
	.tab.active {
		/* The tab you are on keeps its full width in a crowded strip; the
		   others compact to their icons and then scroll. Squeezed to its
		   minimum, it lost its label and its × ran into the next tab. */
		flex-shrink: 0;
		color: var(--color-foreground);
		/* Lit in one frame when you switch to it; the page under it is what
		   arrives ($lib/tabs/arrive). */
		transition-duration: 0s, 0s, var(--motion-release);
	}

	/* A closing tab takes no pointer, so the tab gliding in under it does.
	   It keeps its look while it fades: the tab that takes over is active
	   from the first frame, and the two cross-fade. */
	.tab:global(.closing) {
		pointer-events: none;
	}

	/* In split view two tabs are "active" at once and only one takes your
	   typing. Both stay joined to their cards, as each is the top of its
	   own page. The one in the pane you are not in dims its ink, so the
	   focused pane's tab is the only one that reads as selected. */
	.tab.active:not(.active-in-active-pane) {
		color: var(--color-foreground-muted);
	}

	.tab.active:not(.active-in-active-pane) :is(:global(.tab-icon), .tab-label) {
		opacity: 0.85;
	}

	/* The `.clothed` rules that lived here are gone with the fill. A pinned
	   tab used to take its pin's bookcloth as a solid background, which meant
	   the loudest tab in the bar was whichever one you had pinned rather than
	   whichever one you were looking at — it read as permanently selected.
	   Identity moved to the icon, which the user picks and can color. */

	/* The dragged tab: the whole pill, lifted off the strip. It takes the
	   active tab's fill whichever tab it was, tilts a little and rises on a
	   soft shadow. Over a pane it can be dropped into it straightens and
	   takes the accent edge (SplitContainer sets .tab-docking). The library
	   copies the source's computed styles inline, hence !important. */
	:global(.tab.tab-lifted) {
		background: var(--color-page) !important;
		border-color: transparent !important;
		border-radius: var(--radius-lg) !important;
		color: var(--color-foreground) !important;
		opacity: 1 !important;
		/* design-ok: a dragged tab floats above the window; the shadow is its lift */
		box-shadow: inset 0 0 0 1px var(--card-edge), 0 10px 24px -8px rgb(0 0 0 / 0.28), 0 2px 6px -2px rgb(0 0 0 / 0.12) !important;
		scale: 1.06;
		rotate: -2deg;
		animation: -global-tab-lift var(--motion-enter) var(--ease-premium) both !important;
	}

	/* Nothing on a tab in the air can be clicked, and the label gets the
	   close button's room in the copy's fixed width. */
	:global(.tab.tab-lifted .tab-close) {
		display: none !important;
	}

	:global(.tab.tab-lifted.tab-docking) {
		/* design-ok: a dragged tab floats above the window; the shadow is its lift */
		box-shadow: inset 0 0 0 1.5px color-mix(in srgb, var(--color-primary) 70%, transparent), 0 6px 16px -8px rgb(0 0 0 / 0.24) !important;
		scale: 1;
		rotate: 0deg;
		animation: -global-tab-dock var(--motion-state) var(--ease-premium) both !important;
	}

	/* Let go over the sidebar: the tab fades where it was dropped, since the
	   library would otherwise fly it home as if the drop were refused
	   (dndManager `release`). */
	:global(.tab.tab-lifted.tab-landed) {
		animation: -global-tab-land var(--motion-hover-in) var(--ease-premium) forwards !important;
	}

	@keyframes -global-tab-land {
		to {
			opacity: 0;
			scale: 0.9;
		}
	}

	@keyframes -global-tab-lift {
		from {
			scale: 1;
			rotate: 0deg;
		}
	}

	@keyframes -global-tab-dock {
		from {
			scale: 1.06;
			rotate: -2deg;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		:global(.tab.tab-lifted),
		:global(.tab.tab-lifted.tab-docking) {
			rotate: 0deg;
			scale: 1;
			animation: none !important;
		}

		:global(.tab.tab-lifted.tab-landed) {
			opacity: 0 !important;
		}
	}

	/* A compacted tab — the "Compact / Expand" context action, which shrinks a
	   tab to its icon. Nothing to do with sidebar pins despite the class name.

	   It used to be filled with --color-primary at 15% and its icon set to the
	   accent outright, which made a merely-narrow tab the most saturated object
	   in the window — a theme accent asserting meaning where the only fact is
	   "this tab is short". Narrow is legible from being narrow. */
	.tab.pinned {
		flex: 0 0 auto;
		width: auto;
		min-width: auto;
		max-width: none;
		padding: 0 8px;
		gap: 0;
	}

	:global(.tab-icon) {
		flex-shrink: 0;
		font-size: 14px;
		opacity: 0.7;
	}

	.tab.active :global(.tab-icon) {
		opacity: 1;
	}

	.tab-glyph {
		position: relative;
		display: flex;
		flex-shrink: 0;
	}

	.tab-running {
		display: flex;
		color: var(--color-foreground-muted);
	}

	/* The sidebar row's unread dot (HomePanel), on the icon's corner as a
	   browser badges a tab. The desk-colored ring keeps it off the glyph.
	   It arrives once and then holds still. */
	.tab-unread {
		position: absolute;
		top: -2px;
		right: -3px;
		width: 6px;
		height: 6px;
		border-radius: var(--radius-full);
		background: var(--color-primary);
		/* design-ok: the dot's cut-out ring in the desk's color, not depth */
		box-shadow: 0 0 0 2px var(--desk);
		animation: unread-in var(--motion-enter) var(--ease-premium);
	}

	@keyframes unread-in {
		from {
			transform: scale(0.6);
			opacity: 0;
		}
	}

	/* A crowded strip: the tabs you are not on show only their icon. */
	.tab-bar.compact .tab:not(.active) {
		padding: 0 10px;
		gap: 0;
	}

	.tab-bar.compact .tab:not(.active) .tab-label,
	.tab-bar.compact .tab:not(.active):not(:hover) .tab-close {
		display: none;
	}

	/* An odometer: two lines stacked in a one-line window, shifted by 100% to
	   swap which one shows. The hint is neutral text, not an accent chip —
	   nothing in this shell should be more saturated than the work. */
	.tab-label {
		position: relative;
		min-width: 0;
		overflow: hidden;
		flex: 1;
		text-align: left;
		height: 1.35em;
		line-height: 1.35em;
	}

	.tab-label-text,
	.tab-label-hint {
		display: block;
		overflow: hidden;
		white-space: nowrap;
		transition: transform var(--motion-state) var(--ease-premium);
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

	/* A title that does not fit fades out over its last 24px. */
	.tab-label:global(.clipped) {
		mask-image: linear-gradient(to right, black calc(100% - 24px), transparent);
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
		.tab-label-hint,
		.tab,
		.tab-close,
		.new-tab-btn {
			transition: none;
		}

		.tab.active:active {
			transform: none;
		}

		.tab-unread {
			animation: none;
		}

		.new-tab-btn:active {
			transform: none;
		}
	}

	/* The label's own face and size, so the title does not move or change
	   size when it becomes editable. The field shows as a wash around the
	   text, drawn as a spread shadow so it takes no room. */
	/* Renaming: the tab is the field (app.css). It outranks the glass of
	   the tab you are on and the hover wash. */
	/* The × keeps its space while you type, so the tab keeps its width,
	   but it can't be seen or pressed. */
	.tab.renaming .tab-close {
		visibility: hidden;
	}

	/* The field takes the label's room, so the hidden × stays at the end. */
	.tab.renaming .rename-sizer {
		flex: 1;
	}

	.tab.renaming,
	.tab.renaming:hover {
		background: var(--rename-fill);
		color: var(--color-foreground);
		outline: 1px solid var(--rename-edge);
		outline-offset: -1px;
		cursor: text;
	}

	.tab-emoji {
		font-size: 14px;
		line-height: 14px;
		width: 14px;
		text-align: center;
		flex-shrink: 0;
	}




	.tab-close {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 20px;
		height: 20px;
		padding: 0;
		border: none;
		border-radius: var(--radius-sm);
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: 13px;
		cursor: pointer;
		opacity: 0;
		transition:
			opacity var(--motion-hover-out) var(--ease-premium),
			background-color var(--motion-hover-out) var(--ease-premium);
		flex-shrink: 0;
	}

	/* A pane's only tab hides its × until you point at it: shown at rest, it
	   offers to close the one view you have. */
	.tab:hover .tab-close,
	.tab.active:not(.only) .tab-close {
		opacity: 1;
	}

	/* Neutral, not red. Red is for destructive-with-consequence; closing a tab
	   loses nothing and is one ⌘⇧T away from undone. The old error-tinted
	   treatment (plus a red wash over the whole tab) read as a warning for an
	   action that doesn't warrant one. */
	.tab-close:hover {
		background: var(--active-bg);
		color: var(--color-foreground);
	}

	.tab-close:active {
		background: var(--press-bg);
	}

	.tab-close:focus-visible {
		opacity: 1;
	}

	.new-tab-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: var(--chrome-tab-h);
		height: var(--chrome-tab-h);
		padding: 0;
		border: none;
		border-radius: var(--radius-lg);
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: 16px;
		cursor: pointer;
		flex-shrink: 0;
		transition:
			background-color var(--motion-hover-out) var(--ease-premium),
			color var(--motion-hover-out) var(--ease-premium),
			transform var(--motion-release) var(--ease-premium);
	}

	/* Hover used to be `--color-surface-elevated` over a `--color-background`
	   toolbar — #F5F4EF on #FDFCF9 in the default theme, a ~3% shift that read
	   as nothing happening. A foreground mix is legible in every theme instead
	   of depending on two surface tokens staying far enough apart. */
	.new-tab-btn:hover {
		background: var(--hover-bg);
		color: var(--color-foreground);
		transition-duration: var(--motion-hover-in), var(--motion-hover-in), var(--motion-release);
	}

	/* Lit while its menu is open, as a hovered + is. */
	.new-tab-btn.open {
		background: var(--hover-bg);
		color: var(--color-foreground);
	}

	.new-tab-btn:active {
		background: var(--press-bg);
		transform: scale(0.94);
		transition-duration: var(--motion-press);
	}

	.new-tab-btn:focus-visible {
		color: var(--color-foreground);
	}

	/* The slot a dragged tab will take. The library keeps the tab's own
	   width open there and hides it; the slot shows as the drop tint, and
	   moves along the strip as the neighbours slide aside. */
	:global(.tabs-scroll > [data-is-dnd-shadow-item-internal]) {
		visibility: visible !important;
		background: var(--drop-tint) !important;
		animation: none !important;
	}

	:global(.tabs-scroll > [data-is-dnd-shadow-item-internal] > *) {
		visibility: hidden;
	}

	/* The slot is a place, not a tab: no glass, even when the tab in your
	   hand was the one you were on. */
	:global(.tabs-scroll > [data-is-dnd-shadow-item-internal]) {
		border-color: transparent !important;
		box-shadow: none !important;
		backdrop-filter: none !important;
	}
</style>
