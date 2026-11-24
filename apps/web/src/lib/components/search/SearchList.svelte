<script lang="ts">
	/**
	 * Search's results: the rows `searchGroups` picks for the query, and the
	 * theme list Change theme swaps in. The search card holds the field and
	 * hosts the list, on the desktop and the phone (SearchModal), and passes
	 * its field's keys to `keydown`. Mounted per search, so every search
	 * starts fresh.
	 */
	import Icon from "$lib/components/Icon.svelte";
	import ProjectGlyph from "$lib/components/ProjectGlyph.svelte";
	import { onMount } from "svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { chatSessions } from "$lib/stores/chatSessions.svelte";
	import { askVirtues } from "$lib/stores/pendingPrompt.svelte";
	import { pagesStore } from "$lib/stores/pages.svelte";
	import { projectStore } from "$lib/stores/project.svelte";
	import { visits } from "$lib/stores/visits.svelte";
	import { loadRecentSources } from "$lib/sidebar/recents.svelte";
	import { searchLocal, type LocalSearchHit } from "$lib/api/client";
	import { entities } from "$lib/search/entities.svelte";
	import { handOff } from "$lib/search/handoff";
	import { openWebLink } from "$lib/utils/webLinks";
	import { newTabMenuItems } from "$lib/utils/newActions";
	import { canBrowserPane } from "$lib/tauri/bridge";
	import { getAssistantProfile } from "$lib/api/client";
	import { motion } from "$lib/stores/motion.svelte";
	import { settingPlace, titleOf } from "$lib/settings";
	import { shortcuts } from "$lib/shortcuts/registry.svelte";
	import { openSettingInTab } from "$lib/settings/focus";
	import {
		BOOKMARK_ROWS,
		SCOPED_BOOKMARK_ROWS,
		searchBookmarks,
		commandsFromMenu,
		contentGlyph,
		contentTarget,
		parseQuery,
		previewOf,
		rowNoun,
		rowRoute,
		searchGroups,
		type BookmarkHit,
		type Command,
		type SearchRow,
	} from "$lib/search/results";
	import {
		getAvailableThemes,
		getThemeDisplayName,
		getThemeChoice,
		applyTheme,
		setTheme,
		themeMetadata,
		type ThemeChoice,
	} from "$lib/utils/theme";

	interface Props {
		/** The field's text. Change theme clears it, to filter the themes instead. */
		query: string;
		/** The route of the tab search was opened from, for the empty state. */
		here: string | null;
		/** "theme" while choosing a theme, so the host can say so in its field. */
		mode?: "search" | "theme";
		/** The row the keys or the pointer are on, for the host's preview and footer. */
		selected?: SearchRow | null;
		onClose: () => void;
	}

	let { query = $bindable(), here, mode = $bindable("search"), selected = $bindable(null), onClose }: Props = $props();

	let listEl: HTMLDivElement | null = $state(null);
	let selectedIndex = $state(0);

	onMount(() => {
		if (pagesStore.pages.length === 0 && !pagesStore.pagesLoading) pagesStore.loadPages();
		if (chatSessions.sessions.length === 0 && !chatSessions.isLoading) chatSessions.load();
		if (projectStore.projects.length === 0 && !projectStore.loading) projectStore.load();
		loadRecentSources();
		void entities.load().catch(() => {
			// People and places are left out of this search; the rest still works.
		});
		// The frecency prior, refreshed at most once a minute.
		visits.refresh();
		void canBrowserPane().then((ok) => (browserAvailable = ok));
		void getAssistantProfile<{ assistant_name?: string | null }>()
			.then((p) => (assistantName = p.assistant_name?.trim() || null))
			.catch(() => {
				// Settings' titles say "your assistant" instead.
			});
		// Leaving mid-preview puts back the theme you had.
		return () => {
			if (originalTheme) applyTheme(originalTheme);
		};
	});

	// ── Themes ──────────────────────────────────────────────────────────────
	const themes = getAvailableThemes();
	let originalTheme: ThemeChoice | null = null;
	const shownThemes = $derived(
		mode === "theme" && query.trim()
			? themes.filter((t) => getThemeDisplayName(t).toLowerCase().includes(query.trim().toLowerCase()))
			: themes,
	);

	function enterThemeMode() {
		originalTheme = getThemeChoice();
		query = "";
		mode = "theme";
		selectedIndex = Math.max(0, themes.indexOf(originalTheme));
		applyTheme(themes[selectedIndex]);
	}

	/** Back to search, putting back the theme you had. */
	function exitThemeMode() {
		if (originalTheme) applyTheme(originalTheme);
		originalTheme = null;
		query = "";
		mode = "search";
	}

	function saveTheme(theme: ThemeChoice | undefined) {
		if (!theme) return;
		setTheme(theme);
		originalTheme = null;
		onClose();
	}

	function previewTheme(index: number) {
		selectedIndex = index;
		const theme = shownThemes[index];
		if (theme) applyTheme(theme);
	}

	// ── Search ──────────────────────────────────────────────────────────────
	// The + menu's commands, so search and the menu offer the same things the
	// same way, plus the ones only search has.
	let browserAvailable = $state(false);
	const commands = $derived<Command[]>([
		...commandsFromMenu(newTabMenuItems(undefined, { browser: browserAvailable })),
		{ id: "go-sources", label: "Go to Sources", icon: "atlas:sources", run: () => void windowShellStore.openTabFromRoute("/sources") },
		{ id: "go-settings", label: "Go to Settings", icon: "atlas:settings", shortcut: shortcuts.format("mod+,"), run: () => void windowShellStore.openTabFromRoute("/virtues", { label: "Settings", focusExisting: true }) },
		// The terminal left Settings' list; this is its door.
		{ id: "open-terminal", label: "Open a terminal on your server", icon: "atlas:terminal", run: () => void windowShellStore.openTabFromRoute("/virtues/developer/terminal", { label: "Terminal" }) },
		{ id: "change-theme", label: "Change theme", icon: "mynaui:swatches", run: enterThemeMode, keepOpen: true },
	]);

	const parsed = $derived(parseQuery(query));

	// Matches inside records live on the box and need a round trip, so they
	// arrive after the rest and are drawn last (see `searchGroups`).
	let contentHits = $state<LocalSearchHit[]>([]);
	let contentLoading = $state(false);
	let contentSeq = 0;

	$effect(() => {
		const { scope, text } = parsed;
		if (mode === "theme" || scope || text.length < 2) {
			contentHits = [];
			contentLoading = false;
			return;
		}
		const seq = ++contentSeq;
		const controller = new AbortController();
		contentLoading = true;
		// 180ms: long enough that ordinary typing issues one request per pause
		// rather than one per keystroke, short enough to feel immediate.
		const timer = setTimeout(async () => {
			try {
				const res = await searchLocal(text, { limit: 5, signal: controller.signal });
				// A slower earlier request must not overwrite a newer one's results.
				if (seq === contentSeq) contentHits = res.hits;
			} catch (err) {
				if ((err as Error)?.name !== "AbortError" && seq === contentSeq) contentHits = [];
			} finally {
				if (seq === contentSeq) contentLoading = false;
			}
		}, 180);
		return () => {
			clearTimeout(timer);
			controller.abort();
		};
	});

	// Bookmarks match on the box, by title, address, description, note and
	// tags. Under `#bookmarks` (or `#tags`) they are the whole list, newest
	// first while the field is empty.
	let bookmarkHits = $state<BookmarkHit[]>([]);
	let bookmarkSeq = 0;

	$effect(() => {
		const { scope, text } = parsed;
		const scoped = scope === "bookmark";
		if (mode === "theme" || (scope && !scoped) || (!scoped && text.length < 2)) {
			bookmarkHits = [];
			return;
		}
		const seq = ++bookmarkSeq;
		const timer = setTimeout(async () => {
			try {
				const named = await searchBookmarks(text, scoped ? SCOPED_BOOKMARK_ROWS : BOOKMARK_ROWS);
				if (seq === bookmarkSeq) bookmarkHits = named;
			} catch {
				// absent-ok: search still lists everything else; Bookmarks has its own search.
				if (seq === bookmarkSeq) bookmarkHits = [];
			}
		}, 180);
		return () => clearTimeout(timer);
	});

	// The assistant's name, for settings' titles ("What Ari can do").
	let assistantName = $state<string | null>(null);

	const groups = $derived(searchGroups({ query, here, commands, content: contentHits, bookmarks: bookmarkHits, assistantName }));
	/** Every row in drawing order, which is what the keys move through. */
	const rows = $derived(groups.flatMap((g) => g.rows));
	/** Each group's first row's index in `rows`. */
	const starts = $derived(groups.map((_g, i) => groups.slice(0, i).reduce((n, g) => n + g.rows.length, 0)));
	const count = $derived(mode === "theme" ? shownThemes.length : rows.length);

	// A new query starts on its first row, which is always the likeliest:
	// Open for an address, Ask for a question, otherwise the best match.
	$effect(() => {
		void query;
		if (mode === "search") selectedIndex = 0;
	});

	$effect(() => {
		selected = mode === "search" ? (rows[selectedIndex] ?? null) : null;
	});

	// Keep the selected row in view as the keys move it.
	$effect(() => {
		listEl?.querySelector(`[data-index="${selectedIndex}"]`)?.scrollIntoView({ block: "nearest" });
	});

	function run(row: SearchRow) {
		switch (row.kind) {
			case "open":
				void openWebLink(row.url);
				break;
			case "ask":
				askVirtues(row.query);
				break;
			case "command":
				void row.command.run();
				if (row.command.keepOpen) return;
				break;
			// A row with a preview opens with the hand-off, which closes search itself.
			case "record":
				return handOff(() => windowShellStore.openTabFromRoute(row.record.url, { label: row.record.title, focusExisting: true }));
			case "day":
				return handOff(() => windowShellStore.openTabFromRoute(rowRoute(row)!, { label: row.day.title, focusExisting: true }));
			case "entity":
				return handOff(() => windowShellStore.openTabFromRoute(row.entity.url, { label: row.entity.name, focusExisting: true }));
			case "content": {
				const target = contentTarget(row.hit);
				return handOff(() => windowShellStore.openTabFromRoute(target.route, { label: target.label }));
			}
			case "bookmark":
				return handOff(() => windowShellStore.openTabFromRoute(rowRoute(row)!, { label: row.bookmark.crumb, focusExisting: true }));
			case "setting":
				// Two settings work right here; the rest open on their page.
				if (row.setting.id === "theme") return enterThemeMode();
				if (row.setting.id === "reduce-motion") {
					// The device already keeps the app still; there is nothing to switch.
					if (!motion.device) motion.set(motion.setting === "reduce" ? "device" : "reduce");
					return;
				}
				openSettingInTab(row.setting.id, assistantName ?? "");
				break;
		}
		onClose();
	}

	/** What Return does to a setting row that works in place. */
	function settingNote(id: string): string | null {
		if (id === "theme") return "Choose";
		if (id === "reduce-motion") return motion.device ? "On by your device" : motion.reduced ? "On" : "Off";
		return null;
	}

	/** The host's field passes its keys here. */
	export function keydown(e: KeyboardEvent) {
		const move = (to: number) => {
			e.preventDefault();
			const next = Math.max(0, Math.min(to, count - 1));
			if (mode === "theme") previewTheme(next);
			else selectedIndex = next;
		};
		if (e.key === "Escape") {
			e.preventDefault();
			if (mode === "theme") exitThemeMode();
			else onClose();
		} else if (e.key === "ArrowDown") {
			move(selectedIndex + 1);
		} else if (e.key === "ArrowUp") {
			move(selectedIndex - 1);
		} else if (e.key === "Enter") {
			e.preventDefault();
			if (mode === "theme") saveTheme(shownThemes[selectedIndex]);
			else if (rows[selectedIndex]) run(rows[selectedIndex]);
		} else if (e.key === "Tab") {
			// Its default would move focus out of the field and strand search.
			e.preventDefault();
		}
	}

	const RECORD_GLYPH = { chat: "atlas:chats", page: "atlas:pages", project: "atlas:projects", applet: "atlas:applets" };
