<script lang="ts">
	/**
	 * The sidebar while it's in a sub-navigation mode (Settings, Developer).
	 *
	 * The exit row is gone: the path mast now reads `Virtues / Settings`, and
	 * clicking the root leaves the mode. A mode used to be the one place in
	 * the app you left by a bespoke control that existed nowhere else; now
	 * every "you are inside something" state — a pinned notebook, a mode, and
	 * whatever we add next — is entered and left through the same breadcrumb.
	 * That is the whole reason the mast became a path.
	 *
	 * Sections open into one reused tab rather than a new one each time. A few
	 * minutes in Settings would otherwise leave a row of tabs to clean up.
	 */
	import Icon from '$lib/components/Icon.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import type { SidebarMode } from '$lib/sidebar/modes';

	interface Props {
		mode: SidebarMode;
	}

	let { mode }: Props = $props();

	// Derived from the route, never stored — the same contract SubNav keeps.
	// A remembered `activeHref` only knows about clicks in here, so Back, a
	// deep link, or SettingsView rewriting a legacy route left the highlight
	// pointing at a section the pane had already left. Longest match wins, so
	// `/virtues/developer/telemetry` picks Telemetry over any shorter prefix;
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
		{#each mode.rows as row (row.id)}
			<button
				type="button"
				class="mode-row"
				class:active={activeHref === row.href}
				onclick={() => open(row.href, row.label)}
			>
				<Icon icon={row.icon} width="16" />
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
		padding: 0 6px;
	}

	.mode-row {
		display: flex;
		align-items: center;
		gap: var(--sidebar-interactive-gap);
		width: 100%;
		height: var(--sidebar-interactive-height);
		padding: 0 var(--sidebar-padding-left-base);
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
		background: color-mix(in srgb, var(--color-foreground) 9%, transparent);
		color: var(--color-foreground);
	}

	.mode-row:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

</style>
