<!--
	ContextPanel.svelte

	The `@` menu in the chat composer's tray. The field keeps the focus and
	what is typed after the `@` is the query, so the arrows, Enter, Tab and
	Escape are caught here before the editor sees them: the arrows move the
	highlight, Tab moves between the type tabs, Enter adds the highlighted
	row, and Escape closes the menu and leaves the text as typed.

	Uses: GET /api/pages/search/refs (searchRefs) with kinds and created_after.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { onMount } from "svelte";
	import { searchRefs, type RefSearchResult } from "$lib/api/client";
	import { contextMenu } from "$lib/stores/contextMenu.svelte";
	import { formatRelativeTimestamp } from "$lib/utils/dateUtils";
	import { inComposition } from "$lib/utils/ime";

	let {
		query,
		active = true,
		onSelect,
		onClose,
		onRangePicked,
	}: {
		query: string;
		/** False while the tray folds it away: it no longer takes keys. */
		active?: boolean;
		onSelect: (result: RefSearchResult) => void;
		onClose: () => void;
		/** A date range was chosen: the field takes the keyboard back. */
		onRangePicked: () => void;
	} = $props();

	// "All" also finds organizations and files, which have no tab of their own.
	const TABS: { id: string; label: string; kinds?: string[] }[] = [
		{ id: "all", label: "All" },
		{ id: "page", label: "Pages", kinds: ["page"] },
		{ id: "person", label: "People", kinds: ["person"] },
		{ id: "place", label: "Places", kinds: ["place"] },
		{ id: "project", label: "Projects", kinds: ["project"] },
		{ id: "chat", label: "Chats", kinds: ["chat"] },
	];

	const DAY_MS = 24 * 60 * 60 * 1000;
	const RANGES: { id: string; label: string; since: () => Date | undefined }[] = [
		{ id: "any", label: "Created any time", since: () => undefined },
		{
			id: "today",
			label: "Created today",
			since: () => {
				const d = new Date();
				d.setHours(0, 0, 0, 0);
				return d;
			},
		},
		{ id: "week", label: "Created in the past week", since: () => new Date(Date.now() - 7 * DAY_MS) },
		{ id: "month", label: "Created in the past month", since: () => new Date(Date.now() - 30 * DAY_MS) },
		{ id: "year", label: "Created in the past year", since: () => new Date(Date.now() - 365 * DAY_MS) },
	];

	const uid = $props.id();
	let tab = $state("all");
	let range = $state("any");
	let results = $state<RefSearchResult[]>([]);
	let selected = $state(0);
	let loading = $state(true);
	let failed = $state(false);
	let panelEl: HTMLDivElement | null = $state(null);
	let listEl: HTMLDivElement | null = $state(null);
	let atEnd = $state(true);

	const rangeLabel = $derived(RANGES.find((r) => r.id === range)?.label ?? RANGES[0].label);

	// A short pause, so each keystroke does not send its own search, and
	// an abort, so a slow answer to an older query cannot replace a newer one.
	// The first search goes at once: the tray grows to fit what it finds, and
	// waiting would make it stop at a smaller height first.
	let first = true;
	$effect(() => {
		const q = query;
		const kinds = TABS.find((t) => t.id === tab)?.kinds;
		const createdAfter = RANGES.find((r) => r.id === range)?.since();
		const ctrl = new AbortController();
		loading = true;
		const timer = setTimeout(async () => {
			try {
				const data = await searchRefs(q, { kinds, createdAfter, limit: 30, signal: ctrl.signal });
				// A box older than these filters ignores them, so the type is
				// checked here too. It sends no dates either, so a date range
				// cannot be applied to its answer.
				results = kinds ? data.results.filter((r) => kinds.includes(r.entity_type)) : data.results;
				failed = false;
			} catch (e) {
				if (ctrl.signal.aborted) return;
				console.error("ContextPanel search failed:", e);
				results = [];
				failed = true;
			}
			selected = 0;
			loading = false;
			listEl?.scrollTo({ top: 0 });
			queueMicrotask(updateFade);
		}, first ? 0 : 120);
		first = false;
		return () => {
			clearTimeout(timer);
			ctrl.abort();
		};
	});

	function updateFade() {
		if (!listEl) return;
		atEnd = listEl.scrollTop + listEl.clientHeight >= listEl.scrollHeight - 2;
	}

	function move(by: number) {
		if (results.length === 0) return;
		selected = (selected + by + results.length) % results.length;
		listEl
			?.querySelector(`#${CSS.escape(`${uid}-row-${selected}`)}`)
			?.scrollIntoView({ block: "nearest" });
	}

	function cycleTab(by: number) {
		const i = TABS.findIndex((t) => t.id === tab);
		tab = TABS[(i + by + TABS.length) % TABS.length].id;
	}

	function pickRange(anchor: HTMLElement) {
		const r = anchor.getBoundingClientRect();
		contextMenu.show(
			{ x: r.right, y: r.bottom },
			RANGES.map((x) => ({
				id: `range-${x.id}`,
				label: x.label,
				checked: x.id === range,
				action: () => {
					range = x.id;
					onRangePicked();
				},
			})),
			{
				anchor: { x: r.left, y: r.top, width: r.width, height: r.height },
				placement: "bottom-end",
			},
		);
	}

	function dayLabel(iso?: string): string {
		if (!iso) return "";
		const d = new Date(iso);
		const today = new Date();
		const same =
			d.getFullYear() === today.getFullYear() &&
			d.getMonth() === today.getMonth() &&
			d.getDate() === today.getDate();
		return same ? "Today" : formatRelativeTimestamp(iso);
	}

	// Only while the field this panel belongs to has the keyboard, and not
	// while another menu (the date range) is open over it.
	function onKeydown(e: KeyboardEvent) {
		if (!active || contextMenu.visible || inComposition(e)) return;
		const shell = panelEl?.closest(".chat-input-wrapper");
		if (!shell || !shell.contains(e.target as Node)) return;
		let handled = true;
		if (e.key === "ArrowDown") move(1);
		else if (e.key === "ArrowUp") move(-1);
		else if (e.key === "Tab") cycleTab(e.shiftKey ? -1 : 1);
		else if (e.key === "Escape") onClose();
		else if (e.key === "Enter" && !e.shiftKey && results[selected]) onSelect(results[selected]);
		else handled = false;
		if (handled) {
			e.preventDefault();
			e.stopPropagation();
		}
	}

	onMount(() => {
		document.addEventListener("keydown", onKeydown, true);
		return () => document.removeEventListener("keydown", onKeydown, true);
	});
