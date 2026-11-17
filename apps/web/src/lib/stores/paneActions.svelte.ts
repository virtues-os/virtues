/**
 * The action toolbar's slots (tabs/ActionToolbar.svelte).
 *
 * One contract for every view: **the page's name on the left, its actions on
 * the right.** A view publishes buttons as `PaneAction`s with `set`, or its
 * own controls as a snippet with `usePageActions`, when they are more than a
 * button: a search field, a switch, a popover.
 *
 * Keyed by tab id, because two panes are visible at once and each shows its own
 * view's actions. A view registers on mount and clears on destroy.
 */

import { getContext, setContext, untrack, type Snippet } from "svelte";
import { mobileLayout } from "$lib/stores/mobileLayout.svelte";
import type { CrumbPath } from "$lib/tabs/crumbs";

export interface PaneAction {
	id: string;
	label: string;
	icon: string;
	/** Gets the click when the action is a button in the strip, so a menu
	 *  can open under it; nothing when run from the overflow menu. */
	run: (e?: MouseEvent) => void;
	/** Renders as the emphasised action, and keeps its label when space allows.
	    At most one per view. */
	primary?: boolean;
	disabled?: boolean;
	/**
	 * For actions that are a state rather than an event — SystemInfoView's
	 * "Detail", say. Renders held-down. Without this the slot could only
	 * express fire-and-forget buttons, and toggles would have had to stay
	 * behind in their views, which defeats the point of having one place.
	 */
	active?: boolean;
}

/**
 * One short mark before a page's actions: its state ("Saved", "Offline"),
 * or what is working on it.
 */
export interface StatusMark {
	label: string;
	/** The longer sentence, shown on hover. */
	tip?: string;
	/** Myna icon drawn before the label. */
	icon?: string;
	/** TODO(2026-11-16): delete once DayPage stops passing it (another
	 *  agent has DayPage open). Nothing reads it: the toolbar no longer
	 *  marks work that leaves the box. */
	offBox?: boolean;
	/** Something to act on (a failed save). */
	tone?: "muted" | "error";
	/** A mark that can be acted on (Stop, open the model settings). */
	run?: () => void;
}

class PaneActionsStore {
	#byTab = $state<Record<string, PaneAction[]>>({});
	#crumbs = $state<Record<string, CrumbPath>>({});
	#snippets = $state<Record<string, Snippet[]>>({});
	#status = $state<Record<string, StatusMark>>({});

	/** The mark a tab's view set, if any. */
	statusFor(tabId: string | null | undefined): StatusMark | undefined {
		return tabId ? this.#status[tabId] : undefined;
	}

	/** Set a tab's status mark. Returns the teardown. */
	setStatus(tabId: string, mark: StatusMark): () => void {
		untrack(() => {
			this.#status[tabId] = mark;
		});
		return () => {
			untrack(() => {
				if (this.#status[tabId] === mark) delete this.#status[tabId];
			});
		};
	}

	/** Actions for a tab, or an empty list. */
	for(tabId: string | null | undefined): PaneAction[] {
		return tabId ? (this.#byTab[tabId] ?? []) : [];
	}

	/** The controls a tab's view drew for the toolbar, in the order they came. */
	snippetsFor(tabId: string | null | undefined): Snippet[] {
		return tabId ? (this.#snippets[tabId] ?? []) : [];
	}

	/** The path a tab's view published, if it knows better than its route. */
	crumbsFor(tabId: string | null | undefined): CrumbPath | undefined {
		return tabId ? this.#crumbs[tabId] : undefined;
	}

	/**
	 * Publish this view's actions. Returns a teardown for `onMount` — without
	 * one, a closed tab's actions would linger in the toolbar of whatever
	 * replaced it.
	 *
	 * Views call this from an `$effect` (the disabled/active state has to stay
	 * live), so the write must not also read. Rebuilding the map — `{ ...byTab,
	 * [tabId]: actions }` — read every key, which made the effect depend on the
	 * thing it was about to write: set → invalidate → set, until Svelte's depth
	 * guard fired. Assign the one key, untracked.
	 */
	set(tabId: string, actions: PaneAction[]): () => void {
		untrack(() => {
			this.#byTab[tabId] = actions;
		});
		return () => this.clear(tabId);
	}

	/** Add controls to a tab's toolbar. A page and a grid inside it can
	 *  each add their own. Returns the teardown. */
	addSnippet(tabId: string, snippet: Snippet): () => void {
		untrack(() => {
			this.#snippets[tabId] = [...(this.#snippets[tabId] ?? []), snippet];
		});
		return () => {
			untrack(() => {
				const left = (this.#snippets[tabId] ?? []).filter((s) => s !== snippet);
				if (left.length > 0) this.#snippets[tabId] = left;
				else delete this.#snippets[tabId];
			});
		};
	}

	setCrumbs(tabId: string, crumbs: CrumbPath): () => void {
		untrack(() => {
			this.#crumbs[tabId] = crumbs;
		});
		return () => {
			untrack(() => {
				delete this.#crumbs[tabId];
			});
		};
	}

	clear(tabId: string): void {
		untrack(() => {
			delete this.#byTab[tabId];
		});
	}
}

export const paneActions = new PaneActionsStore();

const TAB_ID = Symbol("tab-id");

/** TabContent names the tab its view belongs to, for `usePageActions`. */
export function provideTabId(tabId: () => string): void {
	setContext(TAB_ID, tabId);
}

/**
 * Put a component's page-level controls in its tab's action toolbar. Call
 * it while the component initialises, from anywhere under a tab's view.
 *
 * The toolbar exists only on the desktop, so the result says where the
 * controls are: while `inToolbar` is false (the phone, or a view drawn
 * outside a tab), the component renders the snippet in its own place.
 */
export function usePageActions(snippet: () => Snippet | undefined | null): { readonly inToolbar: boolean } {
	const tabId = getContext<(() => string) | undefined>(TAB_ID);
	const inToolbar = $derived(!!tabId && !mobileLayout.isMobile);
	$effect(() => {
		const s = snippet();
		if (!inToolbar || !s || !tabId) return;
		return paneActions.addSnippet(tabId(), s);
	});
	return {
		get inToolbar() {
			return inToolbar;
		},
	};
}

/**
 * Set the status mark of the tab a component is drawn in, from anywhere
 * under its view (see `usePageActions`). While `inToolbar` is false the
 * component says its state in its own place.
 */
export function usePageStatus(mark: () => StatusMark | null | undefined): { readonly inToolbar: boolean } {
	const tabId = getContext<(() => string) | undefined>(TAB_ID);
	const inToolbar = $derived(!!tabId && !mobileLayout.isMobile);
	$effect(() => {
		const m = mark();
		if (!inToolbar || !m || !tabId) return;
		return paneActions.setStatus(tabId(), m);
	});
	return {
		get inToolbar() {
			return inToolbar;
		},
	};
}

/**
 * Set the path of the tab a component is drawn in, from anywhere under its
 * view (see `usePageActions`). While `inToolbar` is false the component
 * draws its own breadcrumb.
 */
export function usePageCrumbs(path: () => CrumbPath | null | undefined): { readonly inToolbar: boolean } {
	const tabId = getContext<(() => string) | undefined>(TAB_ID);
	const inToolbar = $derived(!!tabId && !mobileLayout.isMobile);
	$effect(() => {
		const p = path();
		if (!inToolbar || !p || !tabId) return;
		return paneActions.setCrumbs(tabId(), p);
	});
	return {
		get inToolbar() {
			return inToolbar;
		},
	};
}
