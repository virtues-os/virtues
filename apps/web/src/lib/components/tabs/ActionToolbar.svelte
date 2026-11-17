<script lang="ts">
	/**
	 * The action toolbar at the top of every page on the desktop. TabContent
	 * draws it above each tab's view, so no view lays out its own.
	 *
	 * The left end moves this tab back and forward. The middle says where the
	 * page is (`tabs/crumbs.ts`, or the path a view publishes with
	 * `paneActions.setCrumbs`), with the pin at its end. A chat's path stops
	 * at its project, since its tab already says its name. A segment that holds
	 * things opens a menu of them (`tabs/crumbMenus.ts`), and once one is
	 * open, pointing at another segment opens its menu instead, as in the
	 * Mac's menu bar. The last segment is the page itself: double-click to
	 * rename it in place, right-click for its actions. Search is not here:
	 * it searches everything, so it sits in the window header's corner
	 * (HeaderCorner). The right end holds the page's status mark
	 * (`usePageStatus`), then what the page lets you do: the controls a
	 * view hands over with `usePageActions`, then the buttons it publishes
	 * with `paneActions.set`, two inline and the rest behind a `···`, since a
	 * split pane can be dragged to a third of the window.
	 */
	import Icon from "$lib/components/Icon.svelte";
	import AtlasIcon from "$lib/components/sidebar/AtlasIcon.svelte";
	import { paneActions } from "$lib/stores/paneActions.svelte";
	import { contextMenu, type ContextMenuItem } from "$lib/stores/contextMenu.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { canPin, isPinned, togglePin } from "$lib/pins/pinAction";
	import { crumbsForRoute } from "$lib/tabs/crumbs";
	import { crumbMenuItems } from "$lib/tabs/crumbMenus";
	import { getProjectMenuItems } from "$lib/utils/contextMenuItems";
	import { identityOf, ownedRef, renameRef } from "$lib/refs/identity.svelte";
	import type { Tab } from "$lib/tabs/types";

	let { tab }: { tab: Tab } = $props();

	const paneId = $derived(windowShellStore.findTabPane(tab.id) ?? undefined);
	/** In split view, the pane you are not in: its bar dims, as its tab does. */
	const idle = $derived(windowShellStore.isSplit && !!paneId && windowShellStore.activePaneId !== paneId);
	const canGoBack = $derived(!!paneId && windowShellStore.canGoBack(paneId));
	const canGoForward = $derived(!!paneId && windowShellStore.canGoForward(paneId));

	// The page's name as its tab shows it (WindowTabBar's `tabLook`).
	const title = $derived(identityOf(tab.route, { title: tab.label, icon: null, color: null }).title);
	const path = $derived(paneActions.crumbsFor(tab.id) ?? crumbsForRoute(tab.route, title));
	/** Which segment is the page itself, or -1 when the path leaves it out. */
	const pageAt = $derived(path.withoutPage ? -1 : path.crumbs.length - 1);
	const pinnable = $derived(canPin(tab.route));
	const pinned = $derived(pinnable && isPinned(tab.route));

	// Only a thing with a name of its own renames here, through the same
	// path as the tab and the sidebar (`renameRef`), so they all move together.
	const renamable = $derived(!!ownedRef(tab.route) && pageAt >= 0);
	/** The name being typed, while the last segment is a field. */
	let draft = $state<string | null>(null);

	function startRename() {
		if (renamable) draft = path.crumbs.at(-1)?.label ?? title;
	}

	function commitRename() {
		const next = draft?.trim();
		draft = null;
		if (!next || next === (path.crumbs.at(-1)?.label ?? title)) return;
		renameRef(tab.route, next).catch((err) => console.error("[ActionToolbar] rename failed:", err));
	}

	function renameKeydown(e: KeyboardEvent) {
		if (e.key === "Enter") {
			e.preventDefault();
			commitRename();
		} else if (e.key === "Escape") {
			e.preventDefault();
			draft = null;
		}
	}

	// Selecting the name lets you type over it.
	function focusAndSelect(node: HTMLInputElement) {
		node.focus();
		node.select();
	}

	const status = $derived(paneActions.statusFor(tab.id));
	const statusIcon = $derived(status?.icon);
	const controls = $derived(paneActions.snippetsFor(tab.id));
	const actions = $derived(paneActions.for(tab.id));
	const INLINE_ACTION_LIMIT = 2;
	const inlineActions = $derived(actions.slice(0, INLINE_ACTION_LIMIT));
	const overflowActions = $derived(actions.slice(INLINE_ACTION_LIMIT));

	function go(route: string) {
		windowShellStore.navigate(route, { paneId });
	}

	/** The segment whose menu is open, while one is. */
	let openCrumb = $state<number | null>(null);
	let crumbsEl = $state<HTMLElement | null>(null);

	function showCrumbMenu(index: number, el: HTMLElement) {
		const crumb = path.crumbs[index];
		if (!crumb?.menu) return;
		const rect = el.getBoundingClientRect();
		contextMenu.show({ x: rect.left, y: rect.bottom }, crumbMenuItems(crumb, crumb.menu, tab.route, go), {
			anchor: { x: rect.left, y: rect.top, width: rect.width, height: rect.height },
			placement: "bottom-start",
		});
		openCrumb = index;
	}

	// The menu closes on its own terms: a choice, a click elsewhere, Escape.
	$effect(() => {
		if (!contextMenu.visible) openCrumb = null;
	});

	// While a segment's menu is open, pointing at another segment opens that
	// one's. The menu's backdrop takes the pointer, so this reads the
	// segments' boxes rather than waiting for them to be hovered.
	$effect(() => {
		if (openCrumb === null || !crumbsEl) return;
		const nav = crumbsEl;
		const move = (e: PointerEvent) => {
			for (const el of nav.querySelectorAll<HTMLElement>("[data-crumb-menu]")) {
				const r = el.getBoundingClientRect();
				if (e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom) continue;
				const index = Number(el.dataset.crumbMenu);
				if (index !== openCrumb) showCrumbMenu(index, el);
				return;
			}
		};
		window.addEventListener("pointermove", move);
		return () => window.removeEventListener("pointermove", move);
	});

	/** The page's own actions, on the last segment's right-click. */
	function showPageMenu(e: MouseEvent) {
		if (!renamable) return;
		e.preventDefault();
		const items: ContextMenuItem[] = [
			{ id: "rename", label: "Rename", icon: "mynaui:edit", action: startRename },
			...getProjectMenuItems(tab.route, title),
		];
		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}

	function togglePinned() {
		void togglePin({ url: tab.route, label: tab.label, icon: tab.icon }).catch((err) =>
			console.error("[ActionToolbar] pin failed:", err),
		);
	}

	function showOverflow(e: MouseEvent) {
		e.stopPropagation();
		const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
		contextMenu.show(
			{ x: rect.left, y: rect.bottom + 4 },
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
</script>

<div class="action-toolbar" class:idle role="toolbar" aria-label="Page">
	<div class="history">
		<button
			class="action"
			onclick={() => windowShellStore.goBack(paneId)}
			disabled={!canGoBack}
			aria-label="Back"
			data-tip="Back"
		>
			<AtlasIcon name="back" size={16} bare />
		</button>
		<button
			class="action"
			onclick={() => windowShellStore.goForward(paneId)}
			disabled={!canGoForward}
			aria-label="Forward"
			data-tip="Forward"
		>
			<AtlasIcon name="forward" size={16} bare />
		</button>
	</div>

	<div class="path">
		{#if path.crumbs.length}
			<span class="path-glyph" aria-hidden="true"><AtlasIcon name={path.glyph} size={14} bare /></span>
		{/if}
		<nav class="crumbs" aria-label="Where this page is" bind:this={crumbsEl}>
			{#each path.crumbs as crumb, i (i)}
				{#if i > 0}<span class="crumb-sep" aria-hidden="true">/</span>{/if}
				{#if crumb.menu && i !== pageAt}
					<button
						class="crumb link menu"
						class:root={i === 0}
						class:open={openCrumb === i}
						data-crumb-menu={i}
						aria-haspopup="menu"
						aria-expanded={openCrumb === i}
						onclick={(e) => showCrumbMenu(i, e.currentTarget)}>{crumb.label}</button
					>
				{:else if (crumb.route || crumb.run) && i !== pageAt}
					<button class="crumb link" class:root={i === 0} onclick={() => (crumb.run ? crumb.run() : go(crumb.route!))}
						>{crumb.label}</button
					>
				{:else if i === pageAt && draft !== null}
					<span class="rename-sizer crumb-field" data-value={draft}>
						<input
							class="crumb current rename"
							size="1"
							bind:value={draft}
							onkeydown={renameKeydown}
							onblur={commitRename}
							aria-label="Name"
							use:focusAndSelect
						/>
					</span>
				{:else}
					<!-- svelte-ignore a11y_no_static_element_interactions -->
					<span
						class="crumb"
						class:root={i === 0}
						class:current={i === pageAt}
						class:renamable={renamable && i === pageAt}
						aria-current={i === pageAt ? "page" : undefined}
						ondblclick={i === pageAt ? startRename : undefined}
						oncontextmenu={i === pageAt ? showPageMenu : undefined}>{crumb.label}</span
					>
				{/if}
			{/each}
		</nav>
		{#if pinnable}
			<button
				class="pin"
				class:on={pinned}
				onclick={togglePinned}
				aria-pressed={pinned}
				aria-label={pinned ? "Unpin" : "Pin"}
				data-tip={pinned ? "Unpin" : "Pin"}
			>
				<AtlasIcon name={pinned ? "pinned" : "pin"} size={14} bare />
			</button>
		{/if}
	</div>

	{#if status || controls.length > 0 || actions.length > 0}
		<div class="actions">
			{#if status}
				<svelte:element
					this={status.run ? "button" : "span"}
					class="status"
					class:error={status.tone === "error"}
					class:runnable={!!status.run}
					role={status.run ? undefined : "status"}
					onclick={status.run}
					data-tip={status.tip}
					aria-label={status.tip ? `${status.label}. ${status.tip}` : undefined}
				>
					{#if statusIcon}<Icon icon={statusIcon} width="14" />{/if}
					<span class="status-label">{status.label}</span>
				</svelte:element>
			{/if}
			{#each controls as control, i (i)}
				<div class="controls">{@render control()}</div>
			{/each}
			{#each inlineActions as action (action.id)}
				<button
					class="action"
					class:primary={action.primary}
					class:toggled={action.active}
					aria-pressed={action.active !== undefined ? action.active : undefined}
					disabled={action.disabled}
					onclick={action.run}
					aria-label={action.label}
					data-tip={action.label}
				>
					<Icon icon={action.icon} />
					{#if action.primary}<span class="action-label">{action.label}</span>{/if}
				</button>
			{/each}

			{#if overflowActions.length > 0}
				<button class="action" onclick={showOverflow} aria-label="More actions" data-tip="More actions">
					<Icon icon="mynaui:dots" />
				</button>
			{/if}
		</div>
	{/if}
</div>

<style>
	.action-toolbar {
		container-type: inline-size;
		container-name: actiontoolbar;
		flex: none;
		display: flex;
		align-items: center;
		gap: 12px;
		height: 48px;
		padding: 0 12px;
		/* No rule under the bar: it is the top of the page's paper. */
	}

	/* The bar of the pane you are not in keeps its layout and goes quiet:
	   the page's name drops to muted ink and the rest fades, the way a Mac
	   window behind another greys its title. Pointing at it brings the ink
	   back, since a click there still works. */
	.action-toolbar > * {
		transition: opacity var(--motion-hover-out) var(--ease-premium);
	}

	.action-toolbar.idle > * {
		opacity: 0.55;
	}

	.action-toolbar.idle .crumb.current {
		color: var(--color-foreground-muted);
	}

	.action-toolbar.idle:hover > * {
		opacity: 1;
		transition-duration: var(--motion-hover-in);
	}

	.history {
		display: flex;
		align-items: center;
		gap: 4px;
		flex: none;
	}

	/* Where the page is: the bar's middle, on the page's own ground. */
	.path {
		flex: 1;
		min-width: 0;
		display: flex;
		align-items: center;
		gap: 4px;
		height: 32px;
		padding: 0 6px;
	}

	.path-glyph {
		display: flex;
		flex: none;
		color: var(--color-foreground-muted);
	}

	.crumbs {
		flex: 1;
		min-width: 0;
		display: flex;
		align-items: center;
		gap: 4px;
		font-size: 13px;
		color: var(--color-foreground-muted);
		overflow: hidden;
	}

	/* Every segment has the same room around its words, so one that is a
	   button can show it on hover without the path moving. */
	.crumb {
		min-width: 0;
		padding: 0 6px;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	/* The root and the page keep their width; the segments between give
	   theirs up first, down to a letter and an ellipsis, so a segment never
	   leaves two separators side by side. */
	.crumb:not(.root):not(.current) {
		min-width: 2.5em;
	}

	.crumb.root,
	.crumb.current {
		flex: none;
		max-width: 60%;
	}

	.crumb.link {
		height: 24px;
		border: none;
		border-radius: var(--radius-sm);
		background: none;
		font: inherit;
		color: inherit;
		cursor: pointer;
		transition:
			background-color var(--motion-hover-out) var(--ease-premium),
			color var(--motion-hover-out) var(--ease-premium);
	}

	.crumb.link:hover {
		color: var(--color-foreground);
		transition-duration: var(--motion-hover-in);
	}

	/* A segment that opens a menu shows it is a button, and stays lit while
	   its menu is open. */
	.crumb.menu:hover,
	.crumb.menu.open {
		background: var(--hover-bg);
		color: var(--color-foreground);
	}

	.crumb.current {
		color: var(--color-foreground);
		font-weight: 500;
	}

	.crumb.renamable {
		cursor: text;
	}

	/* The name as a field, in the same look as a renaming tab or sidebar
	   row (app.css). It keeps the crumb's type and padding, and the sizer
	   keeps it as wide as the name, so the path does not jump when it
	   opens. The sizer's hidden copy takes the same padding and weight. */
	.crumb-field {
		flex: none;
		max-width: 60%;
	}

	.crumb-field::after {
		padding: 0 6px;
		font-weight: 500;
	}

	.crumb.rename {
		min-width: 4ch;
		/* The sizer holds the crumb's 60% cap; on the input it would be 60%
		   of the sizer. */
		max-width: none;
		height: 24px;
		border: none;
		border-radius: var(--radius-sm);
		background: var(--rename-fill);
		font: inherit;
		font-weight: 500;
		color: var(--color-foreground);
		caret-color: var(--color-primary);
		outline: 1px solid var(--rename-edge);
		outline-offset: -1px;
	}

	.crumb-sep {
		flex: none;
		color: var(--color-foreground-subtle);
	}

	.pin {
		display: flex;
		align-items: center;
		justify-content: center;
		flex: none;
		width: 24px;
		height: 24px;
		padding: 0;
		border: none;
		border-radius: var(--radius-sm);
		background: transparent;
		color: var(--color-foreground-subtle);
		cursor: pointer;
	}

	.pin:hover,
	.pin.on {
		color: var(--color-foreground);
	}

	.actions {
		display: flex;
		align-items: center;
		gap: 4px;
		flex: none;
		max-width: 50%;
	}

	/* The page's state, or where its work runs. Quiet: it is read, not
	   pressed, unless it carries an action. */
	.status {
		display: flex;
		align-items: center;
		gap: 4px;
		min-width: 0;
		height: 28px;
		padding: 0 8px;
		margin-right: 4px;
		border: none;
		border-radius: var(--radius-md);
		background: transparent;
		font: inherit;
		font-size: 12px;
		color: var(--color-foreground-muted);
		white-space: nowrap;
	}

	.status-label {
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.status.runnable {
		cursor: pointer;
	}

	.status.runnable:hover {
		background: var(--hover-bg);
		color: var(--color-foreground);
	}

	.status.error {
		color: var(--color-error);
	}

	/* A narrow pane keeps the mark's icon and drops its words. */
	@container actiontoolbar (max-width: 420px) {
		.status-label {
			display: none;
		}
	}

	/* A view's own controls: their own sizes, on the toolbar's centre line,
	   and the toolbar's text size for any that inherit it. */
	.controls {
		position: relative;
		display: flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
		font-size: 13px;
		margin-right: 4px;
	}

	.action {
		display: flex;
		align-items: center;
		justify-content: center;
		flex: none;
		width: 28px;
		height: 28px;
		padding: 0;
		border: none;
		border-radius: var(--radius-md);
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: 16px;
		cursor: pointer;
		transition:
			background-color var(--motion-hover-out) var(--ease-premium),
			color var(--motion-hover-out) var(--ease-premium);
	}

	.action:hover:not(:disabled) {
		background: var(--hover-bg);
		color: var(--color-foreground);
	}

	.action.primary {
		color: var(--color-primary);
		width: auto;
		gap: 4px;
		padding: 0 8px;
	}

	.action-label {
		font-size: 12px;
		white-space: nowrap;
	}

	/* The primary action keeps its label while the pane has room for it. */
	@container actiontoolbar (max-width: 520px) {
		.action-label {
			display: none;
		}
		.action.primary {
			width: 28px;
			padding: 0;
		}
	}

	/* A toggle that's on reads as held down, not as merely hovered. */
	.action.toggled {
		background: var(--active-bg);
		color: var(--color-foreground);
	}

	.action:disabled {
		opacity: 0.4;
		cursor: default;
	}
</style>