</script>

{#snippet row(item: SearchRow, index: number)}
	<button
		class="row"
		class:two-line={item.kind === "content" && !!previewOf(item.hit)}
		class:selected={selectedIndex === index}
		data-index={index}
		onclick={() => run(item)}
		onpointermove={() => {
			if (selectedIndex !== index) selectedIndex = index;
		}}
	>
		{#if item.kind === "open"}
			<Icon icon="mynaui:globe" width="16" class="row-icon" />
			<span class="row-label">Open <span class="muted">{item.url.replace(/^https?:\/\//i, "")}</span></span>
		{:else if item.kind === "ask"}
			<Icon icon="atlas:new-chat" width="16" class="row-icon" />
			<span class="row-label">Ask <span class="muted">{item.query}</span></span>
		{:else if item.kind === "command"}
			<Icon icon={item.command.icon} width="16" class="row-icon" />
			<span class="row-label">{item.command.label}</span>
			{#if item.command.shortcut}<kbd>{item.command.shortcut}</kbd>{/if}
		{:else if item.kind === "record"}
			{#if item.record.kind === "thread"}
				<span class="glyph-slot"><ProjectGlyph project={item.record.project} size={16} /></span>
			{:else}
				<Icon icon={RECORD_GLYPH[item.record.kind]} width="16" class="row-icon" />
			{/if}
			<span class="row-label" class:muted={item.record.untitled}>{item.record.title}</span>
		{:else if item.kind === "day"}
			<Icon icon="atlas:calendar" width="16" class="row-icon" />
			<span class="row-label">{item.day.title}{#if item.day.hint}<span class="muted">&ensp;{item.day.hint}</span>{/if}</span>
		{:else if item.kind === "setting"}
			<Icon icon="atlas:settings" width="16" class="row-icon" />
			<span class="row-label">{titleOf(item.setting.title, assistantName)}<span class="muted">&ensp;{settingPlace(item.setting, assistantName)}</span></span>
			{#if settingNote(item.setting.id)}<span class="row-note">{settingNote(item.setting.id)}</span>{/if}
		{:else if item.kind === "entity"}
			<Icon icon={item.entity.kind === "person" ? "mynaui:user" : "mynaui:map-pin"} width="16" class="row-icon" />
			<span class="row-label">{item.entity.name}</span>
		{:else if item.kind === "bookmark"}
			{@const tags = item.bookmark.tags ?? []}
			{@const line = item.bookmark.line}
			<Icon icon="atlas:bookmarks" width="16" class="row-icon" />
			<span class="row-label">{line.title}<span class="muted">&ensp;{line.meta}{#if tags.length}&ensp;{tags.slice(0, 3).map((t) => `#${t}`).join(" ")}{/if}</span></span>
		{:else}
			<Icon icon={contentGlyph(item.hit)} width="16" class="row-icon" />
			<span class="row-text">
				<span class="row-label">{item.hit.title || "Untitled"}</span>
				{#if previewOf(item.hit)}<span class="row-preview">{previewOf(item.hit)}</span>{/if}
			</span>
		{/if}
		{#if rowNoun(item)}<span class="row-kind">{rowNoun(item)}</span>{/if}
	</button>
{/snippet}

{#if mode === "theme"}
	<div class="list" bind:this={listEl} role="listbox" aria-label="Themes">
		<div class="group-label">Choose a theme</div>
		{#each shownThemes as theme, i (theme)}
			<button
				class="row"
				class:selected={selectedIndex === i}
				data-index={i}
				onclick={() => saveTheme(theme)}
				onpointermove={() => {
					if (selectedIndex !== i) previewTheme(i);
				}}
			>
				<Icon icon={themeMetadata[theme].icon} width="16" class="row-icon" />
				<span class="row-label">{getThemeDisplayName(theme)}</span>
				<span class="row-note">{themeMetadata[theme].description}</span>
			</button>
		{/each}
	</div>
{:else if rows.length > 0}
	<div class="list" bind:this={listEl} role="listbox" aria-label="Results">
		{#each groups as group, gi}
			{#if group.label}<div class="group-label">{group.label}</div>{/if}
			{#each group.rows as item, i}
				{@render row(item, starts[gi] + i)}
			{/each}
		{/each}
	</div>
{:else if parsed.scope && parsed.text && !contentLoading}
	<div class="list">
		<p class="empty">No {parsed.scope}s match "{parsed.text}".</p>
	</div>
{/if}

<style>
	.list {
		padding: 4px;
		border-top: 1px solid var(--color-border);
	}

	.group-label {
		padding: 10px 8px 4px;
		font-size: 11px;
		font-weight: 500;
		color: var(--color-foreground-subtle);
		user-select: none;
	}

	.list > .group-label:first-child {
		padding-top: 6px;
	}

	/* Icon under the field's glyph, text under its words. */
	.row {
		display: flex;
		align-items: center;
		gap: 10px;
		width: 100%;
		height: 32px;
		padding: 0 8px;
		border: none;
		border-radius: var(--radius-md);
		background: transparent;
		color: var(--color-foreground);
		font-size: 13px;
		text-align: left;
		cursor: default;
	}

	.row.selected {
		background: var(--hover-bg);
	}

	.row.two-line {
		height: auto;
		min-height: 32px;
		padding-top: 6px;
		padding-bottom: 6px;
		align-items: flex-start;
	}

	:global(.row-icon),
	.glyph-slot {
		flex: none;
		color: var(--color-foreground-muted);
	}

	.glyph-slot {
		display: inline-flex;
	}

	/* Sized to its text, so a shortcut or note sits beside the name rather
	   than across a wide field. */
	.row-label {
		flex: 0 1 auto;
		min-width: 0;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.muted {
		color: var(--color-foreground-subtle);
	}

	.row-text {
		display: flex;
		flex: 0 1 auto;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}

	.row-preview {
		font-size: 12px;
		color: var(--color-foreground-subtle);
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	/* What the row is, at the right edge so the nouns line up down the list. */
	.row-kind {
		flex: none;
		margin-left: auto;
		padding-left: 8px;
		font-size: 12px;
		color: var(--color-foreground-subtle);
		white-space: nowrap;
	}

	.two-line .row-kind {
		align-self: center;
	}

	.row-note {
		display: flex;
		flex: none;
		align-items: center;
		gap: 4px;
		font-size: 12px;
		color: var(--color-foreground-subtle);
		white-space: nowrap;
	}

	.row kbd {
		flex: none;
		font-family: var(--font-sans);
		font-size: 11px;
		line-height: 1;
		padding: 3px 5px;
		border-radius: var(--radius-sm);
		background: var(--fill-bg);
		color: var(--color-foreground-subtle);
	}

	.empty {
		margin: 0;
		padding: 8px;
		font-size: 13px;
		color: var(--color-foreground-subtle);
	}
</style>
