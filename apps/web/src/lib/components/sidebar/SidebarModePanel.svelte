<script lang="ts">
	/**
	 * The sidebar while it's in a sub-navigation mode (Settings, Developer).
	 *
	 * The exit row is gone: the path mast now reads `Virtues / Settings`, and
	 * clicking the root leaves the mode. A mode used to be the one place in
	 * the app you left by a bespoke control that existed nowhere else; now
	 * every "you are inside something" state — a pinned project, a mode, and
	 * whatever we add next — is entered and left through the same breadcrumb.
	 * That is the whole reason the mast became a path.
	 *
	 * Sections open into one reused tab rather than a new one each time. A few
	 * minutes in Settings would otherwise leave a row of tabs to clean up.
	 */
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import type { SidebarMode } from '$lib/sidebar/modes';
	import AtlasIcon from './AtlasIcon.svelte';

	interface Props {
		mode: SidebarMode;
	}

	let { mode }: Props = $props();

	// Derived from the route, never stored — the same contract SubNav keeps.
	// A remembered `activeHref` only knows about clicks in here, so Back, a
	// deep link, or SettingsView rewriting a legacy route left the highlight
	// pointing at a section the pane had already left. Longest match wins, so
	// `/virtues/developer/terminal` picks Terminal over any shorter prefix;
	// nothing matches when the pane is showing something outside the mode
	// (entering a mode doesn't navigate), and then nothing is highlighted.
	const activeTab = $derived.by(() => {
		const pane = windowShellStore.activePane;
		if (!pane) return null;
		return pane.tabs.find((t) => t.id === pane.activeTabId) ?? null;
	});

	const activeHref = $derived.by(() => {
		const route = activeTab?.route;
		if (!route) return null;
		let best: string | null = null;
		for (const row of mode.rows) {
			const hit = route === row.href || route.startsWith(row.href + '/');
			if (hit && (best === null || row.href.length > best.length)) best = row.href;
		}
		return best;
	});

	function open(href: string, label: string) {
		// `navigate` swaps the active tab's content in place rather than opening
		// a new one, so moving through six settings sections leaves you with one
		// tab instead of six to close afterwards. It pushes onto that tab's
		// history too, so Back walks the sections.
		windowShellStore.navigate(href, { label: `${mode.title} · ${label}` });
	}
</script>

<div class="mode-panel">
	<nav class="mode-rows">
		{#each mode.rows as row, i (row.id)}
			<!-- A heading wherever the group changes. Rows of a group must be
			     adjacent (modes.ts says so); this renders whatever the list
			     actually is rather than re-sorting it, so a list that breaks the
			     rule shows the repeat instead of hiding it.

			     Falling OUT of a group (History at the foot of the wiki) draws a
			     rule instead of a heading: the row is set apart without being
			     given a category of one. -->
			{#if row.group !== mode.rows[i - 1]?.group}
				{#if row.group}
					<!-- A div, NOT an h3: `app.css` makes every h1-h6 in the app
					     serif, so a heading element here rendered the sidebar's
					     one serif word. The Home panel's own group labels
					     (Pinned, Projects, Today, Recent) are divs for the same
					     reason, and this matches their metrics exactly. -->
					<div class="mode-group" class:first={i === 0}>{row.group}</div>
				{:else if i > 0}
					<hr class="mode-rule" />
				{/if}
			{/if}
			<button
				type="button"
				class="mode-row"
				class:active={activeHref === row.href}
				onclick={() => open(row.href, row.label)}
			>
				<!-- The glyph is optional and Atlas-only. The rule it used to
				     break is design.md's "icons assist, labels lead — eight
				     identical 16px icons at FULL CONTRAST in a column is what
				     makes a sidebar look like every other sidebar"; the objection
				     is the full-contrast uniform column, not the picture. Dressed
				     by `.sidebar-icon` (muted, half opacity, full strength only
				     on hover and where you are) it is a hint, which is what the
				     rule asks for, and it is the same treatment the Home panel's
				     rows have always had.

				     The other half of the old objection stands and is why this
				     takes `glyph` rather than `icon`: eleven REMIX glyphs under
				     an Atlas rail tile was two icon families in one column. -->
				{#if row.glyph}
					<AtlasIcon name={row.glyph} size={15} />
				{/if}
				<span>{row.label}</span>
			</button>
		{/each}
	</nav>
</div>

<style>
	.mode-panel {
		display: flex;
		flex-direction: column;
		gap: 2px;
		/* No inset of its own — the panel body already insets. */
		padding: 0;
	}

	/* The Home panel's group label, to the pixel: 12px, subtle, sentence case,
	   no tracking and no caps. The sidebar has one way of naming a group of
	   rows and this is it — a second treatment would make Wiki's panel a
	   different piece of furniture from Home's, in the one part of the app a
	   user sees every minute.

	   It is an aid to scanning, not a row: quieter than the rows beside it, and
	   it must never read as clickable. Home's label IS a button (it folds its
	   group); these do not fold, so no hover, no cursor. */
	.mode-group {
		display: flex;
		align-items: center;
		height: 24px;
		margin-top: 12px;
		padding: 0 6px 0 12px;
		font-size: 12px;
		color: var(--color-foreground-subtle);
		user-select: none;
	}

	/* The panel body already insets from the top; the first group would
	   otherwise sit lower than the panel title it follows. */
	.mode-group.first {
		margin-top: 2px;
	}

	.mode-rule {
		margin: 10px 12px 6px;
		border: none;
		border-top: 1px solid var(--color-border-subtle);
	}

	.mode-row {
		display: flex;
		align-items: center;
		gap: var(--sidebar-interactive-gap);
		width: 100%;
		height: var(--sidebar-interactive-height);
		padding: 0 12px;
		border: none;
		border-radius: var(--sidebar-interactive-radius);
		background: none;
		cursor: pointer;
		text-align: left;
		font-size: var(--sidebar-interactive-font-size);
		color: var(--color-foreground-muted);
	}

	.mode-row :global(svg) {
		opacity: var(--sidebar-icon-opacity);
		transition: opacity var(--sidebar-transition-duration) ease;
	}

	.mode-row {
		font-weight: 500;
	}

	.mode-row:hover {
		background: var(--sidebar-hover-bg);
		color: var(--color-foreground);
	}

	.mode-row:hover :global(svg),
	.mode-row.active :global(svg) {
		opacity: 1;
	}

	.mode-row.active {
		background: var(--sidebar-active-bg);
		color: var(--color-foreground);
	}

	.mode-row:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

</style>
