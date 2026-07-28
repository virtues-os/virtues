/**
 * The pane toolbar's action slot.
 *
 * One contract for every view: **breadcrumb/title on the left, view actions on
 * the right.** Before this, actions lived in three unrelated places — the
 * `actions` snippet on `Page.svelte`, the datagrid's own toolbar, and Pages'
 * bottom bar — so the same kind of control appeared somewhere different
 * depending on which view you were in.
 *
 * Not a new toolbar. Views publish into the toolbar that already exists, so
 * Pages stops being special: it just fills the slot like everything else.
 *
 * Keyed by tab id, because two panes are visible at once and each shows its own
 * view's actions. A view registers on mount and clears on destroy.
 */

export interface PaneAction {
	id: string;
	label: string;
	icon: string;
	run: () => void;
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

class PaneActionsStore {
	#byTab = $state<Record<string, PaneAction[]>>({});
	#crumbs = $state<Record<string, string[]>>({});

	/** Actions for a tab, or an empty list. */
	for(tabId: string | null | undefined): PaneAction[] {
		return tabId ? (this.#byTab[tabId] ?? []) : [];
	}

	/** Breadcrumb trail for a tab (["Settings", "Billing"]). */
	crumbsFor(tabId: string | null | undefined): string[] {
		return tabId ? (this.#crumbs[tabId] ?? []) : [];
	}

	/**
	 * Publish this view's actions. Returns a teardown for `onMount` — without
	 * one, a closed tab's actions would linger in the toolbar of whatever
	 * replaced it.
	 */
	set(tabId: string, actions: PaneAction[]): () => void {
		this.#byTab = { ...this.#byTab, [tabId]: actions };
		return () => this.clear(tabId);
	}

	setCrumbs(tabId: string, crumbs: string[]): () => void {
		this.#crumbs = { ...this.#crumbs, [tabId]: crumbs };
		return () => {
			const next = { ...this.#crumbs };
			delete next[tabId];
			this.#crumbs = next;
		};
	}

	clear(tabId: string): void {
		const next = { ...this.#byTab };
		delete next[tabId];
		this.#byTab = next;
	}
}

export const paneActions = new PaneActionsStore();