</script>

<!-- A press inside the panel would otherwise take the focus from the field. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="context-panel" bind:this={panelEl} onmousedown={(e) => e.preventDefault()}>
	<div class="context-head">
		<div class="context-tabs" role="tablist" aria-label="Kind">
			{#each TABS as t (t.id)}
				<button
					type="button"
					role="tab"
					class="context-tab"
					class:active={t.id === tab}
					aria-selected={t.id === tab}
					onclick={() => (tab = t.id)}
				>
					{t.label}
				</button>
			{/each}
		</div>
		<button type="button" class="context-range" aria-haspopup="menu" onclick={(e) => pickRange(e.currentTarget)}>
			<Icon icon="ri:calendar-line" width="14" style="color: inherit" />
			<span>{rangeLabel}</span>
			<Icon icon="ri:arrow-down-s-line" width="14" style="color: inherit" />
		</button>
	</div>

	<div
		class="context-list"
		class:fade={!atEnd}
		bind:this={listEl}
		onscroll={updateFade}
		role="listbox"
		aria-label="Add context"
	>
		{#each results as r, i (r.entity_type + r.id)}
			<button
				type="button"
				id="{uid}-row-{i}"
				class="context-row"
				class:selected={i === selected}
				role="option"
				aria-selected={i === selected}
				onmousemove={() => (selected = i)}
				onclick={() => onSelect(r)}
			>
				<span class="context-tile"><Icon icon={r.icon} width="15" style="color: inherit" /></span>
				<span class="context-name">{r.name}</span>
				<span class="context-date">{dayLabel(r.created_at)}</span>
			</button>
		{:else}
			<p class="context-empty" role="status">
				{#if failed}
					Search didn't respond. Check that your server is reachable, then type again.
				{:else if loading}
					<!-- Nothing while the first answer is on its way. -->
				{:else if query}
					Nothing matches "{query}".
				{:else}
					Nothing here yet.
				{/if}
			</p>
		{/each}
	</div>
</div>

<style>
	.context-panel {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		width: 100%;
		min-width: 0;
	}

	.context-head {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		min-width: 0;
	}

	.context-tabs {
		display: flex;
		align-items: center;
		gap: 0.125rem;
		min-width: 0;
		overflow-x: auto;
		scrollbar-width: none;
	}

	.context-tab,
	.context-range {
		display: inline-flex;
		align-items: center;
		gap: 0.3125rem;
		flex-shrink: 0;
		height: 1.875rem;
		padding: 0 0.5rem;
		border-radius: var(--radius-md);
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		white-space: nowrap;
		cursor: pointer;
		transition:
			background-color 0.15s ease,
			color 0.15s ease;
	}

	.context-tab:hover,
	.context-range:hover {
		color: var(--color-foreground);
	}

	.context-tab.active {
		background: var(--hover-bg);
		color: var(--color-foreground);
	}

	.context-range {
		margin-left: auto;
	}

	/* A fixed height of five and a half rows, whatever the tab or query
	   finds, so the tab row above never moves. The half row shows there is
	   more to scroll. */
	.context-list {
		display: flex;
		flex-direction: column;
		height: 13.75rem;
		overflow-y: auto;
		overscroll-behavior: contain;
	}

	/* More rows below: the last visible one fades out. */
	.context-list.fade {
		mask-image: linear-gradient(to bottom, black calc(100% - 2.5rem), transparent);
	}

	.context-row {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		flex-shrink: 0;
		width: 100%;
		min-height: 2.5rem;
		padding: 0.25rem 0.5rem 0.25rem 0.25rem;
		border-radius: var(--radius-md);
		text-align: left;
		cursor: pointer;
	}

	.context-row.selected {
		background: var(--hover-bg);
	}

	.context-tile {
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		width: 1.875rem;
		height: 1.875rem;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background: var(--color-surface);
		color: var(--color-foreground-muted);
	}

	.context-name {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: 0.875rem;
		color: var(--color-foreground);
	}

	.context-date {
		flex-shrink: 0;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}

	.context-empty {
		margin: auto 0;
		padding: 0.75rem 0.5rem;
		text-align: center;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}
</style>
